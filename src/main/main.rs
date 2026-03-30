//! Trading System v7.0 - 指标数据自驱动协程架构

mod actors;     // StrategyActor + RiskActor
mod components; // SystemComponents 构造器
mod utils;       // 工具函数

use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use a_common::event_bus::{PipelineBus, PipelineBusHandle};
use a_common::logs::{JsonLinesWriter, JsonLinesLayer};
use crate::actors::{run_strategy_actor, run_risk_actor};
use crate::components::{create_components, init_heartbeat, print_heartbeat_report, DataLayer, SystemComponents};
use crate::utils::{DATA_FILE, SYMBOL};

// ============================================================================
// 流水线（从 pipeline.rs 内联）
// ============================================================================

/// 自驱动流水线启动函数
///
/// # 行为
/// 1. Spawn StrategyActor（主动驱动：拉取数据 → 处理 → 发信号）
/// 2. Spawn RiskActor（被动消费：等信号 → 风控 → 下单）
/// 3. 等待任一 actor 结束
/// 4. 广播停止信号
async fn run_pipeline(
    components: SystemComponents,
    data_layer: DataLayer,
    bus: (PipelineBusHandle, PipelineBus),
) -> Result<(), Box<dyn std::error::Error>> {
    let (bus_handle, bus_receiver) = bus;

    tracing::info!("Self-driven pipeline starting");

    // 创建 broadcast channel（Send-safe stop signal）
    let (stop_tx, _) = broadcast::channel::<()>(1);

    // Spawn StrategyActor（主动驱动方）
    let strat_stop_rx = stop_tx.subscribe();
    let strat_handle = tokio::spawn(run_strategy_actor(
        data_layer,
        components.clone(),
        bus_handle.clone(),
        strat_stop_rx,
    ));

    // Spawn RiskActor（被动消费者）
    let risk_stop_rx = stop_tx.subscribe();
    let risk_handle = tokio::spawn(run_risk_actor(
        bus_receiver.receiver,
        bus_handle,
        components,
        risk_stop_rx,
    ));

    // 等待任一 actor 结束
    tokio::select! {
        r = strat_handle => {
            match r {
                Ok(()) => tracing::info!("[Pipeline] StrategyActor finished normally"),
                Err(e) => tracing::error!("[Pipeline] StrategyActor panicked: {}", e),
            }
        }
        r = risk_handle => {
            match r {
                Ok(()) => tracing::info!("[Pipeline] RiskActor finished normally"),
                Err(e) => tracing::error!("[Pipeline] RiskActor panicked: {}", e),
            }
        }
    }

    // 广播停止信号（优雅退出）
    let _ = stop_tx.send(());

    tracing::info!("Self-driven pipeline stopped");
    Ok(())
}

// ============================================================================
// 程序入口
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志目录 + 启动 JSON Lines Writer
    let log_dir = std::env::var("LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./logs"));
    a_common::logs::init_log_dir(log_dir.clone());
    let _writer = JsonLinesWriter::new();
    tracing::info!("[JsonLinesWriter] log_dir={}, started", log_dir.display());

    // 2. 初始化 tracing subscriber（带 JSON Lines layer）
    tracing_subscriber::registry()
        .with(JsonLinesLayer::build())
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("=== Trading System v7.0 | 指标数据自驱动 | {} | {} ===", SYMBOL, DATA_FILE);
    init_heartbeat();

    // 3. 创建所有共享组件
    let (components, data_layer) = create_components().await?;

    // 4. 创建 PipelineBus
    let bus = PipelineBus::new(128, 128);

    // 5. 启动自驱动流水线
    run_pipeline(components, data_layer, bus).await?;

    // 6. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}
