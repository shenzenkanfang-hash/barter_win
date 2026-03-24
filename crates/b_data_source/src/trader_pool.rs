//! TraderPool - 交易品种池
//!
//! 管理激活的交易品种，只处理池中品种的 Tick。
//! 核心设计：不是全市场扫描，而是只处理激活的品种。

use fnv::FnvHashMap;
use fnv::FnvHashSet;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// 品种交易状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingStatus {
    /// 待激活
    Pending,
    /// 正常交易
    Active,
    /// 暂停
    Paused,
    /// 已平仓
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
    /// 品种名称（统一小写存储）
    pub symbol: String,
    /// 交易状态
    pub status: TradingStatus,
    /// 优先级 (0-100)
    pub priority: u8,
    /// 最大持仓数量
    pub max_position: Option<f64>,
    /// 最小交易数量
    pub min_qty: f64,
    /// 价格精度
    pub price_precision: u8,
    /// 数量精度
    pub qty_precision: u8,
}

impl SymbolMeta {
    /// 创建品种元数据
    ///
    /// 注意：symbol 会自动转为小写
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into().to_lowercase(),  // 统一小写存储
            status: TradingStatus::Pending,
            priority: 50,
            max_position: None,
            min_qty: 0.001,
            price_precision: 2,
            qty_precision: 3,
        }
    }

    /// 设置交易状态
    pub fn with_status(mut self, status: TradingStatus) -> Self {
        self.status = status;
        self
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// 设置最大持仓
    pub fn with_max_position(mut self, max_position: f64) -> Self {
        self.max_position = Some(max_position);
        self
    }
}

impl Default for SymbolMeta {
    fn default() -> Self {
        Self::new("")
    }
}

/// TraderPool - 交易品种池
///
/// 管理激活的交易品种，提供：
/// - 品种注册/注销
/// - 品种状态管理
/// - 品种过滤
///
/// 线程安全: 使用 RwLock 保护
#[derive(Default)]
pub struct TraderPool {
    /// 激活的交易品种集合
    trading_symbols: RwLock<FnvHashSet<String>>,
    /// 品种元数据
    symbol_meta: RwLock<FnvHashMap<String, SymbolMeta>>,
}

impl TraderPool {
    /// 创建新的 TraderPool
    pub fn new() -> Self {
        Self {
            trading_symbols: RwLock::new(FnvHashSet::default()),
            symbol_meta: RwLock::new(FnvHashMap::default()),
        }
    }

    /// 注册交易品种
    ///
    /// 如果品种已存在，则更新元数据
    /// 注意：symbol 会自动转为小写
    pub fn register(&self, meta: SymbolMeta) {
        let symbol = meta.symbol.clone();
        self.trading_symbols.write().insert(symbol.clone());
        self.symbol_meta.write().insert(symbol, meta);
    }

    /// 注册多个品种（批量操作，减少锁竞争）
    pub fn register_batch(&self, symbols: impl IntoIterator<Item = SymbolMeta>) {
        let mut trading = self.trading_symbols.write();
        let mut meta_map = self.symbol_meta.write();
        
        for meta in symbols {
            let symbol = meta.symbol.clone();
            trading.insert(symbol.clone());
            meta_map.insert(symbol, meta);
        }
    }

    /// 注销交易品种
    pub fn unregister(&self, symbol: &str) {
        let symbol_lower = symbol.to_lowercase();
        self.trading_symbols.write().remove(&symbol_lower);
        self.symbol_meta.write().remove(&symbol_lower);
    }

    /// 更新品种状态
    pub fn update_status(&self, symbol: &str, status: TradingStatus) {
        let symbol_lower = symbol.to_lowercase();
        let mut meta_map = self.symbol_meta.write();
        if let Some(meta) = meta_map.get_mut(&symbol_lower) {
            meta.status = status;
        }
    }

    /// 获取所有激活品种
    pub fn get_trading_symbols(&self) -> Vec<String> {
        self.trading_symbols.read().iter().cloned().collect()
    }

    /// 获取激活品种数量
    pub fn count(&self) -> usize {
        self.trading_symbols.read().len()
    }

    /// 检查品种是否激活（已注册）
    pub fn is_trading(&self, symbol: &str) -> bool {
        self.trading_symbols.read().contains(&symbol.to_lowercase())
    }

    /// 检查品种是否激活且状态为 Active
    pub fn is_active(&self, symbol: &str) -> bool {
        let symbol_lower = symbol.to_lowercase();
        let meta_map = self.symbol_meta.read();
        meta_map.get(&symbol_lower)
            .map(|m| m.status == TradingStatus::Active)
            .unwrap_or(false)
    }

    /// 获取品种元数据
    pub fn get_meta(&self, symbol: &str) -> Option<SymbolMeta> {
        self.symbol_meta.read().get(&symbol.to_lowercase()).cloned()
    }

    /// 获取品种状态
    pub fn get_status(&self, symbol: &str) -> Option<TradingStatus> {
        self.symbol_meta.read().get(&symbol.to_lowercase()).map(|m| m.status)
    }

    /// 获取所有品种元数据
    pub fn get_all_meta(&self) -> Vec<SymbolMeta> {
        self.symbol_meta.read().values().cloned().collect()
    }

    /// 获取指定状态的品种
    pub fn get_by_status(&self, status: TradingStatus) -> Vec<String> {
        let meta_map = self.symbol_meta.read();
        meta_map.iter()
            .filter(|(_, m)| m.status == status)
            .map(|(s, _)| s.clone())
            .collect()
    }

    /// 清空所有品种
    pub fn clear(&self) {
        self.trading_symbols.write().clear();
        self.symbol_meta.write().clear();
    }

    /// 暂停所有品种
    pub fn pause_all(&self) {
        let mut meta_map = self.symbol_meta.write();
        for meta in meta_map.values_mut() {
            if meta.status == TradingStatus::Active {
                meta.status = TradingStatus::Paused;
            }
        }
    }

    /// 激活所有待激活品种
    pub fn activate_all(&self) {
        let mut meta_map = self.symbol_meta.write();
        for meta in meta_map.values_mut() {
            if meta.status == TradingStatus::Pending || meta.status == TradingStatus::Paused {
                meta.status = TradingStatus::Active;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register() {
        let pool = TraderPool::new();
        pool.register(SymbolMeta::new("BTCUSDT").with_status(TradingStatus::Active));
        
        assert!(pool.is_trading("BTCUSDT"));
        assert!(pool.is_active("BTCUSDT"));
        assert_eq!(pool.count(), 1);
    }

    #[test]
    fn test_symbol_normalization() {
        let pool = TraderPool::new();
        pool.register(SymbolMeta::new("BTCUSDT"));
        
        // 大小写不敏感
        assert!(pool.is_trading("btcusdt"));
        assert!(pool.is_trading("BTCUSDT"));
        assert!(pool.is_trading("BtcUsdt"));
    }

    #[test]
    fn test_unregister() {
        let pool = TraderPool::new();
        pool.register(SymbolMeta::new("BTCUSDT").with_status(TradingStatus::Active));
        pool.unregister("BTCUSDT");
        
        assert!(!pool.is_trading("BTCUSDT"));
        assert_eq!(pool.count(), 0);
    }

    #[test]
    fn test_status_update() {
        let pool = TraderPool::new();
        pool.register(SymbolMeta::new("BTCUSDT").with_status(TradingStatus::Pending));
        
        assert!(!pool.is_active("BTCUSDT"));
        
        pool.update_status("BTCUSDT", TradingStatus::Active);
        assert!(pool.is_active("BTCUSDT"));
    }

    #[test]
    fn test_get_trading_symbols() {
        let pool = TraderPool::new();
        pool.register(SymbolMeta::new("BTCUSDT").with_status(TradingStatus::Active));
        pool.register(SymbolMeta::new("ETHUSDT").with_status(TradingStatus::Active));
        
        let symbols = pool.get_trading_symbols();
        assert!(symbols.contains(&"btcusdt".to_string()));
        assert!(symbols.contains(&"ethusdt".to_string()));
    }

    #[test]
    fn test_get_by_status() {
        let pool = TraderPool::new();
        pool.register(SymbolMeta::new("BTCUSDT").with_status(TradingStatus::Active));
        pool.register(SymbolMeta::new("ETHUSDT").with_status(TradingStatus::Pending));
        pool.register(SymbolMeta::new("BNBUSDT").with_status(TradingStatus::Active));
        
        let active = pool.get_by_status(TradingStatus::Active);
        assert_eq!(active.len(), 2);
        
        let pending = pool.get_by_status(TradingStatus::Pending);
        assert_eq!(pending.len(), 1);
    }
}
