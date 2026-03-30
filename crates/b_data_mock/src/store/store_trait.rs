//! MarketDataStore trait - 统一数据存储接口
//!
//! 与 b_data_source::store::store_trait 完全对齐

use crate::ws::kline_1m::KlineData;
use serde::{Deserialize, Serialize};

/// 订单簿数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookData {
    pub symbol: String,
    pub bids: Vec<(f64, f64)>, // (price, qty)
    pub asks: Vec<(f64, f64)>, // (price, qty)
    pub timestamp_ms: i64,
}

/// 波动率数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityData {
    pub symbol: String,
    pub volatility: f64,
    pub update_time_ms: i64,
}

/// MarketDataStore 统一存储接口
pub trait MarketDataStore: Send + Sync {
    // ========== 写入 ==========

    /// 写入K线数据
    fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool);

    /// 写入订单簿
    fn write_orderbook(&self, symbol: &str, orderbook: OrderBookData);

    // ========== 查询 ==========

    /// 获取当前K线（实时分区）
    fn get_current_kline(&self, symbol: &str) -> Option<KlineData>;

    /// 获取订单簿
    fn get_orderbook(&self, symbol: &str) -> Option<OrderBookData>;

    /// 获取历史K线（历史分区）
    fn get_history_klines(&self, symbol: &str) -> Vec<KlineData>;

    /// 获取历史订单簿
    fn get_history_orderbooks(&self, symbol: &str) -> Vec<OrderBookData>;

    /// 获取波动率
    fn get_volatility(&self, symbol: &str) -> Option<VolatilityData>;

    // ========== 指标存储（NO_SIGNAL 修复）==========

    /// 写入分钟级指标（由 SignalProcessor 调用）
    fn write_indicator(&self, symbol: &str, indicator: serde_json::Value) {
        let _ = (symbol, indicator);
    }

    /// 读取分钟级指标（由 Trader 调用）
    fn get_indicator(&self, symbol: &str) -> Option<serde_json::Value> {
        let _ = symbol;
        None
    }
}
