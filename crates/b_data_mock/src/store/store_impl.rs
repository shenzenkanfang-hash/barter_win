//! MarketDataStoreImpl - 默认实现
//!
//! 组合 MemoryStore + HistoryStore + VolatilityManager

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ws::kline_1m::KlineData as MockKlineData;
use super::store_trait::{MarketDataStore, OrderBookData, VolatilityData};
use super::memory_store::MemoryStore;
use super::history_store::HistoryStore;
use super::volatility::VolatilityManager;

// 导入 b_data_source 的 trait 和数据类型（用于跨 crate 兼容）
use b_data_source::store::{
    MarketDataStore as BMarketDataStore,
    OrderBookData as BOrderBookData,
    VolatilityData as BVolatilityData,
};
use b_data_source::ws::kline_1m::ws::KlineData as BKlineData;

/// MarketDataStore 默认实现
pub struct MarketDataStoreImpl {
    memory: Arc<MemoryStore>,
    history: Arc<HistoryStore>,
    volatility: Arc<VolatilityManager>,
    /// NO_SIGNAL 修复：分钟级指标存储（由 SignalProcessor 写入，Trader 读取）
    /// 使用 parking_lot::RwLock 提供 interior mutability
    indicators: parking_lot::RwLock<HashMap<String, serde_json::Value>>,
}

impl MarketDataStoreImpl {
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("mock_market_store");
        std::fs::create_dir_all(&temp_dir).ok();

        let memory = Arc::new(MemoryStore::new());
        let history = Arc::new(HistoryStore::new(temp_dir));
        let volatility = Arc::new(VolatilityManager::new());
        let indicators = parking_lot::RwLock::new(HashMap::new());

        Self { memory, history, volatility, indicators }
    }

    /// 创建带磁盘路径的实例
    pub fn with_path(path: PathBuf) -> Self {
        let memory = Arc::new(MemoryStore::new());
        let history = Arc::new(HistoryStore::new(path));
        let volatility = Arc::new(VolatilityManager::new());
        let indicators = parking_lot::RwLock::new(HashMap::new());

        Self { memory, history, volatility, indicators }
    }

    /// 获取内部 MemoryStore
    pub fn memory(&self) -> &MemoryStore {
        &self.memory
    }

    /// 获取内部 HistoryStore
    pub fn history(&self) -> &HistoryStore {
        &self.history
    }
}

impl MarketDataStore for MarketDataStoreImpl {
    fn write_kline(&self, symbol: &str, kline: MockKlineData, is_closed: bool) {
        self.memory.write_kline(symbol, kline.clone());
        self.volatility.update(symbol, &kline);

        if is_closed {
            self.history.append_kline(symbol, kline);
        }
    }

    fn write_orderbook(&self, symbol: &str, orderbook: OrderBookData) {
        self.memory.write_orderbook(symbol, orderbook);
    }

    fn get_current_kline(&self, symbol: &str) -> Option<MockKlineData> {
        self.memory.get_kline(symbol)
    }

    fn get_orderbook(&self, symbol: &str) -> Option<OrderBookData> {
        self.memory.get_orderbook(symbol)
    }

    fn get_history_klines(&self, symbol: &str) -> Vec<MockKlineData> {
        self.history.get_klines(symbol)
    }

    fn get_history_orderbooks(&self, _symbol: &str) -> Vec<OrderBookData> {
        Vec::new()
    }

    fn get_volatility(&self, symbol: &str) -> Option<VolatilityData> {
        self.volatility.get_volatility(symbol)
    }

    fn write_indicator(&self, symbol: &str, indicator: serde_json::Value) {
        self.indicators.write().insert(symbol.to_uppercase(), indicator);
    }

    fn get_indicator(&self, symbol: &str) -> Option<serde_json::Value> {
        self.indicators.read().get(&symbol.to_uppercase()).cloned()
    }
}

// =============================================================================
// b_data_source::store::MarketDataStore 跨 crate 实现
// =============================================================================
// 让 b_data_mock::MarketDataStoreImpl 也能作为 b_data_source 的 StoreRef 使用

impl BMarketDataStore for MarketDataStoreImpl {
    fn write_kline(&self, symbol: &str, kline: BKlineData, is_closed: bool) {
        // 将 b_data_source 的 KlineData 转换为 b_data_mock 的内部格式
        let mock_kline = MockKlineData {
            kline_start_time: kline.kline_start_time,
            kline_close_time: kline.kline_close_time,
            symbol: kline.symbol.clone(),
            interval: kline.interval.clone(),
            open: kline.open,
            close: kline.close,
            high: kline.high,
            low: kline.low,
            volume: kline.volume,
            is_closed: kline.is_closed,
        };
        self.memory.write_kline(symbol, mock_kline.clone());
        self.volatility.update(symbol, &mock_kline);

        if is_closed {
            self.history.append_kline(symbol, mock_kline);
        }
    }

    fn write_orderbook(&self, symbol: &str, orderbook: BOrderBookData) {
        let mock_orderbook = OrderBookData {
            symbol: orderbook.symbol.clone(),
            bids: orderbook.bids.clone(),
            asks: orderbook.asks.clone(),
            timestamp_ms: orderbook.timestamp_ms,
        };
        self.memory.write_orderbook(symbol, mock_orderbook);
    }

    fn get_current_kline(&self, symbol: &str) -> Option<BKlineData> {
        self.memory.get_kline(symbol).map(|k| BKlineData {
            kline_start_time: k.kline_start_time,
            kline_close_time: k.kline_close_time,
            symbol: k.symbol.clone(),
            interval: k.interval.clone(),
            open: k.open,
            close: k.close,
            high: k.high,
            low: k.low,
            volume: k.volume,
            is_closed: k.is_closed,
        })
    }

    fn get_orderbook(&self, symbol: &str) -> Option<BOrderBookData> {
        self.memory.get_orderbook(symbol).map(|o| BOrderBookData {
            symbol: o.symbol.clone(),
            bids: o.bids.clone(),
            asks: o.asks.clone(),
            timestamp_ms: o.timestamp_ms,
        })
    }

    fn get_history_klines(&self, symbol: &str) -> Vec<BKlineData> {
        self.history.get_klines(symbol).into_iter().map(|k| BKlineData {
            kline_start_time: k.kline_start_time,
            kline_close_time: k.kline_close_time,
            symbol: k.symbol.clone(),
            interval: k.interval.clone(),
            open: k.open,
            close: k.close,
            high: k.high,
            low: k.low,
            volume: k.volume,
            is_closed: k.is_closed,
        }).collect()
    }

    fn get_history_orderbooks(&self, symbol: &str) -> Vec<BOrderBookData> {
        // b_data_mock 不存储历史订单簿
        Vec::new()
    }

    fn get_volatility(&self, symbol: &str) -> Option<BVolatilityData> {
        self.volatility.get_volatility(symbol).map(|v| BVolatilityData {
            symbol: v.symbol.clone(),
            volatility: v.volatility,
            update_time_ms: v.update_time_ms,
        })
    }

    fn write_indicator(&self, symbol: &str, indicator: serde_json::Value) {
        self.indicators.write().insert(symbol.to_uppercase(), indicator);
    }

    fn get_indicator(&self, symbol: &str) -> Option<serde_json::Value> {
        self.indicators.read().get(&symbol.to_uppercase()).cloned()
    }
}

impl Default for MarketDataStoreImpl {
    fn default() -> Self {
        Self::new()
    }
}
