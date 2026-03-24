//! 市场数据模型
//!
//! 定义市场数据相关的通用类型。

use crate::config::VOLATILITY_CONFIG;
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

/// 波动率等级 (3级)
/// - Low: 1m < 阈值 AND 15m < 阈值
/// - Medium: 1m >= 阈值 AND 15m < 阈值
/// - High: 15m >= 阈值
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilityTier {
    Low,
    Medium,
    High,
}

impl Default for VolatilityTier {
    fn default() -> Self {
        VolatilityTier::Low
    }
}

impl VolatilityTier {
    /// 根据1分钟和15分钟波动率判断等级
    pub fn from_volatility(vol_1m: Decimal, vol_15m: Decimal) -> Self {
        let config = &*VOLATILITY_CONFIG;
        if vol_15m >= config.high_vol_15m {
            VolatilityTier::High
        } else if vol_1m >= config.high_vol_1m {
            VolatilityTier::Medium
        } else {
            VolatilityTier::Low
        }
    }
}

/// 波动率信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityInfo {
    pub symbol: String,
    pub tier: VolatilityTier,
    pub vol_1m: Decimal,
    pub vol_15m: Decimal,
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
