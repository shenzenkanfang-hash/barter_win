//! 量化交易系统 Rust 版 - 主入口
//!
//! 基于 Barter-rs 风格架构的高性能量化交易系统

use account::types::FundPool;
use engine::TradingEngine;
use market::{MockMarketConnector, MockMarketStream};
use rust_decimal::Decimal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("info".parse()?))
        .init();

    tracing::info!("量化交易系统启动");

    // 1. 创建市场数据流 (模拟)
    let mut connector = MockMarketConnector::new();
    connector.subscribe("BTCUSDT").await?;
    let market_stream = Box::new(MockMarketStream::new(
        "BTCUSDT".to_string(),
        Decimal::try_from(50000.0).unwrap(),
    ));

    // 2. 创建初始资金池
    let fund_pool = FundPool {
        total_equity: Decimal::try_from(100000.0).unwrap(),
        available: Decimal::try_from(100000.0).unwrap(),
        positions_value: Decimal::try_from(0.0).unwrap(),
    };

    // 3. 创建交易引擎
    let mut engine = TradingEngine::new(
        market_stream,
        "BTCUSDT".to_string(),
        fund_pool,
    );

    // 4. 运行引擎 (模拟数据，运行 10 秒后退出)
    tracing::info!("开始模拟交易...");

    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 10 {
        if let Some(tick) = engine.market_stream.next_tick().await {
            engine.on_tick(&tick).await;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    tracing::info!("模拟交易结束");

    Ok(())
}
