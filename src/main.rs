//! Trading System Rust Version - Main Entry
//!
//! 初始化流程:
//! 1. 从交易所拉取交易规则
//! 2. 订阅 1m K线 WS (分片: 50个/批, 200ms间隔)

use b_data_source::{BinanceApiGateway, Kline1mStream, Paths};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Trading system starting");

    let paths = Paths::new();
    tracing::info!("Platform: {:?}", paths.platform());
    tracing::info!("Memory backup: {}", paths.memory_backup_dir);

    // 1. 从交易所拉取交易规则
    let gateway = BinanceApiGateway::new();
    let all_symbols = gateway.fetch_all_usdt_symbol_rules().await?;

    let trading_symbols: Vec<String> = all_symbols
        .iter()
        .map(|s| s.symbol.clone())
        .collect();

    tracing::info!("Found {} USDT trading pairs", trading_symbols.len());

    // 2. 启动 1m K线 WS 订阅 (自动分片: 50个/批, 200ms间隔)
    tracing::info!("Starting 1m KLine WS subscription...");
    let mut kline_stream = Kline1mStream::new(trading_symbols).await?;
    tracing::info!("1m KLine WS subscription started");

    // 主循环
    let mut count = 0;
    loop {
        if let Some(_msg) = kline_stream.next_message().await {
            count += 1;
            if count % 1000 == 0 {
                tracing::info!("Processed {} kline messages", count);
            }
        } else {
            tracing::warn!("Stream ended");
            break;
        }
    }

    Ok(())
}
