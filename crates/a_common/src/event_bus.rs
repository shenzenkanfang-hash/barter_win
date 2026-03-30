//! PipelineBus - 策略协程间信号总线
//!
//! # 架构原则
//! - 数据层被动（只暴露 pull/pop 接口，不发事件）
//! - 消费者主动（自己决定什么时候取数据）
//! - PipelineBus 只传递：策略信号 + 订单结果

use rust_decimal::Decimal;
use tokio::sync::{broadcast, mpsc};

// ============================================================================
// 事件类型（仅跨协程信号，不含原始数据）
// ============================================================================

/// 策略信号事件（StrategyActor → RiskActor）
#[derive(Debug, Clone)]
pub struct StrategySignalEvent {
    pub tick_id: u64,
    pub symbol: String,
    pub decision: StrategyDecision,
    pub qty: Option<Decimal>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyDecision {
    /// 多头入场
    LongEntry,
    /// 空头入场
    ShortEntry,
    /// 平仓
    Flat,
    /// 无信号跳过
    Skip,
    /// 执行错误
    Error,
}

/// 订单事件（RiskActor → 外部记录/日志）
#[derive(Debug, Clone)]
pub struct OrderEvent {
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub qty: Decimal,
    pub filled_price: Decimal,
    pub status: OrderStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide { Buy, Sell }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus { Pending, Filled, Rejected, Cancelled }

// ============================================================================
// PipelineBus
// ============================================================================

/// PipelineBus 句柄（发送端，由 StrategyActor 和 RiskActor 持有）
#[derive(Clone)]
pub struct PipelineBusHandle {
    /// 策略信号发送端（StrategyActor → RiskActor）
    pub strategy_tx: broadcast::Sender<StrategySignalEvent>,
    /// 订单事件发送端（RiskActor → 记录）
    pub order_tx: mpsc::Sender<OrderEvent>,
}

/// PipelineBus 接收端（由 RiskActor 持有）
///
/// 使用 broadcast channel 而非 mpsc：
/// - broadcast::Receiver 是 Send-safe（解决了 mpsc::Receiver 含 std::sync::RwLock 非 Send 的问题）
/// - broadcast 支持多个消费者（扩展性更好）
pub struct PipelineBusReceiver {
    /// 策略信号广播接收端
    pub strategy_rx: broadcast::Receiver<StrategySignalEvent>,
}

/// PipelineBus 主体（持有接收端，用于跨模块传递）
pub struct PipelineBus {
    pub receiver: PipelineBusReceiver,
}

impl PipelineBus {
    /// 创建 PipelineBus
    ///
    /// 使用 broadcast channel 替代 mpsc channel：
    /// - broadcast::channel 返回 (Sender, Receiver)，均实现 Send
    /// - mpsc::channel 的 Receiver 内部含 std::sync::RwLock（非 Send），无法跨任务边界
    pub fn new(strategy_buffer: usize, order_buffer: usize) -> (PipelineBusHandle, PipelineBus) {
        let (strategy_tx, strategy_rx) = broadcast::channel(strategy_buffer);
        let (order_tx, _order_rx) = mpsc::channel(order_buffer);

        let handle = PipelineBusHandle {
            strategy_tx,
            order_tx,
        };

        let bus = PipelineBus {
            receiver: PipelineBusReceiver { strategy_rx },
        };

        (handle, bus)
    }
}

impl PipelineBusHandle {
    /// 发送策略信号（StrategyActor → RiskActor）
    ///
    /// broadcast::Sender::send 是同步的（不需要 await），返回 Result<usize, SendError>
    /// usize 表示当前订阅者数量（lag 指标）
    pub fn send_strategy_signal(&self, event: StrategySignalEvent) -> Result<usize, broadcast::error::SendError<StrategySignalEvent>> {
        let event_for_err = event.clone();
        self.strategy_tx.send(event).map_err(|_| broadcast::error::SendError(event_for_err))
    }

    /// 发送订单事件（RiskActor → 记录）
    pub async fn send_order(&self, event: OrderEvent) -> Result<(), mpsc::error::SendError<OrderEvent>> {
        self.order_tx.send(event).await
    }

    /// 通道状态
    ///
    /// broadcast::Sender 没有 capacity()，用 len() 近似
    #[allow(dead_code)]
    pub fn channel_status(&self) -> ChannelStatus {
        ChannelStatus {
            strategy_remaining: self.strategy_tx.len(),
            order_remaining: self.order_tx.capacity(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChannelStatus {
    pub strategy_remaining: usize,
    pub order_remaining: usize,
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_signal_event_clone() {
        let event = StrategySignalEvent {
            tick_id: 42,
            symbol: "BTCUSDT".into(),
            decision: StrategyDecision::LongEntry,
            qty: Some(Decimal::try_from(0.1).unwrap()),
            reason: "signal_triggered".into(),
        };
        let cloned = event.clone();
        assert_eq!(cloned.tick_id, event.tick_id);
        assert_eq!(cloned.symbol, event.symbol);
        assert_eq!(cloned.decision, event.decision);
        assert_eq!(cloned.qty, event.qty);
        assert_eq!(cloned.reason, event.reason);
    }

    #[test]
    fn test_strategy_signal_event_debug() {
        let event = StrategySignalEvent {
            tick_id: 1,
            symbol: "ETHUSDT".into(),
            decision: StrategyDecision::Skip,
            qty: None,
            reason: "lock_conflict".into(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("ETHUSDT"));
        assert!(debug_str.contains("Skip"));
    }

    #[test]
    fn test_strategy_decision_variants() {
        assert_eq!(StrategyDecision::LongEntry, StrategyDecision::LongEntry);
        assert_eq!(StrategyDecision::ShortEntry, StrategyDecision::ShortEntry);
        assert_eq!(StrategyDecision::Flat, StrategyDecision::Flat);
        assert_eq!(StrategyDecision::Skip, StrategyDecision::Skip);
        assert_eq!(StrategyDecision::Error, StrategyDecision::Error);
    }

    #[test]
    fn test_strategy_decision_partial_eq() {
        assert_eq!(StrategyDecision::LongEntry, StrategyDecision::LongEntry);
        assert_ne!(StrategyDecision::LongEntry, StrategyDecision::Skip);
    }

    #[test]
    fn test_strategy_decision_copy() {
        let d = StrategyDecision::LongEntry;
        let d2 = d; // Copy
        assert_eq!(d, d2);
    }

    #[test]
    fn test_order_event_clone() {
        let event = OrderEvent {
            order_id: "order_1".into(),
            symbol: "BTCUSDT".into(),
            side: OrderSide::Buy,
            qty: Decimal::try_from(0.05).unwrap(),
            filled_price: Decimal::try_from(50000).unwrap(),
            status: OrderStatus::Filled,
        };
        let cloned = event.clone();
        assert_eq!(cloned.order_id, "order_1");
        assert_eq!(cloned.side, OrderSide::Buy);
        assert_eq!(cloned.status, OrderStatus::Filled);
    }

    #[test]
    fn test_order_event_debug() {
        let event = OrderEvent {
            order_id: "order_99".into(),
            symbol: "BTCUSDT".into(),
            side: OrderSide::Sell,
            qty: Decimal::try_from(0.01).unwrap(),
            filled_price: Decimal::try_from(49000).unwrap(),
            status: OrderStatus::Rejected,
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("order_99"));
        assert!(debug_str.contains("Rejected"));
    }

    #[test]
    fn test_order_side_variants() {
        assert_eq!(OrderSide::Buy, OrderSide::Buy);
        assert_eq!(OrderSide::Sell, OrderSide::Sell);
        assert_ne!(OrderSide::Buy, OrderSide::Sell);
    }

    #[test]
    fn test_order_status_variants() {
        assert_eq!(OrderStatus::Pending, OrderStatus::Pending);
        assert_eq!(OrderStatus::Filled, OrderStatus::Filled);
        assert_eq!(OrderStatus::Rejected, OrderStatus::Rejected);
        assert_eq!(OrderStatus::Cancelled, OrderStatus::Cancelled);
    }

    #[test]
    fn test_pipeline_bus_new() {
        let (handle, bus) = PipelineBus::new(16, 32);
        let handle2 = handle.clone();
        let status = handle2.channel_status();
        assert_eq!(status.strategy_remaining, 0);
        assert_eq!(status.order_remaining, 32);
        let _ = bus.receiver;
    }

    #[test]
    fn test_pipeline_bus_handle_clone() {
        let (handle, _bus) = PipelineBus::new(8, 8);
        let handle2 = handle.clone();
        drop(handle);
        let status = handle2.channel_status();
        assert_eq!(status.strategy_remaining, 0);
    }

    #[tokio::test]
    async fn test_send_strategy_signal_no_receiver() {
        let (handle, bus) = PipelineBus::new(1, 1);
        drop(bus);
        let event = StrategySignalEvent {
            tick_id: 1,
            symbol: "BTCUSDT".into(),
            decision: StrategyDecision::LongEntry,
            qty: Some(Decimal::try_from(0.1).unwrap()),
            reason: "test".into(),
        };
        let result = handle.send_strategy_signal(event);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_strategy_signal_with_receiver() {
        let (handle, bus) = PipelineBus::new(8, 8);
        let event = StrategySignalEvent {
            tick_id: 7,
            symbol: "BTCUSDT".into(),
            decision: StrategyDecision::LongEntry,
            qty: Some(Decimal::try_from(0.1).unwrap()),
            reason: "test_with_receiver".into(),
        };
        let result = handle.send_strategy_signal(event);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        let mut rx = bus.receiver.strategy_rx;
        let received = rx.recv().await;
        assert!(received.is_ok());
        let recv_event = received.unwrap();
        assert_eq!(recv_event.tick_id, 7);
        assert_eq!(recv_event.symbol, "BTCUSDT");
    }

    #[tokio::test]
    async fn test_send_strategy_signal_multiple_receivers() {
        let (handle, bus) = PipelineBus::new(8, 8);
        let sub_rx = handle.strategy_tx.subscribe();

        let event = StrategySignalEvent {
            tick_id: 99,
            symbol: "ETHUSDT".into(),
            decision: StrategyDecision::Skip,
            qty: None,
            reason: "multi_receiver".into(),
        };

        let result = handle.send_strategy_signal(event);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);

        let mut rx1 = bus.receiver.strategy_rx;
        let mut rx2 = sub_rx;

        let recv1 = rx1.recv().await.unwrap();
        let recv2 = rx2.recv().await.unwrap();
        assert_eq!(recv1.tick_id, 99);
        assert_eq!(recv2.tick_id, 99);
    }

    #[tokio::test]
    async fn test_send_order_no_receiver() {
        let (handle, _bus) = PipelineBus::new(8, 8);
        let event = OrderEvent {
            order_id: "test_order".into(),
            symbol: "BTCUSDT".into(),
            side: OrderSide::Buy,
            qty: Decimal::try_from(0.1).unwrap(),
            filled_price: Decimal::try_from(50000).unwrap(),
            status: OrderStatus::Filled,
        };
        let result = handle.send_order(event).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_channel_status() {
        let (handle, _bus) = PipelineBus::new(8, 8);
        let status = handle.channel_status();
        assert_eq!(status.strategy_remaining, 0);
        assert_eq!(status.order_remaining, 8);
    }

    #[test]
    fn test_channel_status_after_send() {
        let (handle, _bus) = PipelineBus::new(8, 8);
        let event = StrategySignalEvent {
            tick_id: 1,
            symbol: "BTCUSDT".into(),
            decision: StrategyDecision::LongEntry,
            qty: Some(Decimal::try_from(0.1).unwrap()),
            reason: "test".into(),
        };
        let _ = handle.send_strategy_signal(event);
        let status = handle.channel_status();
        assert_eq!(status.strategy_remaining, 1);
    }

    #[test]
    fn test_channel_status_clone() {
        let status = ChannelStatus {
            strategy_remaining: 5,
            order_remaining: 10,
        };
        let cloned = status.clone();
        assert_eq!(cloned.strategy_remaining, 5);
        assert_eq!(cloned.order_remaining, 10);
    }

    #[test]
    fn test_channel_status_debug() {
        let status = ChannelStatus {
            strategy_remaining: 3,
            order_remaining: 7,
        };
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("3"));
        assert!(debug_str.contains("7"));
    }
}
