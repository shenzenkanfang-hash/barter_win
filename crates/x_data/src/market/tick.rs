//! Tick 数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// ============================================================================
// Period
// ============================================================================

/// K线周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Period {
    /// 分钟周期，如 Minute(1) 表示 1 分钟
    Minute(u8),
    /// 日周期
    Day,
}

// ============================================================================
// Tick
// ============================================================================

/// Tick 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    /// 交易品种
    pub symbol: String,
    /// 最新价格
    pub price: Decimal,
    /// 数量
    pub qty: Decimal,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 当前 1m K线（增量更新中）
    pub kline_1m: Option<KLine>,
    /// 15m K线（每15根1m K线合成）
    pub kline_15m: Option<KLine>,
    /// 日K线
    pub kline_1d: Option<KLine>,
}

// ============================================================================
// KLine
// ============================================================================

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    /// 交易品种
    pub symbol: String,
    /// 周期
    pub period: Period,
    /// 开盘价
    pub open: Decimal,
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 成交量
    pub volume: Decimal,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}
