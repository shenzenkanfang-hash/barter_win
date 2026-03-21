//! Trading System Rust Version - Main Entry
//!
//! High-performance trading system based on Barter-rs architecture

use a_common::config::Paths;
use b_data_source::BinanceMultiStream;
use std::fs;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Trading system starting");

    // 使用平台自适应路径配置
    let paths = Paths::new();
    let platform = paths.platform();

    // 3 output files - 使用平台自适应路径
    let base_dir = if platform.is_windows() {
        "E:/logs".to_string()
    } else {
        "data/logs".to_string()
    };

    // 确保目录存在
    fs::create_dir_all(&base_dir)?;

    let trade_path = format!("{}/trade.log", base_dir);
    let kline_path = format!("{}/kline.log", base_dir);
    let depth_path = format!("{}/depth.log", base_dir);

    tracing::info!("Platform: {:?}", platform);
    tracing::info!("Output directory: {}", base_dir);

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
