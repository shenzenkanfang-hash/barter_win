//! Trading System Rust Version - Main Entry
//!
//! 初始化流程:
//! 1. 从交易所拉取交易规则
//! 2. 订阅 1m K线 WS (分片: 50个/批, 500ms间隔)
//! 3. 订阅 1d K线 WS (分片: 50个/批, 500ms间隔)
//! 4. 订阅 Depth 订单簿 WS (仅 BTC 维护连接)

use b_data_source::{BinanceApiGateway, Kline1mStream, Kline1dStream, DepthStream, Paths};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)   // 显示 target
                .with_level(true)   // 显示日志级别
                .with_thread_ids(true) // 显示线程ID
        )
        .with(LevelFilter::TRACE)  // 显示所有日志
        .init();

    tracing::info!("Trading system starting");

    let paths = Paths::new();
    tracing::info!("Platform: {:?}", paths.platform());
    tracing::info!("Memory backup: {}", paths.memory_backup_dir);

    // 1. 从交易所拉取交易规则（同时保存原始 JSON 到 symbols_rules/）
    let gateway = BinanceApiGateway::new();
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

    // 主循环：交替处理三个流
    let mut count_1m = 0;
    let mut count_1d = 0;
    let mut count_depth = 0;
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
        }
    }

    Ok(())
}
