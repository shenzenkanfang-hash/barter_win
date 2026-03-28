//! 模拟品种注册中心
//!
//! 不依赖 Redis，纯内存实现

use fnv::FnvHashSet;
use tokio::sync::RwLock;

/// 模拟品种注册中心
pub struct SymbolRegistry {
    trading_symbols: RwLock<FnvHashSet<String>>,
    /// 是否已初始化
    initialized: bool,
}

impl SymbolRegistry {
    /// 创建模拟品种注册中心
    pub fn new_mock() -> Self {
        Self {
            trading_symbols: RwLock::new(FnvHashSet::default()),
            initialized: false,
        }
    }

    /// 初始化品种列表（从配置或 CSV）
    pub async fn initialize(&self, symbols: Vec<String>) {
        let mut guard = self.trading_symbols.write().await;
        for symbol in symbols {
            guard.insert(symbol.to_uppercase());
        }
    }

    /// 获取所有交易品种
    pub async fn get_trading_symbols(&self) -> FnvHashSet<String> {
        self.trading_symbols.read().await.clone()
    }

    /// 检查品种是否存在
    pub async fn is_trading(&self, symbol: &str) -> bool {
        self.trading_symbols.read().await.contains(&symbol.to_uppercase())
    }

    /// 添加品种
    pub async fn add_symbol(&self, symbol: &str) {
        self.trading_symbols.write().await.insert(symbol.to_uppercase());
    }
}

impl Default for SymbolRegistry {
    fn default() -> Self {
        Self::new_mock()
    }
}
