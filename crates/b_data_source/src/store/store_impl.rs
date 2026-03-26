//! MarketDataStoreImpl - 默认实现
//!
//! 组合 MemoryStore + HistoryStore + VolatilityManager

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use a_common::Paths;
use crate::ws::kline_1m::ws::KlineData;

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
        let paths = Paths::new();
        let disk_path = PathBuf::from(&paths.memory_backup_dir).join("market_store");
        
        let memory = Arc::new(MemoryStore::new());
        let history = Arc::new(HistoryStore::new(disk_path));
        let volatility = Arc::new(VolatilityManager::new());
        
        // 从历史分区恢复实时分区最新K线
        let all_history: HashMap<String, Vec<KlineData>> = history.get_all();
        for (symbol, klines) in all_history {
            if let Some(last) = klines.last() {
                memory.write_kline(&symbol, last.clone());
            }
        }
        
        Self { memory, history, volatility }
    }

    /// 创建测试实例（使用临时目录）
    #[cfg(test)]
    pub fn new_test() -> Self {
        let temp_dir = std::env::temp_dir().join("market_store_test");
        std::fs::create_dir_all(&temp_dir).ok();
        
        let memory = Arc::new(MemoryStore::new());
        let history = Arc::new(HistoryStore::new(temp_dir));
        let volatility = Arc::new(VolatilityManager::new());
        
        Self { memory, history, volatility }
    }
}

impl MarketDataStore for MarketDataStoreImpl {
    fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool) {
        // 1. 写入实时分区
        self.memory.write_kline(symbol, kline.clone());
        
        // 2. 触发波动率计算（每次都计算，不管是否闭合）
        self.volatility.update(symbol, &kline);
        
        // 3. 闭合时写入历史分区
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

    fn get_history_orderbooks(&self, symbol: &str) -> Vec<OrderBookData> {
        self.history.get_orderbooks(symbol)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_kline() {
        let store = MarketDataStoreImpl::new_test();
        
        let kline = KlineData {
            kline_start_time: 1000,
            kline_close_time: 2000,
            symbol: "BTCUSDT".to_string(),
            interval: "1m".to_string(),
            open: "100.0".to_string(),
            close: "105.0".to_string(),
            high: "110.0".to_string(),
            low: "95.0".to_string(),
            volume: "1000.0".to_string(),
            is_closed: false,
        };
        
        store.write_kline("BTCUSDT", kline.clone(), false);
        
        let retrieved = store.get_current_kline("BTCUSDT");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().close, "105.0");
    }

    #[test]
    fn test_closed_kline_写入_history() {
        let store = MarketDataStoreImpl::new_test();
        
        let kline = KlineData {
            kline_start_time: 1000,
            kline_close_time: 2000,
            symbol: "ETHUSDT".to_string(),
            interval: "1m".to_string(),
            open: "2000.0".to_string(),
            close: "2100.0".to_string(),
            high: "2150.0".to_string(),
            low: "1950.0".to_string(),
            volume: "500.0".to_string(),
            is_closed: true,
        };
        
        store.write_kline("ETHUSDT", kline.clone(), true);
        
        // 实时分区应该有数据
        assert!(store.get_current_kline("ETHUSDT").is_some());
        
        // 历史分区应该有数据
        let history = store.get_history_klines("ETHUSDT");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].close, "2100.0");
    }

    #[test]
    fn test_volatility_update() {
        let store = MarketDataStoreImpl::new_test();
        
        for i in 0..5 {
            let kline = KlineData {
                kline_start_time: i as i64 * 60000,
                kline_close_time: (i as i64 + 1) * 60000,
                symbol: "BTCUSDT".to_string(),
                interval: "1m".to_string(),
                open: format!("{}.0", 100 + i),
                close: format!("{}.0", 105 + i),
                high: format!("{}.0", 110 + i),
                low: format!("{}.0", 95 + i),
                volume: "1000.0".to_string(),
                is_closed: true,
            };
            store.write_kline("BTCUSDT", kline, true);
        }
        
        let vol = store.get_volatility("BTCUSDT");
        assert!(vol.is_some());
        assert!(vol.unwrap().volatility > 0.0);
    }
}
