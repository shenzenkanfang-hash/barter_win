//! Trading System Rust Version - Main Entry
//!
//! 初始化流程:
//! 1. 从交易所拉取交易规则
//! 2. 订阅 1m K线 WS (分片: 50个/批, 500ms间隔)
//! 3. 订阅 1d K线 WS (分片: 50个/批, 500ms间隔)
//! 4. 订阅 Depth 订单簿 WS (仅 BTC 维护连接)
//! 5. 定时打印账户余额
//!
//! 测试模式: --test-rate-limit

use b_data_source::{BinanceApiGateway, Kline1mStream, Kline1dStream, DepthStream, Paths, FuturesDataSyncer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 检查是否为测试模式
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--test-rate-limit".to_string()) {
        return run_rate_limit_test().await;
    }

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)   // 显示 target
                .with_level(true)   // 显示日志级别
                .with_thread_ids(false) // 不显示线程ID
        )
        .with(LevelFilter::WARN)  // 只显示警告和错误
        .init();

    tracing::info!("Trading system starting");

    let paths = Paths::new();
    tracing::info!("Platform: {:?}", paths.platform());
    tracing::info!("Memory backup: {}", paths.memory_backup_dir);

    // 1. 从交易所拉取交易规则（同时保存原始 JSON 到 symbols_rules/）
    let mut gateway = BinanceApiGateway::new();
    let all_symbols = gateway.fetch_and_save_all_usdt_symbol_rules().await?;

    let trading_symbols: Vec<String> = all_symbols
        .iter()
        .map(|s| s.symbol.clone())
        .collect();

    tracing::info!("Found {} USDT trading pairs", trading_symbols.len());

    // 2. 启动 1m K线 WS 订阅 (分片: 50个/批, 500ms间隔)
    tracing::info!("Starting 1m KLine WS subscription...");
    let mut kline_1m_stream = Kline1mStream::new(trading_symbols.clone()).await?;
    tracing::info!("1m KLine WS subscription started");

    // 短暂等待后启动 1d
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // 3. 启动 1d K线 WS 订阅 (分片: 50个/批, 500ms间隔)
    tracing::info!("Starting 1d KLine WS subscription...");
    let mut kline_1d_stream = Kline1dStream::new(trading_symbols).await?;
    tracing::info!("1d KLine WS subscription started");

    // 短暂等待后启动 Depth
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 4. 启动 Depth 订单簿 WS (仅 BTC)
    tracing::info!("Starting Depth WS subscription (BTC only)...");
    let mut depth_stream = DepthStream::new_btc_only().await?;
    tracing::info!("Depth WS subscription started");

    // 5. 初始化账户数据同步器 (实盘行情 + 测试网账户)
    let account_syncer = FuturesDataSyncer::new();
    tracing::info!("Account syncer initialized (market: fapi.binance.com, account: testnet.binancefuture.com)");

    // 主循环：交替处理三个流
    let mut count_1m = 0;
    let mut count_1d = 0;
    let mut count_depth = 0;
    let mut account_print_flag = false;

    loop {
        tokio::select! {
            msg_1m = kline_1m_stream.next_message() => {
                if let Some(_msg) = msg_1m {
                    count_1m += 1;
                    if count_1m % 1000 == 0 {
                        tracing::info!("1m: Processed {} messages", count_1m);
                    }
                } else {
                    tracing::warn!("1m Stream ended");
                    break;
                }
            }
            msg_1d = kline_1d_stream.next_message() => {
                if let Some(_msg) = msg_1d {
                    count_1d += 1;
                    if count_1d % 1000 == 0 {
                        tracing::info!("1d: Processed {} messages", count_1d);
                    }
                } else {
                    tracing::warn!("1d Stream ended");
                    break;
                }
            }
            msg_depth = depth_stream.next_message() => {
                if let Some(_msg) = msg_depth {
                    count_depth += 1;
                    if count_depth % 1000 == 0 {
                        tracing::info!("Depth: Processed {} messages", count_depth);
                    }
                } else {
                    tracing::warn!("Depth Stream ended");
                    break;
                }
            }
            _ = async {
                if !account_print_flag {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            } => {
                // 5秒后打印账户信息
                if !account_print_flag {
                    println!("========== 账户信息查询 ==========");
                    match account_syncer.fetch_account().await {
                        Ok(account) => {
                            println!("总保证金: {} USDT", account.total_margin_balance);
                            println!("可用余额: {} USDT", account.available);
                            println!("未实现盈亏: {} USDT", account.unrealized_pnl);
                            println!("有效保证金: {} USDT", account.effective_margin);
                        }
                        Err(e) => {
                            println!("获取账户信息失败: {:?}", e);
                        }
                    }

                    // 获取持仓
                    match account_syncer.fetch_positions().await {
                        Ok(positions) => {
                            if positions.is_empty() {
                                println!("当前无持仓");
                            } else {
                                for pos in &positions {
                                    println!(
                                        "{} {} 数量:{} 杠杆:{}x",
                                        pos.symbol, pos.side, pos.qty, pos.leverage
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            println!("获取持仓信息失败: {:?}", e);
                        }
                    }
                    println!("==================================");
                    account_print_flag = true;
                }
            }
        }
    }

    Ok(())
}

/// ============================================================
/// Rate Limiter 测试模式
///
/// 测试 Binance API 限速机制：
/// 1. REQUEST_WEIGHT: 2400 次/分钟
/// 2. ORDERS: 1200 次/分钟
///
/// 运行方式: cargo run -- --test-rate-limit
/// ============================================================
async fn run_rate_limit_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("==============================================");
    println!("         Rate Limiter 测试模式");
    println!("==============================================");
    println!();

    // 创建合约 API 网关（会自动从 exchangeInfo 解析限制值）
    let mut gateway = BinanceApiGateway::new_futures();

    println!("[1] 创建网关成功，等待获取 exchangeInfo...");
    println!();

    // 发送一次 exchangeInfo 请求来获取和设置限制值
    let url = format!("{}/api/v3/exchangeInfo", gateway.market_api_base());
    let client = reqwest::Client::new();

    let resp = client
        .get(&url)
        .send()
        .await?;

    // 打印响应状态
    let status = resp.status();
    println!("[2] exchangeInfo 响应状态: {}", status);

    // 打印限速 header
    let headers = resp.headers();
    println!();
    println!("[3] 限速 Header 信息:");
    println!("----------------------------------------");

    if let Some(weight) = headers.get("x-mbx-used-weight-1m") {
        println!("  x-mbx-used-weight-1m: {:?}", weight.to_str().unwrap_or("N/A"));
    }
    if let Some(orders) = headers.get("x-mbx-order-count-1m") {
        println!("  x-mbx-order-count-1m: {:?}", orders.to_str().unwrap_or("N/A"));
    }
    if let Some(weight) = headers.get("x-mbx-used-weight") {
        println!("  x-mbx-used-weight: {:?}", weight.to_str().unwrap_or("N/A"));
    }
    if let Some(orders) = headers.get("x-mbx-order-count") {
        println!("  x-mbx-order-count: {:?}", orders.to_str().unwrap_or("N/A"));
    }

    println!("----------------------------------------");
    println!();

    // 解析 exchangeInfo 获取限制值
    let body_text = resp.text().await?;
    let info: a_common::api::BinanceExchangeInfo = serde_json::from_str(&body_text)?;

    println!("[4] 从 exchangeInfo 解析的限速规则:");
    for limit in &info.rate_limits {
        println!(
            "  - {}: {}/{}次，上限 = {}",
            limit.rate_limit_type, limit.interval, limit.interval_num, limit.limit
        );
    }
    println!();

    // 设置限速器
    gateway.rate_limiter.lock().set_limits(&info);

    // 获取当前限速状态
    let limiter = gateway.rate_limiter.lock();
    let (weight_rate, orders_rate) = limiter.usage_rate();
    let near_limit = limiter.is_near_limit();
    drop(limiter);

    println!("[5] 限速器初始状态:");
    println!("  - REQUEST_WEIGHT 使用率: {:.1}%", weight_rate * 100.0);
    println!("  - ORDERS 使用率: {:.1}%", orders_rate * 100.0);
    println!("  - 是否接近限制 (80%): {}", near_limit);
    println!();

    // 连续发送多次请求测试限速
    println!("[6] 开始连续请求测试 (发送 5 次请求)...");
    println!("----------------------------------------");

    for i in 1..=5 {
        let start = Instant::now();

        // 调用限速器的 acquire (虽然 gateway 是 mut，但我们测试的是限速器本身)
        // 由于内部获取锁的方式，我们需要创建临时请求来测试

        let test_resp = client
            .get(&url)
            .send()
            .await?;

        let elapsed = start.elapsed();
        let test_headers = test_resp.headers();

        println!("请求 #{}: 状态={}, 耗时={:?}", i, test_resp.status(), elapsed);

        if let Some(weight) = test_headers.get("x-mbx-used-weight-1m") {
            println!("  x-mbx-used-weight-1m: {}", weight.to_str().unwrap_or("N/A"));
        }
        if let Some(orders) = test_headers.get("x-mbx-order-count-1m") {
            println!("  x-mbx-order-count-1m: {}", orders.to_str().unwrap_or("N/A"));
        }

        // 更新限速器状态
        gateway.rate_limiter.lock().update_from_headers(test_headers);

        let limiter = gateway.rate_limiter.lock();
        let (wr, or) = limiter.usage_rate();
        let nl = limiter.is_near_limit();
        println!("  当前使用率: WEIGHT={:.1}%, ORDERS={:.1}%, near_limit={}", wr * 100.0, or * 100.0, nl);
        println!();

        // 如果接近限制，显示警告
        if nl {
            println!("  ⚠️ 警告: 接近限速阈值！");
        }
    }

    println!("----------------------------------------");
    println!();

    // 显示最终状态
    let limiter = gateway.rate_limiter.lock();
    let (weight_rate, orders_rate) = limiter.usage_rate();
    let near_limit = limiter.is_near_limit();

    println!("[7] 最终限速状态:");
    println!("  - REQUEST_WEIGHT 使用率: {:.1}%", weight_rate * 100.0);
    println!("  - ORDERS 使用率: {:.1}%", orders_rate * 100.0);
    println!("  - 是否接近限制 (80%): {}", near_limit);

    // 获取系统配置快照
    let config = limiter.to_system_config();
    println!();
    println!("[8] 系统配置快照:");
    println!("  - REQUEST_WEIGHT 限制: {}", config.request_weight_limit);
    println!("  - ORDERS 限制: {}", config.orders_limit);
    println!("  - 已用 WEIGHT: {}", config.used_weight);
    println!("  - 已用 ORDERS: {}", config.used_orders);
    println!("  - 窗口开始时间戳: {} 秒", config.window_start_ts);
    println!("  - 更新时间: {}", config.updated_at);

    drop(limiter);

    println!();
    println!("==============================================");
    println!("         测试完成");
    println!("==============================================");

    Ok(())
}
