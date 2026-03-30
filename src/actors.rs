//! PipelineActors - 消费者自驱动协程
//!
//! # 架构原则（已纠正）
//! - 数据层 = 被动接口（Kline1mStream::next_message()）
//! - StrategyActor = 主动驱动方（自己的循环，按需拉取）
//! - RiskActor = 被动消费者（等待 PipelineBus 信号）
//! - PipelineBus = 仅跨协程信号通道
//!
//! # 关键区别
//! - 无 DataSourceActor（数据层不发事件）
//! - 无 PipelineBus.raw_data_tx（原始数据不走 Bus）
//! - StrategyActor 直接调用 kline_stream.next_message()
//!
//! # Send 约束
//! - Kline1mStream 含非 Send 类型（ThreadRng），mutex guard 必须在 await 前释放
//! - 使用独立 block { let _guard = lock.await; data = fn(); } 确保 guard 在 await 前 drop
//! - Stop signal 使用 broadcast::Receiver（Send-safe）而非 watch::Receiver（非 Send）

use tokio::time::{sleep, Duration};
use rust_decimal::Decimal;

use crate::event_bus::{
    PipelineBusHandle, PipelineBusReceiver, StrategySignalEvent, StrategyDecision,
    OrderEvent, OrderSide, OrderStatus,
};
use crate::components::{SystemComponents, DataLayer};
use crate::tick_context::{SYMBOL, INITIAL_BALANCE};
use crate::utils::parse_raw_kline;

/// 心跳报到间隔（秒）
const HEARTBEAT_INTERVAL_SECS: u64 = 10;

/// StrategyActor - 策略执行协程（主动驱动方）
///
/// 拥有自己的循环，主动从数据层拉取数据：
/// 1. 调用 kline_stream.next_message()（数据层被动接口）
/// 2. 调用 signal_processor.min_update()
/// 3. 调用 trader.execute_once_wal()（带 TradeLock）
/// 4. 通过 PipelineBus.strategy_tx 发送信号
///
/// # Send 边界说明
/// - `data_layer`（含 Kline1mStream，非 Send）仅在独立 block 内使用，guard 在 await 前 drop
/// - `components`（SystemComponents，Send-safe）跨 await 点捕获
/// - Stop signal 使用 broadcast::Receiver（Send-safe）
pub async fn run_strategy_actor(
    data_layer: DataLayer,
    components: SystemComponents,
    bus_handle: PipelineBusHandle,
    mut stop_rx: tokio::sync::broadcast::Receiver<()>,
) {
    tracing::info!("[Actor:strategy] started");

    let mut tick_id = 0u64;
    let mut heartbeat_tick = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));

    loop {
        tokio::select! {
            biased;

            // 停止信号（broadcast recv 返回 Err 表示 channel 关闭）
            _ = stop_rx.recv() => {
                tracing::info!("[Actor:strategy] stop signal received");
                break;
            }

            // 心跳报到
            _ = heartbeat_tick.tick() => {
                tracing::trace!("[Actor:strategy] heartbeat tick_id={}", tick_id);
            }

            // 主动从数据层拉取数据（50ms 间隔，actor 自驱动节奏控制）
            // 注意：mutex guard 必须在 await 前释放（Kline1mStream 含非 Send RNG）
            _ = sleep(Duration::from_millis(50)) => {
                let kline_data = {
                    // guard 在此 block 结束时立即释放，不跨越 await
                    let mut stream = data_layer.kline_stream.lock().await;
                    stream.next_message()
                };

                let Some(data) = kline_data else {
                    tracing::info!("[Actor:strategy] data exhausted at tick {}", tick_id);
                    break;
                };

                tick_id += 1;

                let kline = match parse_raw_kline(&data) {
                    Ok(k) => k,
                    Err(e) => {
                        tracing::warn!("[StageB] Parse error: {}", e);
                        continue;
                    }
                };

                // ===== Stage B: 数据验证 =====
                let valid = kline.close > Decimal::ZERO;
                if !valid {
                    tracing::warn!("[StageB] invalid price close={}", kline.close);
                    continue;
                }

                // ===== Stage F: 更新网关价格 =====
                components.gateway.update_price(SYMBOL, kline.close);

                // ===== Stage C: 更新指标处理器 =====
                let _signal_ok = components.signal_processor.min_update(
                    SYMBOL,
                    kline.high,
                    kline.low,
                    kline.close,
                    kline.volume,
                );

                // ===== Stage D: 策略执行（带 TradeLock） =====
                let trade_result = {
                    let guard = match components.trade_lock.acquire("h_15m_strategy") {
                        Ok(g) => g,
                        Err(e) => {
                            tracing::warn!("[StageD] TradeLock conflict: {}", e);
                            // 锁冲突：发送 Skip 信号
                            let signal = StrategySignalEvent {
                                tick_id,
                                symbol: SYMBOL.to_string(),
                                decision: StrategyDecision::Skip,
                                qty: None,
                                reason: format!("lock_conflict: {}", e),
                            };
                            let _ = bus_handle.send_strategy_signal(signal);
                            continue;
                        }
                    };

                    let r = components.trader.execute_once_wal().await;
                    drop(guard); // RAII 释放锁
                    r
                };

                // ===== 转换为 StrategySignalEvent =====
                let (decision, qty, reason) = match &trade_result {
                    Ok(d_checktable::h_15m::ExecutionResult::Executed { qty, .. }) => {
                        (StrategyDecision::LongEntry, Some(*qty), "signal_triggered".into())
                    }
                    Ok(d_checktable::h_15m::ExecutionResult::Skipped(reason)) => {
                        (StrategyDecision::Skip, None, reason.to_string())
                    }
                    Ok(d_checktable::h_15m::ExecutionResult::Failed(e)) => {
                        (StrategyDecision::Error, None, e.to_string())
                    }
                    Err(e) => {
                        (StrategyDecision::Error, None, e.to_string())
                    }
                };

                let signal = StrategySignalEvent {
                    tick_id,
                    symbol: SYMBOL.to_string(),
                    decision,
                    qty,
                    reason,
                };

                if bus_handle.send_strategy_signal(signal).is_err() {
                    tracing::warn!("[Actor:strategy] strategy_tx channel closed");
                    break;
                }

                if tick_id % 100 == 0 {
                    tracing::info!(
                        "[Actor:strategy] tick {} decision={:?}",
                        tick_id,
                        decision
                    );
                }
            }
        }
    }

    tracing::info!("[Actor:strategy] stopped, total ticks={}", tick_id);
}

/// RiskActor - 风控执行协程（被动消费者）
///
/// 接收 StrategySignalEvent，执行风控检查和下单：
/// 1. 等待 PipelineBus.strategy_rx 收到信号
/// 2. pre_check() 风控检查
/// 3. place_order() 下单
/// 4. 通过 PipelineBus.order_tx 发送订单结果
///
/// # Send 边界说明
/// - Stop signal 使用 broadcast::Receiver（Send-safe）
pub async fn run_risk_actor(
    receiver: PipelineBusReceiver,
    bus_handle: PipelineBusHandle,
    components: SystemComponents,
    mut stop_rx: tokio::sync::broadcast::Receiver<()>,
) {
    tracing::info!("[Actor:risk] started");

    let mut strategy_rx = receiver.strategy_rx;
    let mut order_id_counter = 0u64;

    loop {
        tokio::select! {
            biased;

            // 停止信号（broadcast recv 返回 Err 表示 channel 关闭）
            _ = stop_rx.recv() => {
                tracing::info!("[Actor:risk] stop signal");
                break;
            }

            Ok(signal) = strategy_rx.recv() => {
                // ===== Stage E: 风控检查 =====
                let Some(qty) = signal.qty else {
                    continue; // Skip/Error 无下单数量
                };

                order_id_counter += 1;
                let order_id = format!("order_{}", order_id_counter);

                // 余额风控检查
                let balance_passed = components
                    .risk_checker
                    .pre_check(
                        SYMBOL,
                        INITIAL_BALANCE,
                        Decimal::try_from(100).unwrap(),
                        INITIAL_BALANCE,
                    )
                    .is_ok();

                // 订单风控检查
                let order_check_result = components.order_checker.pre_check(
                    &order_id,
                    SYMBOL,
                    "h_15m_strategy",
                    Decimal::try_from(100).unwrap(),
                    INITIAL_BALANCE,
                    Decimal::ZERO,
                );
                let order_passed = order_check_result.passed;

                if balance_passed && order_passed {
                    match components.gateway.place_order(
                        SYMBOL,
                        b_data_mock::api::mock_account::Side::Buy,
                        qty,
                        None,
                    ) {
                        Ok(order) => {
                            tracing::info!(
                                "[StageE] Filled: {} price={} qty={}",
                                order_id,
                                order.filled_price,
                                order.filled_qty
                            );
                            let event = OrderEvent {
                                order_id,
                                symbol: SYMBOL.to_string(),
                                side: OrderSide::Buy,
                                qty: order.filled_qty,
                                filled_price: order.filled_price,
                                status: OrderStatus::Filled,
                            };
                            let _ = bus_handle.send_order(event).await;
                        }
                        Err(e) => {
                            tracing::warn!("[StageE] Order failed: {}", e);
                            let event = OrderEvent {
                                order_id,
                                symbol: SYMBOL.to_string(),
                                side: OrderSide::Buy,
                                qty,
                                filled_price: Decimal::ZERO,
                                status: OrderStatus::Rejected,
                            };
                            let _ = bus_handle.send_order(event).await;
                        }
                    }
                } else {
                    tracing::warn!(
                        "[StageE] {} Risk rejected: balance={} order={}",
                        order_id,
                        balance_passed,
                        order_passed
                    );
                    let event = OrderEvent {
                        order_id,
                        symbol: SYMBOL.to_string(),
                        side: OrderSide::Buy,
                        qty,
                        filled_price: Decimal::ZERO,
                        status: OrderStatus::Cancelled,
                    };
                    let _ = bus_handle.send_order(event).await;
                }
            }
        }
    }

    tracing::info!("[Actor:risk] stopped");
}
