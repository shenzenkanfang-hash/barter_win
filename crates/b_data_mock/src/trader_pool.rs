//! TraderPool - 交易品种池
//!
//! 复制自 b_data_source::trader_pool

use fnv::FnvHashMap;
use fnv::FnvHashSet;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// 品种交易状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingStatus {
    Pending,
    Active,
    Paused,
    Closed,
}

impl Default for TradingStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// 品种元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMeta {
    pub symbol: String,
    pub status: TradingStatus,
    pub priority: u8,
    pub max_position: Option<f64>,
    pub min_qty: f64,
    pub price_precision: u8,
    pub qty_precision: u8,
}

impl SymbolMeta {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into().to_lowercase(),
            status: TradingStatus::Pending,
            priority: 50,
            max_position: None,
            min_qty: 0.001,
            price_precision: 2,
            qty_precision: 3,
        }
    }

    pub fn with_status(mut self, status: TradingStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

impl Default for SymbolMeta {
    fn default() -> Self {
        Self::new("")
    }
}

/// TraderPool
#[derive(Default)]
pub struct TraderPool {
    trading_symbols: RwLock<FnvHashSet<String>>,
    symbol_meta: RwLock<FnvHashMap<String, SymbolMeta>>,
}

impl TraderPool {
    pub fn new() -> Self {
        Self {
            trading_symbols: RwLock::new(FnvHashSet::default()),
            symbol_meta: RwLock::new(FnvHashMap::default()),
        }
    }

    pub fn register(&self, meta: SymbolMeta) {
        let symbol = meta.symbol.clone();
        self.trading_symbols.write().insert(symbol.clone());
        self.symbol_meta.write().insert(symbol, meta);
    }

    pub fn unregister(&self, symbol: &str) {
        let symbol_lower = symbol.to_lowercase();
        self.trading_symbols.write().remove(&symbol_lower);
        self.symbol_meta.write().remove(&symbol_lower);
    }

    pub fn update_status(&self, symbol: &str, status: TradingStatus) {
        let symbol_lower = symbol.to_lowercase();
        let mut meta_map = self.symbol_meta.write();
        if let Some(meta) = meta_map.get_mut(&symbol_lower) {
            meta.status = status;
        }
    }

    pub fn get_trading_symbols(&self) -> Vec<String> {
        self.trading_symbols.read().iter().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.trading_symbols.read().len()
    }

    pub fn is_trading(&self, symbol: &str) -> bool {
        self.trading_symbols.read().contains(&symbol.to_lowercase())
    }

    pub fn is_active(&self, symbol: &str) -> bool {
        let symbol_lower = symbol.to_lowercase();
        let meta_map = self.symbol_meta.read();
        meta_map.get(&symbol_lower)
            .map(|m| m.status == TradingStatus::Active)
            .unwrap_or(false)
    }

    pub fn get_meta(&self, symbol: &str) -> Option<SymbolMeta> {
        self.symbol_meta.read().get(&symbol.to_lowercase()).cloned()
    }

    pub fn get_all_meta(&self) -> Vec<SymbolMeta> {
        self.symbol_meta.read().values().cloned().collect()
    }

    pub fn clear(&self) {
        self.trading_symbols.write().clear();
        self.symbol_meta.write().clear();
    }
}
