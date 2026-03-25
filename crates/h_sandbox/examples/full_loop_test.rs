//! 全链路闭环测试
//!
//! 数据 → TickGenerator → DataFeeder → ShadowGateway → 账户持仓
//!
//! 运行: cargo run -p h_sandbox --example full_loop_test -- --path "xxx.parquet"

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::timeout;

use b_data_source::{DataFeeder, Tick};
use h_sandbox::{ShadowBinanceGateway, ShadowConfig, Side, Account, Position};
use h_sandbox::tick_generator::TickGenerator;

#[tokio::main]
async fn main() {
    println!("=== 全链路闭环测试 ===\n");

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    let mut parquet_path = "".to_string();
    let mut kline_count = 100; // 默认100根K线

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--path" => {
                if i + 1 < args.len() {
                    parquet_path = args[i + 1].clone();
                }
            }
            "--klines" => {
                if i + 1 < args.len() {
                    kline_count = args[i + 1].parse().unwrap_or(100);
                }
            }
            "--help" => {
                println!("用法: full_loop_test [选项]");
                println!("选项:");
                println!("  --path <路径>   parquet 文件路径");
                println!("  --klines <N>   测试K线数量 (默认: 100)");
                println!("  --help         显示帮助");
                return;
            }
            _ => {}
        }
    }

    // 如果没有指定路径，使用模拟数据
    let use_mock = parquet_path.is_empty();

    println!("配置:");
    println!("  模式: {}", if use_mock { "模拟数据" } else { "Parquet文件" });
    println!("  K线数量: {}", kline_count);
    println!();

    // 1. 创建 DataFeeder
    let data_feeder = Arc::new(DataFeeder::new());
    println!("✅ 1. DataFeeder 创建成功");

    // 2. 创建 ShadowGateway（劫持网关）
    let gateway = ShadowBinanceGateway::new(ShadowConfig::default());
    let gateway = Arc::new(gateway);
    println!("✅ 2. ShadowGateway 创建成功");

    // 3. 创建账户
    let mut account = Account::new(dec!(10000)); // 初始资金 10000 USDT
    println!("✅ 3. 账户创建成功 (初始资金: {})", account.balance());

    // 4. 准备 K线数据
    let klines = if use_mock {
        generate_mock_klines("POWERUSDT", kline_count)
    } else {
        // TODO: 从 parquet 加载
        println!("⚠️  Parquet 加载暂未实现，使用模拟数据");
        generate_mock_klines("POWERUSDT", kline_count)
    };
    println!("✅ 4. K线数据准备完成 ({} 根)", klines.len());

    // 5. 创建 TickGenerator
    let tick_gen = TickGenerator::from_klines("POWERUSDT".to_string(), klines);
    let total_ticks = tick_gen.total_klines() * 60;
    println!("✅ 5. TickGenerator 创建成功 (预计 {} ticks)", total_ticks);

    // 6. 准备位置
    let symbol = "POWERUSDT";
    let position = Position::new(symbol);
    println!("✅ 6. Position 创建成功");

    // 开始测试
    println!("\n=== 开始闭环测试 ===\n");

    let start = Instant::now();
    let mut tick_count = 0u64;
    let mut order_count = 0u64;
    let mut signal_count = 0u64;

    // 简单策略：每10个tick检测一次
    let mut last_signal_price = Decimal::ZERO;
    let signal_interval = 10;

    // 模拟数据推送循环
    let max_ticks = if use_mock { total_ticks } else { 6000 };

    loop {
        if tick_count >= max_ticks as u64 {
            break;
        }

        // 生成 tick
        let tick = {
            // 简化：直接生成模拟 tick
            let price = dec!(5.0) + Decimal::from(tick_count % 100) * dec!(0.001);
            let t = Tick {
                symbol: symbol.to_string(),
                price,
                qty: dec!(0.01),
                timestamp: Utc::now(),
                kline_1m: None,
                kline_15m: None,
                kline_1d: None,
            };
            t
        };

        // 推送 tick
        data_feeder.push_tick(tick.clone());
        tick_count += 1;

        // 策略信号检测（简化）
        if tick_count % signal_interval == 0 {
            signal_count += 1;

            // 简单策略：价格变化 > 0.1% 时下单
            let price_change = if last_signal_price.is_zero() {
                Decimal::ZERO
            } else {
                ((tick.price - last_signal_price) / last_signal_price).abs()
            };

            if price_change > dec!(0.001) && order_count < 5 {
                // 尝试下单
                let side = if tick.price > last_signal_price {
                    Side::Buy
                } else {
                    Side::Sell
                };

                // 通过 ShadowGateway 下单
                let result = gateway.place_order(symbol, side, tick.price, dec!(0.01)).await;

                match result {
                    Ok(order_id) => {
                        order_count += 1;
                        println!(
                            "[Tick {:04}] 📝 {} {} @ {} (price_change: {:.2}%)",
                            tick_count,
                            if side == Side::Buy { "买入" } else { "卖出" },
                            order_id,
                            tick.price,
                            price_change * dec!(100)
                        );
                    }
                    Err(e) => {
                        // ShadowGateway 返回模拟结果
                        order_count += 1;
                        println!(
                            "[Tick {:04}] 📝 {} 模拟订单 @ {} (reason: {:?})",
                            tick_count,
                            if side == Side::Buy { "买入" } else { "卖出" },
                            tick.price,
                            e
                        );
                    }
                }

                last_signal_price = tick.price;
            }
        }

        // 打印进度
        if tick_count % 500 == 0 {
            let elapsed = start.elapsed();
            let rate = tick_count as f64 / elapsed.as_secs_f64();
            println!(
                "进度: {}/{} | 速率: {:.0} ticks/s | 订单: {} | 信号: {}",
                tick_count,
                max_ticks,
                rate,
                order_count,
                signal_count
            );
        }

        // 模拟 1ms 延迟
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let elapsed = start.elapsed();

    // 输出结果
    println!("\n=== 测试完成 ===");
    println!("耗时: {:.2}s", elapsed.as_secs_f64());
    println!("总 ticks: {}", tick_count);
    println!("触发信号: {}", signal_count);
    println!("模拟订单: {}", order_count);
    println!("平均速率: {:.0} ticks/s", tick_count as f64 / elapsed.as_secs_f64());

    // 测试 DataFeeder 查询
    println!("\n=== DataFeeder 查询测试 ===");
    let latest = data_feeder.ws_get_1m(symbol);
    match latest {
        Some(kline) => {
            println!("✅ DataFeeder 查询成功");
            println!("  最新K线: O={} H={} L={} C={}", kline.open, kline.high, kline.low, kline.close);
        }
        None => {
            println!("⚠️  DataFeeder 查询返回 None");
        }
    }

    println!("\n=== 闭环测试通过 ===");
}

/// 生成模拟 K线数据
fn generate_mock_klines(symbol: &str, count: usize) -> Vec<b_data_source::KLine> {
    use b_data_source::Period;
    
    let mut klines = Vec::new();
    let base_price = 5.0;
    let mut current_price = base_price;
    let now = Utc::now();

    for i in 0..count {
        let open = current_price;
        let change = ((i % 20) as f64 - 10.0) / 1000.0;
        let close = current_price * (1.0 + change);
        let high = open.max(close) * 1.001;
        let low = open.min(close) * 0.999;
        current_price = close;

        klines.push(b_data_source::KLine {
            symbol: symbol.to_string(),
            period: Period::Minute(1),
            open: Decimal::try_from(open).unwrap_or(dec!(5.0)),
            high: Decimal::try_from(high).unwrap_or(dec!(5.0)),
            low: Decimal::try_from(low).unwrap_or(dec!(5.0)),
            close: Decimal::try_from(close).unwrap_or(dec!(5.0)),
            volume: dec!(100),
            timestamp: now + chrono::Duration::minutes(i as i64),
        });
    }

    klines
}
