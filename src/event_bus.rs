//! PipelineBus - 策略协程间事件总线
//!
//! # 架构原则
//! - 数据层被动（只暴露 pull/pop 接口，不发事件）
//! - 消费者主动（自己决定什么时候取数据）
//! - PipelineBus 只传递：策略信号 + 订单结果
//!
//! # 事件流
//! ```
//! StrategyActor
//!   │ execute_once_wal() → 数据层（被动接口）
//!   │ min_update()       → 指标处理器（被动接口）
//!   │ send_strategy_signal()
//!   ▼
//! PipelineBus.strategy_tx
//!   │ StrategySignalEvent
//!   ▼
//! RiskActor
//!   │ pre_check() + place_order()
//!   │ send_order()
//!   ▼
//! PipelineBus.order_tx
//! ```

use tokio::sync::{mpsc, broadcast};
use rust_decimal::Decimal;

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
    pub fn channel_status(&self) -> ChannelStatus {
        ChannelStatus {
            strategy_remaining: self.strategy_tx.len(),
            order_remaining: self.order_tx.capacity(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelStatus {
    pub strategy_remaining: usize,
    pub order_remaining: usize,
}
