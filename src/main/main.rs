//! Trading System v7.0 - 指标数据自驱动协程架构

mod actors;     // StrategyActor + RiskActor
mod components; // SystemComponents 构造器
mod event_bus;  // PipelineBus
mod pipeline;   // 流水线编排
mod utils;       // 工具函数

use std::path::PathBuf;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use a_common::logs::{JsonLinesWriter, JsonLinesLayer};
use crate::components::{create_components, init_heartbeat, print_heartbeat_report};
use crate::event_bus::PipelineBus;
use crate::pipeline::run_pipeline;
use crate::utils::{DATA_FILE, SYMBOL};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志目录 + 启动 JSON Lines Writer
    let log_dir = std::env::var("LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./logs"));
    a_common::logs::init_log_dir(log_dir.clone());
    // 注册全局单例，启动后台写入任务
    let _writer = JsonLinesWriter::new();
    tracing::info!("[JsonLinesWriter] log_dir={}, started", log_dir.display());

    // 2. 初始化 tracing subscriber（带 JSON Lines layer）
    tracing_subscriber::registry()
        .with(JsonLinesLayer::build())
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("=== Trading System v7.0 | 指标数据自驱动 | {} | {} ===", SYMBOL, DATA_FILE);
    init_heartbeat();

    // 3. 创建所有共享组件（Send-safe SystemComponents + 非 Send DataLayer）
    let (components, data_layer) = create_components().await?;

    // 4. 创建 PipelineBus（仅含策略信号/订单 channel）
    let bus = PipelineBus::new(128, 128);

    // 5. 启动自驱动流水线（spawn StrategyActor + RiskActor）
    run_pipeline(components, data_layer, bus).await?;

    // 6. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}
