//! MarketDataStoreImpl - 默认实现
//!
//! 组合 MemoryStore + HistoryStore + VolatilityManager

use std::path::PathBuf;
use std::sync::Arc;

use crate::ws::kline_1m::KlineData;
use super::store_trait::{MarketDataStore, OrderBookData, VolatilityData};
use super::memory_store::MemoryStore;
use super::history_store::HistoryStore;
use super::volatility::VolatilityManager;

/// MarketDataStore 默认实现
pub struct MarketDataStoreImpl {
    memory: Arc<MemoryStore>,
    history: Arc<HistoryStore>,
    volatility: Arc<VolatilityManager>,
}

impl MarketDataStoreImpl {
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("mock_market_store");
        std::fs::create_dir_all(&temp_dir).ok();

        let memory = Arc::new(MemoryStore::new());
        let history = Arc::new(HistoryStore::new(temp_dir));
        let volatility = Arc::new(VolatilityManager::new());

        Self { memory, history, volatility }
    }

    /// 创建带磁盘路径的实例
    pub fn with_path(path: PathBuf) -> Self {
        let memory = Arc::new(MemoryStore::new());
        let history = Arc::new(HistoryStore::new(path));
        let volatility = Arc::new(VolatilityManager::new());

        Self { memory, history, volatility }
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
    fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool) {
        self.memory.write_kline(symbol, kline.clone());
        self.volatility.update(symbol, &kline);

        if is_closed {
            self.history.append_kline(symbol, kline);
        }
    }

    fn write_orderbook(&self, symbol: &str, orderbook: OrderBookData) {
        self.memory.write_orderbook(symbol, orderbook);
    }

    fn get_current_kline(&self, symbol: &str) -> Option<KlineData> {
        self.memory.get_kline(symbol)
    }

    fn get_orderbook(&self, symbol: &str) -> Option<OrderBookData> {
        self.memory.get_orderbook(symbol)
    }

    fn get_history_klines(&self, symbol: &str) -> Vec<KlineData> {
        self.history.get_klines(symbol)
    }

    fn get_history_orderbooks(&self, _symbol: &str) -> Vec<OrderBookData> {
        Vec::new()
    }

    fn get_volatility(&self, symbol: &str) -> Option<VolatilityData> {
        self.volatility.get_volatility(symbol)
    }
}

impl Default for MarketDataStoreImpl {
    fn default() -> Self {
        Self::new()
    }
}
