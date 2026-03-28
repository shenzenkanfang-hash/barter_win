//! 事件总线 - 统一事件分发
//!
//! # 设计原则
//! - **背压控制**: `send().await` 自动阻塞
//! - **零拷贝**: 使用 Arc<TickEvent> 减少克隆开销

use std::sync::Arc;
use tokio::sync::mpsc;

use super::event_engine::{KlineData, OrderResult, TickEvent};

// ============================================================================
// 常量
// ============================================================================

/// 默认 channel buffer 大小
pub const DEFAULT_CHANNEL_BUFFER: usize = 1024;

/// Tick channel buffer 大小
pub const TICK_CHANNEL_BUFFER: usize = 1024;

/// K线闭合 channel buffer 大小
pub const KLINE_CHANNEL_BUFFER: usize = 256;

/// 订单 channel buffer 大小
pub const ORDER_CHANNEL_BUFFER: usize = 128;

// ============================================================================
// 事件类型
// ============================================================================

/// K线闭合事件
#[derive(Debug, Clone)]
pub struct KlineClosedEvent {
    /// 品种
    pub symbol: String,
    /// 周期
    pub period: String,
    /// 闭合的K线
    pub kline: KlineData,
    /// 上一根K线（用于计算指标变化）
    pub prev_kline: Option<KlineData>,
}

/// 订单事件
#[derive(Debug, Clone)]
pub struct OrderEvent {
    /// 订单ID
    pub order_id: String,
    /// 品种
    pub symbol: String,
    /// 订单结果
    pub result: OrderResult,
}

// ============================================================================
// EventBus
// ============================================================================

/// 事件总线
///
/// 负责分发 Tick、K线闭合、订单等事件
/// 
/// # 使用方式
/// ```ignore
/// let (mut bus, handle) = EventBus::default();
/// let tick_rx = bus.take_tick_rx().unwrap();
/// // 或者直接使用
/// let tick_rx = bus.into_tick_rx();
/// ```
pub struct EventBus {
    /// Tick 事件接收端
    tick_rx: Option<mpsc::Receiver<Arc<TickEvent>>>,
    /// K线闭合事件接收端
    kline_rx: Option<mpsc::Receiver<KlineClosedEvent>>,
    /// 订单事件接收端
    order_rx: Option<mpsc::Receiver<OrderEvent>>,

    /// Tick 事件发送端（保留用于未来多消费者扩展）
    #[allow(dead_code)]
    tick_tx: mpsc::Sender<Arc<TickEvent>>,
    /// K线闭合事件发送端
    #[allow(dead_code)]
    kline_tx: mpsc::Sender<KlineClosedEvent>,
    /// 订单事件发送端
    #[allow(dead_code)]
    order_tx: mpsc::Sender<OrderEvent>,
}

/// EventBus 句柄（发送端）
#[derive(Clone)]
pub struct EventBusHandle {
    /// Tick 事件发送端
    pub tick_tx: mpsc::Sender<Arc<TickEvent>>,
    /// K线闭合事件发送端
    pub kline_tx: mpsc::Sender<KlineClosedEvent>,
    /// 订单事件发送端
    pub order_tx: mpsc::Sender<OrderEvent>,
}

impl EventBus {
    /// 创建事件总线
    pub fn new(
        tick_buffer: usize,
        kline_buffer: usize,
        order_buffer: usize,
    ) -> (Self, EventBusHandle) {
        let (tick_tx, tick_rx) = mpsc::channel(tick_buffer);
        let (kline_tx, kline_rx) = mpsc::channel(kline_buffer);
        let (order_tx, order_rx) = mpsc::channel(order_buffer);
        
        let bus = Self {
            tick_rx: Some(tick_rx),
            kline_rx: Some(kline_rx),
            order_rx: Some(order_rx),
            tick_tx: tick_tx.clone(),
            kline_tx: kline_tx.clone(),
            order_tx: order_tx.clone(),
        };
        
        let handle = EventBusHandle {
            tick_tx,
            kline_tx,
            order_tx,
        };
        
        (bus, handle)
    }
    
    /// 使用默认 buffer 大小创建
    pub fn default() -> (Self, EventBusHandle) {
        Self::new(TICK_CHANNEL_BUFFER, KLINE_CHANNEL_BUFFER, ORDER_CHANNEL_BUFFER)
    }
    
    /// 获取 Tick 事件接收端（消费 self）
    pub fn into_tick_rx(self) -> mpsc::Receiver<Arc<TickEvent>> {
        self.tick_rx.expect("tick_rx already taken")
    }
    
    /// 获取 Tick 事件接收端（可变引用）
    pub fn tick_rx_mut(&mut self) -> Option<&mut mpsc::Receiver<Arc<TickEvent>>> {
        self.tick_rx.as_mut()
    }
    
    /// 获取 K线闭合事件接收端
    pub fn kline_rx_mut(&mut self) -> Option<&mut mpsc::Receiver<KlineClosedEvent>> {
        self.kline_rx.as_mut()
    }
    
    /// 获取订单事件接收端
    pub fn order_rx_mut(&mut self) -> Option<&mut mpsc::Receiver<OrderEvent>> {
        self.order_rx.as_mut()
    }
}

impl EventBusHandle {
    /// 发送 Tick 事件
    ///
    /// 使用 Arc 避免克隆开销
    pub async fn send_tick(&self, tick: TickEvent) -> Result<(), TickSendError> {
        let tick = Arc::new(tick);
        self.tick_tx.send(tick).await.map_err(|e| TickSendError(format!("{}", e)))?;
        Ok(())
    }
    
    /// 尝试发送 Tick 事件（非阻塞）
    pub fn try_send_tick(&self, tick: TickEvent) -> Result<(), TickSendError> {
        let tick = Arc::new(tick);
        self.tick_tx.try_send(tick).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => TickSendError("channel full".to_string()),
            mpsc::error::TrySendError::Closed(_) => TickSendError("channel closed".to_string()),
        })?;
        Ok(())
    }
    
    /// 发送 K线闭合事件
    pub async fn send_kline_closed(&self, event: KlineClosedEvent) -> Result<(), KlineSendError> {
        self.kline_tx.send(event).await.map_err(|e| KlineSendError(format!("{}", e)))?;
        Ok(())
    }
    
    /// 发送订单事件
    pub async fn send_order(&self, event: OrderEvent) -> Result<(), OrderSendError> {
        self.order_tx.send(event).await.map_err(|e| OrderSendError(format!("{}", e)))?;
        Ok(())
    }
    
    /// 检查 Tick channel 是否还有容量
    pub fn tick_channel_remaining(&self) -> usize {
        self.tick_tx.capacity()
    }
    
    /// 检查 Tick channel 是否已满
    pub fn is_tick_channel_full(&self) -> bool {
        self.tick_tx.capacity() == 0
    }
}

// ============================================================================
// 错误类型
// ============================================================================

/// Tick 发送错误
#[derive(Debug, thiserror::Error)]
#[error("Tick发送失败: {0}")]
pub struct TickSendError(pub String);

/// K线发送错误
#[derive(Debug, thiserror::Error)]
#[error("K线事件发送失败: {0}")]
pub struct KlineSendError(pub String);

/// 订单发送错误
#[derive(Debug, thiserror::Error)]
#[error("订单事件发送失败: {0}")]
pub struct OrderSendError(pub String);
