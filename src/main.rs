//! Trading System Rust Version - Main Entry
//!
//! 初始化流程:
//! 1. 从交易所拉取交易规则
//! 2. 订阅 1m K线 WS (分片: 50个/批, 500ms间隔)
//! 3. 订阅 1d K线 WS (分片: 50个/批, 500ms间隔)
//! 4. 订阅 Depth 订单簿 WS (仅 BTC 维护连接)
//! 5. 定时打印账户余额
//! 6. Rate Limiter 监控 (每 500ms 发送请求测试限流)

use b_data_source::{BinanceApiGateway, Kline1mStream, Kline1dStream, DepthStream, Paths, FuturesDataSyncer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)   // 显示 target
                .with_level(true)   // 显示日志级别
                .with_thread_ids(false) // 不显示线程ID
        )
        .with(LevelFilter::INFO)  // 显示 info/warn/error
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

    // 6. 初始化 Rate Limiter 测试客户端 (独立客户端用于监控)
    let rate_test_client = reqwest::Client::new();
    // 使用轻量级 API 测试限流：/fapi/v1/ticker/price
    let rate_test_url = "https://fapi.binance.com/fapi/v1/ticker/price?symbol=BTCUSDT".to_string();
    // 并发请求数量
    let concurrent_requests = 20;
    println!("Rate limiter test URL: {}", rate_test_url);
    println!("每 100ms 发送 {} 个并发请求\n", concurrent_requests);

    // 用于打印的计数器
    let mut rate_test_count = 0u64;

    // 主循环：交替处理三个流 + Rate Limiter 监控
    let mut count_1m = 0;
    let mut count_1d = 0;
    let mut count_depth = 0;
    let mut account_print_flag = false;

    println!("==============================================");
    println!("         Rate Limiter 监控已启动");
    println!("==============================================");
    println!("每 500ms 发送一次 exchangeInfo 请求");
    println!("观察是否被限流拦住 (等待/拒绝)");
    println!();

    loop {
        tokio::select! {
            // 1m K线消息
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
            // 1d K线消息
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
            // Depth 消息
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
            // 账户打印 (5秒后)
            _ = async {
                if !account_print_flag {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            } => {
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
            // Rate Limiter 监控 (每 100ms)
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                rate_test_count += 1;
                let start = Instant::now();

                // 并发发送多个请求
                let client = rate_test_client.clone();
                let url = rate_test_url.clone();
                let requests: Vec<_> = (0..concurrent_requests)
                    .map(|_| {
                        let client = client.clone();
                        let url = url.clone();
                        async move {
                            client.get(&url).send().await
                        }
                    })
                    .collect();

                let results = futures::future::join_all(requests).await;

                let elapsed = start.elapsed();

                // 获取任意一个响应的 header（它们应该相同）
                let mut all_success = true;
                let mut binance_weight = "-";
                let mut binance_orders = "-";

                for resp in &results {
                    match resp {
                        Ok(test_resp) => {
                            if binance_weight == "-" {
                                binance_weight = test_resp.headers()
                                    .get("x-mbx-used-weight-1m")
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or("-");
                                binance_orders = test_resp.headers()
                                    .get("x-mbx-order-count-1m")
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or("-");
                            }
                        }
                        Err(_) => {
                            all_success = false;
                        }
                    }
                }

                // 从网关获取限速器状态
                let limiter = gateway.rate_limiter().lock();
                let (weight_rate, _orders_rate) = limiter.usage_rate();
                let near_limit = limiter.is_near_limit();
                drop(limiter);

                // 判断是否被拦住 (响应慢 > 1s)
                let blocked = elapsed > std::time::Duration::from_secs(1);
                let status_str = if blocked { "⚠️ SLOW" } else { "OK" };
                let req_status = if all_success { "200" } else { "有失败" };

                // 打印：序号 | 耗时 | 请求数 | Binance累计权重 | 使用率% | 状态
                println!(
                    "#{:06} | {:.3}s | {}/{}成功 | binance累计={} | {:.0}% | {}",
                    rate_test_count,
                    elapsed.as_secs_f64(),
                    results.iter().filter(|r| r.is_ok()).count(),
                    concurrent_requests,
                    binance_weight,
                    weight_rate * 100.0,
                    status_str
                );

                // 如果超过 80% 阈值，显示警告
                if near_limit {
                    println!("       >>> 警告: 接近限流阈值! <<<");
                }
            }
        }
    }

    Ok(())
}
