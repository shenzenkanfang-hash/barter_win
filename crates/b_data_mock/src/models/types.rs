//! 数据模型 - K线/Tick/周期类型
//!
//! 复制自 b_data_source::models::types，与实盘接口对齐

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// K线周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Period {
    Minute(u8),
    Day,
}

/// 市场 Tick 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    /// 序列号（用于幂等性去重）
    pub sequence_id: u64,
    /// 当前 1m K线（增量更新中）
    pub kline_1m: Option<KLine>,
    /// 15m K线（每15根1m K线合成）
    pub kline_15m: Option<KLine>,
    /// 日K线
    pub kline_1d: Option<KLine>,
}

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    pub symbol: String,
    pub period: Period,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    /// K线是否已闭合（最后一根tick时为true）
    pub is_closed: bool,
}
