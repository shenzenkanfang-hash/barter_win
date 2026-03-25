//! StrategyExecutor - 策略调度器
//!
//! 负责策略注册、调度和信号聚合。

#![forbid(unsafe_code)]

use crate::strategy::{Direction, Strategy, StrategyKLine, StrategyState, TradingSignal};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// 策略调度器
///
/// 负责：
/// - 策略注册/注销
/// - K 线分发到对应策略
/// - 信号缓存和聚合
///
/// 线程安全：使用 RwLock 保护内部状态
pub struct StrategyExecutor {
    /// 策略 Map: strategy_id -> Arc<dyn Strategy>
    strategies: RwLock<FnvHashMap<String, Arc<dyn Strategy>>>,
    /// 品种到策略的映射: symbol -> Vec<strategy_id>
    symbol_strategies: RwLock<FnvHashMap<String, Vec<String>>>,
    /// 信号缓存: "symbol:strategy_id" -> TradingSignal
    signal_cache: RwLock<FnvHashMap<String, TradingSignal>>,
}

impl Default for StrategyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyExecutor {
    /// 创建新的调度器
    pub fn new() -> Self {
        Self {
            strategies: RwLock::new(FnvHashMap::default()),
            symbol_strategies: RwLock::new(FnvHashMap::default()),
            signal_cache: RwLock::new(FnvHashMap::default()),
        }
    }

    /// 注册策略
    ///
    /// # Panics
    /// 如果策略 ID 已存在会覆盖旧策略
    pub fn register(&self, strategy: Arc<dyn Strategy>) {
        let id = strategy.id().to_string();
        let symbols = strategy.symbols();

        self.strategies.write().insert(id.clone(), strategy);

        // 更新品种-策略映射
        let mut symbol_map = self.symbol_strategies.write();
        for symbol in symbols {
            symbol_map
                .entry(symbol)
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        info!("Registered strategy: {}", id);
    }

    /// 注销策略
    pub fn unregister(&self, strategy_id: &str) {
        self.strategies.write().remove(strategy_id);

        // 从品种映射中移除
        let mut symbol_map = self.symbol_strategies.write();
        for (_, strategies) in symbol_map.iter_mut() {
            strategies.retain(|s| s != strategy_id);
        }

        info!("Unregistered strategy: {}", strategy_id);
    }

    /// 分发 K 线到对应策略
    ///
    /// 优化：减少锁竞争，返回结果而非引用
    #[instrument(skip(self), fields(symbol = %bar.symbol))]
    pub fn dispatch(&self, bar: &StrategyKLine) -> Vec<TradingSignal> {
        let symbol = &bar.symbol;

        // 获取策略 ID 列表（克隆避免长持锁）
        let strategy_ids = {
            let symbol_map = self.symbol_strategies.read();
            symbol_map.get(symbol).cloned().unwrap_or_default()
        };

        if strategy_ids.is_empty() {
            debug!("No strategies registered for {}", symbol);
            return Vec::new();
        }

        let mut signals = Vec::new();

        for strategy_id in strategy_ids {
            // 获取策略（短暂持锁）
            let strategy = {
                let strategies = self.strategies.read();
                strategies.get(&strategy_id).cloned()
            };

            if let Some(strategy) = strategy {
                // 检查启用状态
                if !strategy.is_enabled() {
                    continue;
                }

                // 调用策略处理
                if let Some(signal) = strategy.on_bar(bar) {
                    debug!(
                        "Strategy {} generated signal: {:?} {} {}",
                        strategy_id, signal.direction, signal.symbol, signal.quantity
                    );

                    // 缓存信号
                    let cache_key = format!("{}:{}", symbol, strategy_id);
                    self.signal_cache.write().insert(cache_key, signal.clone());

                    signals.push(signal);
                }
            }
        }

        signals
    }

    /// 获取最高优先级信号
    pub fn get_signal(&self, symbol: &str) -> Option<TradingSignal> {
        let cache = self.signal_cache.read();

        cache
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("{}:", symbol)))
            .max_by_key(|(_, s)| s.priority)
            .map(|(_, s)| s.clone())
    }

    /// 获取指定策略的信号
    pub fn get_signal_for_strategy(&self, symbol: &str, strategy_id: &str) -> Option<TradingSignal> {
        let cache_key = format!("{}:{}", symbol, strategy_id);
        self.signal_cache.read().get(&cache_key).cloned()
    }

    /// 获取所有缓存信号
    pub fn get_all_signals(&self) -> Vec<TradingSignal> {
        self.signal_cache.read().values().cloned().collect()
    }

    /// 清理过期信号
    pub fn clear_stale_signals(&self, max_age_secs: i64) {
        let now = chrono::Utc::now().timestamp();
        let mut cache = self.signal_cache.write();

        cache.retain(|_, signal| {
            now - signal.timestamp.timestamp() < max_age_secs
        });
    }

    /// 获取策略状态
    pub fn get_strategy_state(&self, strategy_id: &str) -> Option<StrategyState> {
        let strategies = self.strategies.read();
        strategies.get(strategy_id).map(|s| s.state().clone())
    }

    /// 获取所有策略状态
    pub fn get_all_states(&self) -> Vec<StrategyState> {
        self.strategies
            .read()
            .values()
            .map(|s| s.state().clone())
            .collect()
    }

    /// 设置策略启用状态
    ///
    /// 修复：实际修改策略状态，而非空操作
    pub fn set_enabled(&self, strategy_id: &str, enabled: bool) {
        let strategies = self.strategies.read();
        if let Some(strategy) = strategies.get(strategy_id) {
            strategy.state().set_enabled(enabled);
            debug!("Strategy {} enabled: {}", strategy_id, enabled);
        }
    }

    /// 策略数量
    pub fn count(&self) -> usize {
        self.strategies.read().len()
    }

    /// 关注的品种数量
    pub fn symbol_count(&self) -> usize {
        self.symbol_strategies.read().len()
    }

    /// 清空所有数据
    pub fn clear(&self) {
        self.strategies.write().clear();
        self.symbol_strategies.write().clear();
        self.signal_cache.write().clear();
    }
}

/// 信号聚合器
///
/// 将多个信号按规则聚合成最终执行信号
pub struct SignalAggregator {
    /// 最大信号数量
    max_signals: usize,
}

impl Default for SignalAggregator {
    fn default() -> Self {
        Self::new(10)
    }
}

impl SignalAggregator {
    pub fn new(max_signals: usize) -> Self {
        Self { max_signals }
    }

    /// 聚合信号
    ///
    /// 规则：
    /// 1. 同一品种同一方向只保留数量最大的信号
    /// 2. 按优先级排序
    /// 3. 同一品种同一方向只保留一个信号
    ///
    /// 注意：相反方向的信号都会保留
    pub fn aggregate(&self, signals: Vec<TradingSignal>) -> Vec<TradingSignal> {
        if signals.is_empty() {
            return Vec::new();
        }

        // 按 (symbol, direction) 分组，保留数量最大的信号
        let mut grouped: FnvHashMap<(String, Direction), TradingSignal> = FnvHashMap::default();

        for signal in signals {
            let key = (signal.symbol.clone(), signal.direction);
            
            grouped
                .entry(key)
                .and_modify(|existing| {
                    if signal.quantity > existing.quantity {
                        *existing = signal.clone();
                    }
                })
                .or_insert(signal);
        }

        // 按优先级排序
        let mut sorted: Vec<_> = grouped.into_values().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        // 去重：每个 (symbol, direction) 只保留一个
        // 注意：Long 和 Short 是不同的 direction，所以都会保留
        let mut result: Vec<TradingSignal> = Vec::new();
        let mut seen_keys: FnvHashMap<(String, Direction), bool> = FnvHashMap::default();

        for signal in sorted {
            let key = (signal.symbol.clone(), signal.direction);
            
            if !seen_keys.contains_key(&key) {
                result.push(signal);
            }
            seen_keys.insert(key, true);

            if result.len() >= self.max_signals {
                break;
            }
        }

        result
    }
}
