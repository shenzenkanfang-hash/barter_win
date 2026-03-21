//! Trading System Rust Version - Main Entry
//!
//! High-performance trading system based on Barter-rs architecture

use b_data_source::BinanceMultiStream;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Trading system starting");

    // 3 output files (overwrite mode)
    let trade_path = "E:/logs/trade.log";
    let kline_path = "E:/logs/kline.log";
    let depth_path = "E:/logs/depth.log";

    // Create multi-stream writer for market data
    let symbols = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()];
    let mut multi_stream = BinanceMultiStream::new(trade_path, kline_path, depth_path, symbols).await?;

    tracing::info!("Connected to Binance WebSocket, streaming market data to 3 files");
    tracing::info!("Trade -> {}", trade_path);
    tracing::info!("Kline -> {}", kline_path);
    tracing::info!("Depth -> {}", depth_path);

    // Main loop - continuously read and write messages
    loop {
        if let Some(_msg) = multi_stream.next_message().await {
            // Message already written to file by MultiStreamWriter
        } else {
            tracing::warn!("Stream ended");
            break;
        }
    }

    tracing::info!("Trading system stopped");

    Ok(())
}
