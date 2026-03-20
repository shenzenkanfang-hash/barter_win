//! Binance 实时数据测试
//!
//! 连接 Binance 测试网 WebSocket 获取实时 Tick 数据

use market::{BinanceTradeStream, BinanceWsConnector};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("============================================");
    println!("Binance 测试网实时数据连接测试");
    println!("============================================");
    println!("API: https://testnet.binancefuture.com");
    println!("WebSocket: wss://stream.binancefuture.com/ws/btcusdt@trade");
    println!("============================================\n");

    tracing::info!("连接到 Binance 测试网...");

    // 创建 Binance WebSocket 连接器
    let connector = BinanceWsConnector::new("BTCUSDT");
    let mut stream = connector.connect().await?;

    tracing::info!("连接成功! 开始接收实时数据...\n");

    // 接收并显示前 20 个 tick
    let mut count = 0;
    while count < 20 {
        if let Some(tick) = stream.next_tick().await {
            count += 1;
            println!(
                "[{:03}] Tick | symbol: {} | price: {} | qty: {} | time: {}",
                count,
                tick.symbol,
                tick.price,
                tick.qty,
                tick.timestamp
            );
        }
        // 小延迟避免过于密集
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\n============================================");
    println!("测试完成! 共接收 {} 个 tick", count);
    println!("============================================");

    Ok(())
}
