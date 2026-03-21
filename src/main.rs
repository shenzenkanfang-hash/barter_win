//! Trading System Rust Version - Main Entry
//!
//! High-performance trading system based on Barter-rs architecture

use a_common::config::Paths;
use b_data_source::BinanceMultiStream;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Trading system starting");

    // 使用平台自适应路径配置 (约定的高速内存盘)
    let paths = Paths::new();
    let platform = paths.platform();
    let base_dir = &paths.memory_backup_dir;

    tracing::info!("Platform: {:?}", platform);
    tracing::info!("Memory backup directory: {}", base_dir);

    // 订阅的交易对
    let symbols = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()];

    // 创建多数据流 - 自动使用约定的高速内存盘路径
    // 数据将写入:
    //   - {base_dir}/trades/{symbol}.csv
    //   - {base_dir}/kline-1m-实时/{symbol}.json
    //   - {base_dir}/depth/{symbol}.json
    let mut multi_stream = BinanceMultiStream::new(symbols).await?;

    tracing::info!("Connected to Binance WebSocket, streaming market data to memory backup dir");

    // Main loop - continuously read and write messages
    loop {
        if let Some(_msg) = multi_stream.next_message().await {
            // Message already written to memory backup by MultiStreamWriter
        } else {
            tracing::warn!("Stream ended");
            break;
        }
    }

    tracing::info!("Trading system stopped");

    Ok(())
}
