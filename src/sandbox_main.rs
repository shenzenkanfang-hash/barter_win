//! Sandbox Mode Main Entry - 沙盒模式启动器
//!
//! 集成：模拟 WS + ShadowGateway 订单拦截 + ShadowRiskChecker 风控
//!
//! 运行:
//!   cargo run --bin sandbox -- --symbol POWERUSDT --fund 10000 --duration 300
//!   cargo run --bin sandbox -- --fast  # 快速模式

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};

use b_data_source::{DataFeeder, Tick};
use h_sandbox::{
    ShadowBinanceGateway, ShadowRiskChecker,
    historical_replay::StreamTickGenerator,
};
use f_engine::types::{OrderRequest, OrderType, Side};
use f_engine::RiskChecker;
use a_common::exchange::ExchangeAccount;

const DEFAULT_SYMBOL: &str = "HOTUSDT";
const DEFAULT_FUND: f64 = 10000.0;
const DEFAULT_DURATION: u64 = 300; // 5分钟

#[derive(Debug, Clone)]
struct SandboxConfig {
    symbol: String,
    initial_fund: Decimal,
    duration_secs: u64,
    fast_mode: bool,
    start_date: String,
    end_date: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            symbol: DEFAULT_SYMBOL.to_string(),
            initial_fund: dec!(10000),
            duration_secs: DEFAULT_DURATION,
            fast_mode: false,
            start_date: "2025-10-09".to_string(),
            end_date: "2025-10-11".to_string(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(false)
        )
        .with(LevelFilter::INFO)
        .init();

    // 解析命令行参数
    let config = parse_args();

    tracing::info!("========================================");
    tracing::info!("  沙盒模式启动器");
    tracing::info!("========================================");
    tracing::info!("配置:");
    tracing::info!("  品种: {}", config.symbol);
    tracing::info!("  时间段: {} -> {}", config.start_date, config.end_date);
    tracing::info!("  初始资金: {}", config.initial_fund);
    tracing::info!("  测试时长: {}s", config.duration_secs);
    tracing::info!("  模式: {}", if config.fast_mode { "快速" } else { "实时" });
    tracing::info!("========================================\n");

    // 1. 创建 DataFeeder
    let data_feeder = Arc::new(DataFeeder::new());
    tracing::info!("✅ 1. DataFeeder 创建成功");

    // 2. 创建 ShadowBinanceGateway（订单拦截/模拟成交）
    let gateway = ShadowBinanceGateway::with_default_config(config.initial_fund);
    let gateway = Arc::new(gateway);
    tracing::info!("✅ 2. ShadowGateway 创建成功 (初始资金: {})", config.initial_fund);

    // 3. 创建 ShadowRiskChecker
    let risk_checker = ShadowRiskChecker::new();
    tracing::info!("✅ 3. ShadowRiskChecker 创建成功");

    // 4. 从API拉取K线数据
    tracing::info!("✅ 4. 正在从币安API拉取历史K线...");
    let klines = fetch_klines_from_api(&config.symbol, &config.start_date, &config.end_date).await?;
    let kline_count = klines.len();
    tracing::info!("✅ 4. K线数据准备完成 ({} 根)", kline_count);

    // 5. 创建 TickGenerator
    let tick_gen = StreamTickGenerator::from_loader(config.symbol.clone(), klines.into_iter());
    let total_ticks = kline_count * 60; // 每K线60 ticks
    tracing::info!("✅ 5. TickGenerator 创建成功 (预计 {} ticks)", total_ticks);

    // 打印初始账户信息
    print_account_info(&gateway);

    // ========================================
    // 主循环：模拟 WS + 订单处理
    // ========================================
    tracing::info!("\n========================================");
    tracing::info!("  开始模拟交易循环");
    tracing::info!("========================================\n");

    let start = Instant::now();
    let mut tick_count = 0u64;
    let mut order_count = 0u64;
    let mut signal_count = 0u64;
    let mut last_order_tick = 0u64;

    // 策略参数
    let signal_interval = 10; // 每10个tick检测一次
    let max_orders = 10; // 最多10笔订单
    let mut last_signal_price = Decimal::ZERO;

    // 创建 ticker 用于实时模式延迟
    let tick_interval = if config.fast_mode {
        Duration::from_millis(0)
    } else {
        Duration::from_millis(16) // ~60fps
    };

    // 创建 channel 用于 TickGenerator 异步迭代
    let (tx, mut rx) = mpsc::channel::<Tick>(1000);

    // TickGenerator 运行在独立任务
    let tick_gen_handle = tokio::spawn(async move {
        let mut generator = tick_gen;
        while let Some(tick_data) = generator.next() {
            // 用 SimulatedTick 数据构建 KLine，模拟正常WS推送的完整K线数据
            let kline_1m = b_data_source::KLine {
                symbol: tick_data.symbol.clone(),
                period: b_data_source::Period::Minute(1),
                open: tick_data.open,
                high: tick_data.high,
                low: tick_data.low,
                close: tick_data.price,
                volume: tick_data.volume,
                timestamp: tick_data.kline_timestamp,
            };

            let tick = Tick {
                symbol: tick_data.symbol,
                price: tick_data.price,
                qty: tick_data.qty,
                timestamp: tick_data.timestamp,
                kline_1m: Some(kline_1m),
                kline_15m: None,
                kline_1d: None,
            };

            if tx.send(tick).await.is_err() {
                tracing::warn!("Tick channel closed");
                break;
            }
        }
        tracing::info!("TickGenerator 完成");
    });

    // 主循环：处理 tick
    let max_ticks = if config.fast_mode { total_ticks as u64 } else { u64::MAX };

    loop {
        // 检查超时
        if !config.fast_mode && start.elapsed().as_secs() >= config.duration_secs {
            tracing::info!("达到指定时长 {}s，退出", config.duration_secs);
            break;
        }

        // 检查是否收到 tick
        let tick = match tokio::time::timeout(tick_interval, rx.recv()).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                tracing::info!("Tick 流结束");
                break;
            }
            Err(_) => {
                // 超时，继续（快速模式）
                tokio::task::yield_now().await;
                continue;
            }
        };

        // 更新网关价格（用于计算未实现盈亏）
        gateway.update_price(&config.symbol, tick.price);
        tick_count += 1;

        // 策略信号检测
        if tick_count % signal_interval == 0 {
            signal_count += 1;

            // 简单策略：价格变化 > 0.1% 且订单数未满时下单
            let price_change = if last_signal_price.is_zero() {
                Decimal::ZERO
            } else {
                ((tick.price - last_signal_price) / last_signal_price).abs()
            };

            if price_change > dec!(0.001) && order_count < max_orders && tick_count - last_order_tick >= 50 {
                // 决定方向
                let side = if tick.price > last_signal_price {
                    Side::Buy
                } else {
                    Side::Sell
                };

                // 风控检查
                let order_req = OrderRequest {
                    symbol: config.symbol.clone(),
                    side: side.clone(),
                    order_type: OrderType::Market,
                    qty: dec!(0.01),
                    price: Some(tick.price),
                };

                // 前置风控检查
                let account = gateway.get_account().unwrap_or_else(|_| ExchangeAccount {
                    account_id: "UNKNOWN".to_string(),
                    total_equity: dec!(0),
                    available: dec!(0),
                    frozen_margin: dec!(0),
                    unrealized_pnl: dec!(0),
                    update_ts: 0,
                });

                if risk_checker.pre_check(&order_req, &account).pre_failed() {
                    tracing::warn!("[Tick {:04}] 风控拦截", tick_count);
                } else {
                    // 下单
                    match gateway.place_order(order_req.clone()) {
                        Ok(result) => {
                            order_count += 1;
                            last_order_tick = tick_count;
                            tracing::info!(
                                "[Tick {:04}] 📝 {} @ {} (qty: {}, price_change: {:.2}%)",
                                tick_count,
                                if side == Side::Buy { "买入" } else { "卖出" },
                                tick.price,
                                result.filled_qty,
                                price_change * dec!(100)
                            );
                        }
                        Err(e) => {
                            tracing::error!("[Tick {:04}] ❌ 下单失败: {:?}", tick_count, e);
                        }
                    }
                }

                last_signal_price = tick.price;
            }
        }

        // 推送 tick 到 DataFeeder（可选，用于其他组件订阅）
        data_feeder.push_tick(tick.clone());

        // 打印进度
        if tick_count % 500 == 0 {
            let elapsed = start.elapsed();
            let rate = tick_count as f64 / elapsed.as_secs_f64().max(0.001);
            tracing::info!(
                "进度: {} ticks | 速率: {:.0}/s | 订单: {} | 信号: {}",
                tick_count,
                rate,
                order_count,
                signal_count
            );

            // 每 500 ticks 打印账户信息
            print_account_brief(&gateway);
        }

        // 检查是否达到最大 tick 数
        if tick_count >= max_ticks {
            tracing::info!("达到最大 tick 数 {}，退出", max_ticks);
            break;
        }
    }

    // 等待 TickGenerator 完成
    let _ = tick_gen_handle.await;

    // ========================================
    // 输出测试结果
    // ========================================
    let elapsed = start.elapsed();

    tracing::info!("\n========================================");
    tracing::info!("  测试完成");
    tracing::info!("========================================");
    tracing::info!("耗时: {:.2}s", elapsed.as_secs_f64());
    tracing::info!("总 ticks: {}", tick_count);
    tracing::info!("触发信号: {}", signal_count);
    tracing::info!("成交订单: {}", order_count);
    tracing::info!("平均速率: {:.0} ticks/s", tick_count as f64 / elapsed.as_secs_f64().max(0.001));

    // 打印最终账户信息
    print_account_info(&gateway);

    // 测试 DataFeeder 查询
    tracing::info!("\n========================================");
    tracing::info!("  DataFeeder 查询测试");
    tracing::info!("========================================");
    let latest = data_feeder.ws_get_1m(&config.symbol);
    match latest {
        Some(kline) => {
            tracing::info!("✅ DataFeeder 查询成功");
            tracing::info!("  最新K线: O={} H={} L={} C={}",
                kline.open, kline.high, kline.low, kline.close);
        }
        None => {
            tracing::warn!("⚠️  DataFeeder 查询返回 None");
        }
    }

    tracing::info!("\n========================================");
    tracing::info!("  沙盒模式测试完成");
    tracing::info!("========================================");

    Ok(())
}

/// 解析命令行参数
fn parse_args() -> SandboxConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut config = SandboxConfig::default();

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--symbol" => {
                if i + 1 < args.len() {
                    config.symbol = args[i + 1].clone();
                }
            }
            "--fund" => {
                if i + 1 < args.len() {
                    if let Ok(f) = args[i + 1].parse::<f64>() {
                        config.initial_fund = Decimal::try_from(f).unwrap_or(dec!(10000));
                    }
                }
            }
            "--duration" => {
                if i + 1 < args.len() {
                    if let Ok(d) = args[i + 1].parse::<u64>() {
                        config.duration_secs = d;
                    }
                }
            }
            "--start" => {
                if i + 1 < args.len() {
                    config.start_date = args[i + 1].clone();
                }
            }
            "--end" => {
                if i + 1 < args.len() {
                    config.end_date = args[i + 1].clone();
                }
            }
            "--fast" => {
                config.fast_mode = true;
            }
            "--help" => {
                println!("沙盒模式启动器");
                println!();
                println!("用法: sandbox [选项]");
                println!("选项:");
                println!("  --symbol <品种>   测试品种 (默认: {})", DEFAULT_SYMBOL);
                println!("  --fund <金额>     初始资金 USDT (默认: {})", DEFAULT_FUND);
                println!("  --duration <秒>  测试时长 (默认: {})", DEFAULT_DURATION);
                println!("  --start <日期>    起始日期 YYYY-MM-DD (默认: {})", config.start_date);
                println!("  --end <日期>      结束日期 YYYY-MM-DD (默认: {})", config.end_date);
                println!("  --fast            快速模式 (无延迟)");
                println!("  --help            显示帮助");
                println!();
                println!("示例:");
                println!("  cargo run --bin sandbox -- --symbol HOTUSDT --start 2025-10-09 --end 2025-10-11 --fast");
                std::process::exit(0);
            }
            _ => {}
        }
    }

    config
}

/// 从币安API拉取历史K线
async fn fetch_klines_from_api(
    symbol: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<b_data_source::KLine>, Box<dyn std::error::Error>> {
    use b_data_source::Period;

    // 解析日期
    let start_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 00:00:00", start_date), "%Y-%m-%d %H:%M:%S"
    )?;
    let end_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 00:00:00", end_date), "%Y-%m-%d %H:%M:%S"
    )?;

    let start_ms = chrono::Utc.from_utc_datetime(&start_dt).timestamp_millis();
    let end_ms = chrono::Utc.from_utc_datetime(&end_dt).timestamp_millis();

    tracing::info!("从API拉取 {} {} -> {} ({} -> {})",
        symbol, start_date, end_date, start_ms, end_ms);

    // 分页拉取：币安API每次最多1000条
    let mut all_raw_klines: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut current_start = start_ms;
    let max_requests = 100;
    let page_limit = 1000;

    let client = reqwest::Client::new();

    for page in 0..max_requests {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval=1m&limit={}&startTime={}&endTime={}",
            symbol.to_uppercase(),
            page_limit,
            current_start,
            end_ms
        );

        let response = client.get(&url).send().await?.text().await?;
        let raw_klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&response)
            .map_err(|e| format!("JSON解析失败: {} | Body: {}", e, response))?;

        if raw_klines.is_empty() {
            break;
        }

        let page_count = raw_klines.len();
        all_raw_klines.extend(raw_klines);

        tracing::info!("第 {} 页: {} 条, 累计: {}", page + 1, page_count, all_raw_klines.len());

        if page_count < page_limit {
            break;
        }

        // 下一页
        if let Some(last) = all_raw_klines.last() {
            if let Some(close_time) = last.get(6).and_then(|v| v.as_i64()) {
                current_start = close_time + 1;
            } else {
                break;
            }
        }

        if page >= max_requests - 1 {
            tracing::warn!("已达到最大请求次数限制");
            break;
        }
    }

    if all_raw_klines.is_empty() {
        return Err("未获取到K线数据".into());
    }

    tracing::info!("共获取K线: {} 条，开始解析...", all_raw_klines.len());

    // 转换为内部 KLine 格式
    let klines: Vec<b_data_source::KLine> = all_raw_klines
        .into_iter()
        .filter_map(|arr| {
            let open_time_ms = arr.get(0)?.as_i64()?;
            let timestamp = chrono::Utc.timestamp_millis_opt(open_time_ms).single()?;

            let parse_decimal = |idx: usize| -> Option<Decimal> {
                let s = arr.get(idx)?.as_str()?;
                let f: f64 = s.parse().ok()?;
                Decimal::from_f64_retain(f)
            };

            Some(b_data_source::KLine {
                symbol: symbol.to_string(),
                period: Period::Minute(1),
                open: parse_decimal(1)?,
                high: parse_decimal(2)?,
                low: parse_decimal(3)?,
                close: parse_decimal(4)?,
                volume: parse_decimal(5)?,
                timestamp,
            })
        })
        .collect();

    Ok(klines)
}

/// 打印账户详细信息
fn print_account_info(gateway: &Arc<ShadowBinanceGateway>) {
    match gateway.get_account() {
        Ok(account) => {
            tracing::info!("----------------------------------------");
            tracing::info!("账户信息:");
            tracing::info!("  总权益: {}", account.total_equity);
            tracing::info!("  可用余额: {}", account.available);
            tracing::info!("  冻结保证金: {}", account.frozen_margin);
            tracing::info!("  未实现盈亏: {}", account.unrealized_pnl);
            tracing::info!("----------------------------------------");
        }
        Err(e) => {
            tracing::error!("获取账户信息失败: {:?}", e);
        }
    }

    match gateway.get_position(&DEFAULT_SYMBOL) {
        Ok(Some(pos)) => {
            tracing::info!("持仓信息:");
            tracing::info!("  多头: {} @ {}", pos.long_qty, pos.long_avg_price);
            tracing::info!("  空头: {} @ {}", pos.short_qty, pos.short_avg_price);
            tracing::info!("  未实现盈亏: {}", pos.unrealized_pnl);
        }
        Ok(None) => {
            tracing::info!("无持仓");
        }
        Err(e) => {
            tracing::error!("获取持仓信息失败: {:?}", e);
        }
    }
}

/// 打印账户简要信息
fn print_account_brief(gateway: &Arc<ShadowBinanceGateway>) {
    if let Ok(account) = gateway.get_account() {
        tracing::info!(
            "账户 | 权益: {} | 可用: {} | 浮盈: {}",
            account.total_equity,
            account.available,
            account.unrealized_pnl
        );
    }
}
