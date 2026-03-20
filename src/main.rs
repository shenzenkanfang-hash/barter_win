//! 量化交易系统 Rust 版 - 主入口
//!
//! 基于 Barter-rs 风格架构的高性能量化交易系统

use engine::TradingEngine;
use market::{MarketConnector, MockMarketConnector, MockMarketStream};
use rust_decimal::Decimal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("量化交易系统启动");

    // 1. 创建市场数据流 (模拟)
    let mut connector = MockMarketConnector::new();
    connector.subscribe("BTCUSDT").await?;
    let market_stream = Box::new(MockMarketStream::new(
        "BTCUSDT".to_string(),
        Decimal::try_from(50000.0).unwrap(),
    ));

    // 2. 创建交易引擎 (初始资金 100000)
    let mut engine = TradingEngine::new(
        market_stream,
        "BTCUSDT".to_string(),
        Decimal::try_from(100000.0).unwrap(),
    );

    // 4. 运行引擎 (使用内部 run 方法，会在内部处理循环)
    tracing::info!("开始模拟交易...");

    // 使用 run_with_timeout 替代直接调用 run()
    engine.run_with_timeout(10).await;

    tracing::info!("模拟交易结束");

    Ok(())
}
