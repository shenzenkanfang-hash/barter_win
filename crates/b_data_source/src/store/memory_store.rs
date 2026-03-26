//! MemoryStore - 实时分区实现
//!
//! 内存存储当前K线和订单簿。

use std::collections::HashMap;
use parking_lot::RwLock;

use crate::ws::kline_1m::ws::KlineData;
use super::store_trait::OrderBookData;

/// 实时分区：当前K线和订单簿
pub struct MemoryStore {
    klines: RwLock<HashMap<String, KlineData>>,
    orderbooks: RwLock<HashMap<String, OrderBookData>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            klines: RwLock::new(HashMap::new()),
            orderbooks: RwLock::new(HashMap::new()),
        }
    }

    pub fn write_kline(&self, symbol: &str, kline: KlineData) {
        let symbol_lower = symbol.to_lowercase();
        self.klines.write().insert(symbol_lower, kline);
    }

    pub fn write_orderbook(&self, symbol: &str, orderbook: OrderBookData) {
        let symbol_lower = symbol.to_lowercase();
        self.orderbooks.write().insert(symbol_lower, orderbook);
    }

    pub fn get_kline(&self, symbol: &str) -> Option<KlineData> {
        let symbol_lower = symbol.to_lowercase();
        self.klines.read().get(&symbol_lower).cloned()
    }

    pub fn get_orderbook(&self, symbol: &str) -> Option<OrderBookData> {
        let symbol_lower = symbol.to_lowercase();
        self.orderbooks.read().get(&symbol_lower).cloned()
    }

    pub fn get_all_klines(&self) -> HashMap<String, KlineData> {
        self.klines.read().clone()
    }

    pub fn get_all_orderbooks(&self) -> HashMap<String, OrderBookData> {
        self.orderbooks.read().clone()
    }

    pub fn clear(&self) {
        self.klines.write().clear();
        self.orderbooks.write().clear();
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}
