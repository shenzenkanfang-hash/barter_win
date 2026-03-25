//! Tick 数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

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
    pub ts: i64,
    /// 是否是买方主动成交
    pub is_buyer_maker: bool,
}

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    /// 交易品种
    pub symbol: String,
    /// 开盘时间
    pub open_time: i64,
    /// 开盘价
    pub open: Decimal,
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 收盘时间
    pub close_time: i64,
}
