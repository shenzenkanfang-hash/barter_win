//! Trading System v7.0 - 事件驱动协程自治架构

mod components;
mod pipeline;
mod tick_context;
mod utils;
mod event_bus;  // 新增: PipelineBus
mod actors;     // 新增: StrategyActor + RiskActor

use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::components::{create_components, init_heartbeat, print_heartbeat_report};
use crate::event_bus::PipelineBus;
use crate::pipeline::run_pipeline;
use crate::tick_context::{DATA_FILE, SYMBOL};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("=== Trading System v7.0 | Event-Driven | {} | {} ===", SYMBOL, DATA_FILE);
    init_heartbeat();

    // 1. 创建所有共享组件（Send-safe SystemComponents + 非 Send DataLayer）
    let (components, data_layer) = create_components().await?;

    // 2. 创建 PipelineBus（仅含策略信号/订单 channel）
    let bus = PipelineBus::new(128, 128);

    // 3. 启动事件驱动流水线（spawn StrategyActor + RiskActor）
    run_pipeline(components, data_layer, bus).await?;

    // 4. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}
