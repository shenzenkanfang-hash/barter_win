use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Period {
    Minute(u8),
    Day,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    /// 当前 1m K线（增量更新中）
    pub kline_1m: Option<KLine>,
    /// 15m K线（每15根1m K线合成）
    pub kline_15m: Option<KLine>,
    /// 日K线
    pub kline_1d: Option<KLine>,
}

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
}

/// 波动率统计
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolatilityStats {
    /// 是否高波动
    pub is_high_volatility: bool,
    /// 1m O-C 变化率
    pub vol_1m: Decimal,
    /// 15m Close-Close 变化率
    pub vol_15m: Decimal,
}

impl Default for VolatilityStats {
    fn default() -> Self {
        Self {
            is_high_volatility: false,
            vol_1m: dec!(0),
            vol_15m: dec!(0),
        }
    }
}
