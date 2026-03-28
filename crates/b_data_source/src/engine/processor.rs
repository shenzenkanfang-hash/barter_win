//! 统一事件处理器 trait
//!
//! 提供统一的事件处理架构，基于 barter-rs 的 Processor/Auditor 模式。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 模拟 Tick 数据结构
///
/// 用于回测和模拟场景，包含 K 线上下文信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedTick {
    /// 交易对
    pub symbol: String,
    /// 当前价格
    pub price: Decimal,
    /// 数量
    pub qty: Decimal,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 序列号
    pub sequence_id: u64,
    /// K 线开盘价
    pub open: Decimal,
    /// K 线最高价（截至当前 tick）
    pub high: Decimal,
    /// K 线最低价（截至当前 tick）
    pub low: Decimal,
    /// 成交量（截至当前 tick）
    pub volume: Decimal,
    /// K 线时间戳
    pub kline_timestamp: DateTime<Utc>,
    /// 是否是 K 线最后一根 tick
    pub is_last_in_kline: bool,
}

/// 统一事件处理器
///
/// 任何需要处理事件的组件都可以实现此 trait
pub trait Processor<Event> {
    /// 审计输出类型
    type Audit;

    /// 处理事件
    fn process(&mut self, event: Event) -> Self::Audit;
}

/// Tick 处理器接口
pub trait TickProcessor: Processor<SimulatedTick> {}

impl<T: Processor<SimulatedTick>> TickProcessor for T {}
