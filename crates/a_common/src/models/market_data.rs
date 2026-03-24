//! 市场数据模型
//!
//! 定义市场数据相关的通用类型。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketKLine {
    pub symbol: String,
    pub period: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    pub is_closed: bool,
}

/// Tick 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketTick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 波动率等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilityLevel {
    High,
    Normal,
    Low,
}

/// 波动率信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityInfo {
    pub symbol: String,
    pub level: VolatilityLevel,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}

/// 订单簿层级
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: Decimal,
    pub qty: Decimal,
}

/// 订单簿快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp: DateTime<Utc>,
}
