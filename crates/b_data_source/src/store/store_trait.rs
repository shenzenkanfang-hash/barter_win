//! MarketDataStore trait - 统一数据存储接口
//!
//! WS 和模拟器共用同一存储接口，方便切换模拟/真实 WS 行为。

use crate::ws::kline_1m::ws::KlineData;
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
///
/// 统一管理实时分区（当前K线/订单簿）和历史分区（已闭合K线）。
///
/// # 数据流
/// - `write_kline(is_closed=false)` → 实时分区
/// - `write_kline(is_closed=true)` → 实时分区 + 历史分区 + 波动率计算
/// - `write_orderbook()` → 实时分区
pub trait MarketDataStore: Send + Sync {
    // ========== 写入 ==========

    /// 写入K线数据
    ///
    /// - 每次调用触发波动率计算（不管 is_closed）
    /// - is_closed=true 时同时写入历史分区
    fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool);

    /// 写入订单簿
    fn write_orderbook(&self, symbol: &str, orderbook: OrderBookData);

    /// 批量预加载 K线数据（用于沙盒/回测启动时填充历史数据）
    ///
    /// 将 klines 全部写入历史分区，并将最后一条作为当前K线。
    /// Trader 启动时即可读取历史，无需等待逐根 K线闭合。
    fn preload_klines(&self, symbol: &str, klines: Vec<KlineData>) {
        let _ = (symbol, klines);
    }

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
    /// 指标以 JSON Value 形式存储，避免跨 crate 类型依赖
    fn write_indicator(&self, symbol: &str, indicator: serde_json::Value) {
        let _ = (symbol, indicator);
    }

    /// 读取分钟级指标（由 Trader 调用）
    fn get_indicator(&self, symbol: &str) -> Option<serde_json::Value> {
        let _ = symbol;
        None
    }
}
