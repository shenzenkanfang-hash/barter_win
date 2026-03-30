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

use tokio::sync::mpsc;
use chrono::{DateTime, Utc};
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
    pub strategy_tx: mpsc::Sender<StrategySignalEvent>,
    /// 订单事件发送端（RiskActor → 记录）
    pub order_tx: mpsc::Sender<OrderEvent>,
}

/// PipelineBus 接收端（由 RiskActor 持有）
pub struct PipelineBusReceiver {
    pub strategy_rx: mpsc::Receiver<StrategySignalEvent>,
}

/// PipelineBus 主体（持有接收端，用于跨模块传递）
pub struct PipelineBus {
    pub receiver: PipelineBusReceiver,
}

impl PipelineBus {
    /// 创建 PipelineBus
    pub fn new(strategy_buffer: usize, order_buffer: usize) -> (PipelineBusHandle, PipelineBus) {
        let (strategy_tx, strategy_rx) = mpsc::channel(strategy_buffer);
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
    pub async fn send_strategy_signal(&self, event: StrategySignalEvent) -> Result<(), mpsc::error::SendError<StrategySignalEvent>> {
        self.strategy_tx.send(event).await
    }

    /// 发送订单事件（RiskActor → 记录）
    pub async fn send_order(&self, event: OrderEvent) -> Result<(), mpsc::error::SendError<OrderEvent>> {
        self.order_tx.send(event).await
    }

    /// 通道状态
    pub fn channel_status(&self) -> ChannelStatus {
        ChannelStatus {
            strategy_remaining: self.strategy_tx.capacity(),
            order_remaining: self.order_tx.capacity(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelStatus {
    pub strategy_remaining: usize,
    pub order_remaining: usize,
}
