//! K线数据类型

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

// ============================================================================
// KlineData
// ============================================================================

/// K线数据（WebSocket实时数据，用于存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    /// K线开始时间戳
    #[serde(rename = "t")]
    pub kline_start_time: i64,
    /// K线结束时间戳
    #[serde(rename = "T")]
    pub kline_close_time: i64,
    /// 交易品种
    #[serde(rename = "s")]
    pub symbol: String,
    /// K线周期
    #[serde(rename = "i")]
    pub interval: String,
    /// 开盘价
    #[serde(rename = "o")]
    pub open: String,
    /// 收盘价
    #[serde(rename = "c")]
    pub close: String,
    /// 最高价
    #[serde(rename = "h")]
    pub high: String,
    /// 最低价
    #[serde(rename = "l")]
    pub low: String,
    /// 成交量
    #[serde(rename = "v")]
    pub volume: String,
    /// 是否收盘
    #[serde(rename = "x")]
    pub is_closed: bool,
}
