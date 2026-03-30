//! 自驱动流水线
//!
//! # 架构（已纠正）
//! - 数据层被动：Kline1mStream::next_message()（不主动发事件）
//! - StrategyActor 主动驱动：自己的循环，从数据层拉取
//! - RiskActor 被动消费：等待 PipelineBus 信号
//! - PipelineBus 只传跨协程信号（strategy/order）
//!
//! # 关键变化
//! - 消除 `tokio::time::sleep(50)` 的 serial polling loop
//! - 改为 StrategyActor 自循环（主动拉取）+ RiskActor（被动等待）

use tokio::sync::broadcast;

use crate::components::{SystemComponents, DataLayer};
use crate::event_bus::{PipelineBus, PipelineBusHandle};
use crate::actors::{run_strategy_actor, run_risk_actor};

/// 自驱动流水线启动函数
///
/// # 参数
/// - components: 所有共享组件（由 main.rs create_components 构造）
/// - bus: PipelineBus（自驱动协程间信号传递）
///
/// # 行为
/// 1. Spawn StrategyActor（主动驱动：拉取数据 → 处理 → 发信号）
/// 2. Spawn RiskActor（被动消费：等信号 → 风控 → 下单）
/// 3. 等待任一 actor 结束
/// 4. 广播停止信号
pub async fn run_pipeline(
    components: SystemComponents,
    data_layer: DataLayer,
    bus: (PipelineBusHandle, PipelineBus),
) -> Result<(), Box<dyn std::error::Error>> {
    let (bus_handle, bus_receiver) = bus;

    tracing::info!("Self-driven pipeline starting");

    // 创建 broadcast channel（Send-safe stop signal，所有 actor 共用一个 sender）
    // 使用 broadcast 而非 watch：broadcast::Receiver 是 Send-safe（watch::Receiver 持
    // 有 std::sync::RwLockReadGuard，非 Send，导致 tokio::spawn 时编译失败）
    let (stop_tx, _) = broadcast::channel::<()>(1);

    // Spawn StrategyActor（主动驱动方）
    // data_layer 单独传入（Kline1mStream 非 Send，仅在 actor 内局部使用）
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

    // 广播停止信号（优雅退出）：send() 触发所有 broadcast receiver 退出
    let _ = stop_tx.send(());

    tracing::info!("Self-driven pipeline stopped");
    Ok(())
}
