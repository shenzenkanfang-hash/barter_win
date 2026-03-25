//! K线数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// K线数据（用于存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    /// 交易品种
    pub symbol: String,
    /// K线周期
    pub period: String,
    /// 开盘时间戳
    pub open_time: i64,
    /// 开盘价
    pub open: Decimal,
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 收盘时间戳
    pub close_time: i64,
    /// 成交量
    pub volume: Decimal,
}
