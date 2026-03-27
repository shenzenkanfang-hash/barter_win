#![forbid(unsafe_code)]

//! 信号处理器 - 自循环服务
//!
//! 管理分钟级和日级指标计算器，提供数据查询接口
//!
//! 设计：
//! - 1m 品种需要主动注册，后台自循环处理
//! - 日级品种自动管理（无需注册）
//! - TTL 机制自动清理过时的 1m 品种（默认10分钟无更新则移除）
//! - 日级品种上限清理（超过 MAX_DAY_SYMBOLS 时清理最旧的）
//! - 信号缓存机制：缓存最新的 TradingDecision，供 f_engine 拉取

use crate::min::trend::{Indicator1m, Indicator1mOutput};
use crate::day::trend::{BigCycleCalculator, BigCycleIndicators, PineColorBig as DayPineColorBig};
use crate::types::{PineColor, TradingDecision};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// 信号缓存条目
#[derive(Debug, Clone)]
struct SignalCacheEntry {
    decision: TradingDecision,
    timestamp: Instant,
}

/// 信号处理器
pub struct SignalProcessor {
    /// 分钟级指标计算器（按品种索引）
    min_indicators: RwLock<HashMap<String, Indicator1m>>,
    /// 分钟级最新输出缓存（按品种索引）
    min_outputs: RwLock<HashMap<String, Indicator1mOutput>>,
    /// 分钟级最后更新时间（用于TTL清理）
    min_timestamps: RwLock<HashMap<String, Instant>>,
    /// 已注册的分钟级品种
    registered_symbols: RwLock<HashSet<String>>,
    /// 日级指标计算器（按品种索引）
    day_indicators: RwLock<HashMap<String, BigCycleCalculator>>,
    /// 日级指标最后访问时间（用于上限清理）
    day_timestamps: RwLock<HashMap<String, Instant>>,
    /// TTL 时长（默认10分钟）
    ttl: Duration,
    /// 日级指标最大数量（超过则清理最旧的）
    max_day_symbols: usize,
    /// 是否正在运行（用于优雅shutdown）
    running: AtomicBool,

    /// 分钟级信号缓存
    min_signal_cache: RwLock<HashMap<String, SignalCacheEntry>>,
    /// 日线级信号缓存
    day_signal_cache: RwLock<HashMap<String, SignalCacheEntry>>,
}

/// 日级指标最大数量
const MAX_DAY_SYMBOLS: usize = 100;
/// 日级清理阈值（超过此数量时触发清理）
const DAY_CLEANUP_THRESHOLD: usize = 80;

impl SignalProcessor {
    /// 创建信号处理器（默认TTL 10分钟）
    pub fn new() -> Self {
        Self {
            min_indicators: RwLock::new(HashMap::new()),
            min_outputs: RwLock::new(HashMap::new()),
            min_timestamps: RwLock::new(HashMap::new()),
            registered_symbols: RwLock::new(HashSet::new()),
            day_indicators: RwLock::new(HashMap::new()),
            day_timestamps: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(600), // 10分钟
            max_day_symbols: MAX_DAY_SYMBOLS,
            running: AtomicBool::new(false),
            min_signal_cache: RwLock::new(HashMap::new()),
            day_signal_cache: RwLock::new(HashMap::new()),
        }
    }

    /// 创建信号处理器（自定义TTL）
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            min_indicators: RwLock::new(HashMap::new()),
            min_outputs: RwLock::new(HashMap::new()),
            min_timestamps: RwLock::new(HashMap::new()),
            registered_symbols: RwLock::new(HashSet::new()),
            day_indicators: RwLock::new(HashMap::new()),
            day_timestamps: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
            max_day_symbols: MAX_DAY_SYMBOLS,
            running: AtomicBool::new(false),
            min_signal_cache: RwLock::new(HashMap::new()),
            day_signal_cache: RwLock::new(HashMap::new()),
        }
    }

    // ==================== 注册管理 ====================

    /// 注册分钟级品种（1m 需要主动注册才会处理）
    pub fn register_symbol(&self, symbol: &str) {
        let symbol_upper = symbol.to_uppercase();
        let mut registered = self.registered_symbols.write();
        registered.insert(symbol_upper.clone());

        // 初始化计算器（如果不存在）
        let mut indicators = self.min_indicators.write();
        indicators.entry(symbol_upper.clone()).or_insert_with(Indicator1m::new);

        // 初始化输出缓存
        let mut outputs = self.min_outputs.write();
        outputs.entry(symbol_upper.clone()).or_insert_with(Indicator1mOutput::default);

        tracing::debug!("Registered symbol for 1m: {}", symbol_upper);
    }

    /// 取消注册分钟级品种
    pub fn unregister_symbol(&self, symbol: &str) {
        let symbol_upper = symbol.to_uppercase();
        let mut registered = self.registered_symbols.write();
        registered.remove(&symbol_upper);

        let mut indicators = self.min_indicators.write();
        indicators.remove(&symbol_upper);

        let mut outputs = self.min_outputs.write();
        outputs.remove(&symbol_upper);

        let mut timestamps = self.min_timestamps.write();
        timestamps.remove(&symbol_upper);

        tracing::debug!("Unregistered symbol: {}", symbol_upper);
    }

    /// 获取已注册的品种列表
    pub fn registered_symbols(&self) -> Vec<String> {
        self.registered_symbols.read().iter().cloned().collect()
    }

    /// 检查品种是否已注册
    pub fn is_registered(&self, symbol: &str) -> bool {
        self.registered_symbols.read().contains(&symbol.to_uppercase())
    }

    // ==================== 1m 数据更新 ====================

    /// 更新分钟级指标（被外部调用，通常来自 DataFeeder）
    /// 返回 Result: Ok(()) 或 Err(原因)
    pub fn min_update(&self, symbol: &str, high: Decimal, low: Decimal, close: Decimal, volume: Decimal) -> Result<(), String> {
        // 数据验证: high >= low, close 在 [low, high] 范围内
        if high < low {
            return Err(format!("Invalid data: high({}) < low({})", high, low));
        }
        if close < low || close > high {
            return Err(format!("Invalid data: close({}) not in [low({}), high({})]", close, low, high));
        }
        if volume < Decimal::ZERO {
            return Err(format!("Invalid data: volume({}) < 0", volume));
        }

        let symbol_upper = symbol.to_uppercase();

        // 检查是否已注册
        if !self.is_registered(&symbol_upper) {
            return Err(format!("Symbol {} not registered for 1m", symbol_upper));
        }

        let mut indicators = self.min_indicators.write();
        if let Some(indicator) = indicators.get_mut(&symbol_upper) {
            let output = indicator.update(high, low, close, volume);

            // 缓存输出
            let mut outputs = self.min_outputs.write();
            outputs.insert(symbol_upper.clone(), output);

            // 更新 timestamp
            let mut timestamps = self.min_timestamps.write();
            timestamps.insert(symbol_upper, Instant::now());
            Ok(())
        } else {
            Err(format!("Indicator not found for symbol {}", symbol_upper))
        }
    }

    // ==================== 1m 数据查询 ====================

    /// 获取分钟级 TR Ratio (tr_ratio_10min_1h)
    pub fn min_get_tr_ratio(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.tr_ratio_10min_1h)
    }

    /// 获取分钟级 TR Ratio ZScore
    pub fn min_get_tr_ratio_zscore(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.tr_ratio_zscore_10min_1h)
    }

    /// 获取分钟级 Pine 颜色（需要单独计算，Indicator1m 不包含）
    pub fn min_get_pine(&self, _symbol: &str) -> Option<PineColor> {
        // Indicator1m 目前没有 Pine 颜色输出
        // 如需 Pine 颜色，需要扩展 Indicator1mOutput
        None
    }

    /// 获取分钟级 velocity
    pub fn min_get_velocity(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.velocity)
    }

    /// 获取分钟级 acceleration
    pub fn min_get_acceleration(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.acceleration)
    }

    /// 获取分钟级 power
    pub fn min_get_power(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.power)
    }

    /// 获取分钟级 zscore (1h)
    pub fn min_get_zscore_1h(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.zscore_1h_1m)
    }

    /// 获取分钟级 zscore (14)
    pub fn min_get_zscore_14(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.zscore_14_1m)
    }

    /// 获取分钟级位置 (pos_norm_60)
    pub fn min_get_pos_norm_60(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.pos_norm_60)
    }

    /// 获取分钟级 velocity percentile
    pub fn min_get_velocity_percentile(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.velocity_percentile)
    }

    /// 获取分钟级 power percentile
    pub fn min_get_power_percentile(&self, symbol: &str) -> Option<Decimal> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).map(|o| o.power_percentile)
    }

    /// 获取完整的分钟级输出
    pub fn min_get_output(&self, symbol: &str) -> Option<Indicator1mOutput> {
        let outputs = self.min_outputs.read();
        outputs.get(&symbol.to_uppercase()).cloned()
    }

    // ==================== 日级数据更新 ====================

    /// 更新日级指标
    /// 返回 Result: Ok(()) 或 Err(原因)
    pub fn day_update(&self, symbol: &str, high: Decimal, low: Decimal, close: Decimal) -> Result<(), String> {
        // 数据验证: high >= low, close 在 [low, high] 范围内
        if high < low {
            return Err(format!("Invalid data: high({}) < low({})", high, low));
        }
        if close < low || close > high {
            return Err(format!("Invalid data: close({}) not in [low({}), high({})]", close, low, high));
        }

        let symbol_upper = symbol.to_uppercase();

        // 先检查是否需要清理
        self.day_cleanup_if_needed();

        let mut indicators = self.day_indicators.write();
        let indicator = indicators.entry(symbol_upper.clone()).or_insert_with(BigCycleCalculator::new);
        indicator.calculate(high, low, close);

        // 更新访问时间
        drop(indicators);
        let mut timestamps = self.day_timestamps.write();
        timestamps.insert(symbol_upper, Instant::now());

        Ok(())
    }

    /// 检查并清理日级指标（超过阈值时清理最旧的）
    fn day_cleanup_if_needed(&self) {
        let count = {
            let indicators = self.day_indicators.read();
            indicators.len()
        };

        if count > DAY_CLEANUP_THRESHOLD {
            self.day_cleanup_oldest(count - self.max_day_symbols);
        }
    }

    /// 清理最旧的日级指标
    fn day_cleanup_oldest(&self, count: usize) {
        let mut timestamps = self.day_timestamps.write();
        let mut indicators = self.day_indicators.write();

        // 按访问时间排序，保留最新的
        let mut sorted: Vec<_> = timestamps.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1)); // 按时间降序

        let to_remove: Vec<String> = sorted.into_iter()
            .skip(self.max_day_symbols)
            .take(count)
            .map(|(k, _)| k.clone())
            .collect();

        for symbol in &to_remove {
            timestamps.remove(symbol);
            indicators.remove(symbol);
        }

        if !to_remove.is_empty() {
            tracing::debug!("Cleaned up {} oldest day symbols", to_remove.len());
        }
    }

    // ==================== 日级数据查询 ====================

    /// 获取日级 TR Ratio (tr_ratio_5d_20d, tr_ratio_20d_60d)
    pub fn day_get_tr_ratio(&self, symbol: &str) -> Option<(Decimal, Decimal)> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            ind.calculate_tr_ratio()
        })
    }

    /// 获取日级完整 Pine 颜色（包含 12-26、20-50、100-200 三个周期）
    pub fn day_get_pine(&self, symbol: &str) -> Option<BigCycleIndicators> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            let (tr_5d_20d, tr_20d_60d) = ind.calculate_tr_ratio();
            BigCycleIndicators {
                tr_ratio_5d_20d: tr_5d_20d,
                tr_ratio_20d_60d: tr_20d_60d,
                pos_norm_20: ind.calculate_pos_norm_20(),
                ma5_in_20d_ma5_pos: ind.calculate_ma5_in_20d_ma5_pos(),
                ma20_in_60d_ma20_pos: ind.calculate_ma20_in_60d_ma20_pos(),
                pine_color_100_200: ind.detect_pine_color_100_200(),
                pine_color_20_50: ind.detect_pine_color_20_50(),
                pine_color_12_26: ind.detect_pine_color_12_26(),
            }
        })
    }

    /// 获取日级 Pine 颜色 (100/200)
    pub fn day_get_pine_100_200(&self, symbol: &str) -> Option<DayPineColorBig> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            ind.detect_pine_color_100_200()
        })
    }

    /// 获取日级 Pine 颜色 (20/50)
    pub fn day_get_pine_20_50(&self, symbol: &str) -> Option<DayPineColorBig> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            ind.detect_pine_color_20_50()
        })
    }

    /// 获取日级 Pine 颜色 (12/26)
    pub fn day_get_pine_12_26(&self, symbol: &str) -> Option<DayPineColorBig> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            ind.detect_pine_color_12_26()
        })
    }

    /// 获取日级 20 日区间位置
    pub fn day_get_pos_norm_20(&self, symbol: &str) -> Option<Decimal> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            ind.calculate_pos_norm_20()
        })
    }

    /// 获取日级 MA5 在 20 日 MA5 区间位置
    pub fn day_get_ma5_in_20d_ma5_pos(&self, symbol: &str) -> Option<Decimal> {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase()).map(|ind: &BigCycleCalculator| {
            ind.calculate_ma5_in_20d_ma5_pos()
        })
    }

    /// 检查日级指标是否就绪（至少60根K线）
    pub fn day_is_ready(&self, symbol: &str) -> bool {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase())
            .map(|ind| ind.is_ready())
            .unwrap_or(false)
    }

    /// 获取日级指标数据量
    pub fn day_bar_count(&self, symbol: &str) -> usize {
        let indicators = self.day_indicators.read();
        indicators.get(&symbol.to_uppercase())
            .map(|ind| ind.len())
            .unwrap_or(0)
    }

    /// 检查分钟级指标是否就绪
    pub fn min_is_ready(&self, symbol: &str) -> bool {
        let outputs = self.min_outputs.read();
        // 分钟级指标就绪：至少60根K线（需要1小时窗口数据）
        outputs.get(&symbol.to_uppercase())
            .map(|o| o.pos_norm_60 != dec!(50) || o.velocity != Decimal::ZERO)
            .unwrap_or(false)
    }

    // ==================== TTL 清理 ====================

    /// 清理过期的分钟级品种（超过 TTL 未更新则移除）
    pub fn cleanup_expired(&self) -> usize {
        let now = Instant::now();
        let ttl = self.ttl;
        let mut removed = 0;

        // 收集过期的品种
        let symbols_to_remove: Vec<String> = {
            let timestamps = self.min_timestamps.read();
            timestamps
                .iter()
                .filter(|item| now.duration_since(*item.1) > ttl)
                .map(|item| item.0.clone())
                .collect()
        };

        // 移除过期的品种
        if !symbols_to_remove.is_empty() {
            let mut timestamps = self.min_timestamps.write();
            let mut indicators = self.min_indicators.write();
            let mut outputs = self.min_outputs.write();
            let mut registered = self.registered_symbols.write();

            for symbol in &symbols_to_remove {
                timestamps.remove(symbol);
                indicators.remove(symbol);
                outputs.remove(symbol);
                registered.remove(symbol);
                removed += 1;
            }
        }

        if removed > 0 {
            tracing::debug!("Cleaned up {} expired symbols", removed);
        }

        // 同时清理过期的信号缓存
        self.cleanup_expired_signals();

        removed
    }

    /// 获取活跃品种数量（已注册且有数据）
    pub fn active_count(&self) -> usize {
        self.min_indicators.read().len()
    }

    // ==================== 信号缓存 ====================

    /// 设置分钟级信号缓存（由信号生成器调用）
    pub fn set_min_signal(&self, symbol: &str, decision: TradingDecision) {
        let mut cache = self.min_signal_cache.write();
        cache.insert(symbol.to_uppercase(), SignalCacheEntry {
            decision,
            timestamp: Instant::now(),
        });
    }

    /// 获取分钟级信号缓存（供 f_engine 拉取）
    ///
    /// 返回：(信号, 信号生成时间戳的秒数)
    /// 如果缓存过期（超过 TTL），返回 None
    pub fn get_min_signal(&self, symbol: &str) -> Option<(TradingDecision, i64)> {
        let cache = self.min_signal_cache.read();
        cache.get(&symbol.to_uppercase())
            .filter(|entry| Instant::now().duration_since(entry.timestamp) < self.ttl)
            .map(|entry| {
                let ts_secs = entry.timestamp.elapsed().as_secs() as i64;
                (entry.decision.clone(), ts_secs)
            })
    }

    /// 获取分钟级信号年龄（秒）
    pub fn get_min_signal_age_secs(&self, symbol: &str) -> Option<i64> {
        let cache = self.min_signal_cache.read();
        cache.get(&symbol.to_uppercase()).map(|entry| {
            Instant::now().duration_since(entry.timestamp).as_secs() as i64
        })
    }

    /// 设置日线级信号缓存
    pub fn set_day_signal(&self, symbol: &str, decision: TradingDecision) {
        let mut cache = self.day_signal_cache.write();
        cache.insert(symbol.to_uppercase(), SignalCacheEntry {
            decision,
            timestamp: Instant::now(),
        });
    }

    /// 获取日线级信号缓存
    pub fn get_day_signal(&self, symbol: &str) -> Option<(TradingDecision, i64)> {
        let cache = self.day_signal_cache.read();
        cache.get(&symbol.to_uppercase())
            .filter(|entry| Instant::now().duration_since(entry.timestamp) < self.ttl)
            .map(|entry| {
                let ts_secs = entry.timestamp.elapsed().as_secs() as i64;
                (entry.decision.clone(), ts_secs)
            })
    }

    /// 获取日线级信号年龄（秒）
    pub fn get_day_signal_age_secs(&self, symbol: &str) -> Option<i64> {
        let cache = self.day_signal_cache.read();
        cache.get(&symbol.to_uppercase()).map(|entry| {
            Instant::now().duration_since(entry.timestamp).as_secs() as i64
        })
    }

    /// 清理过期的信号缓存
    fn cleanup_expired_signals(&self) {
        let now = Instant::now();

        // 清理分钟级信号缓存
        {
            let mut cache = self.min_signal_cache.write();
            cache.retain(|_, entry| now.duration_since(entry.timestamp) < self.ttl);
        }

        // 清理日线级信号缓存
        {
            let mut cache = self.day_signal_cache.write();
            cache.retain(|_, entry| now.duration_since(entry.timestamp) < self.ttl);
        }
    }

    // ==================== 自循环服务（已废弃） ====================
    //
    // ⚠️ 警告：start_loop 使用 tokio::spawn，违反事件驱动原则
    // ⚠️ 正确做法：调用者显式调用 cleanup_expired()，或使用 channel 事件驱动
    //
    // 事件驱动原则：
    // - 不应有后台自循环任务
    // - 清理逻辑应由调用者驱动（按需或定时调用 cleanup_expired）
    // - 或使用 channel 接收清理事件

    /// 启动后台自循环（已废弃）
    /// 
    /// ⚠️ 已废弃：使用此方法违反事件驱动原则
    /// 
    /// 替代方案：
    /// 1. 调用者定期调用 `cleanup_expired()` 方法
    /// 2. 或使用外部定时器驱动清理
    #[deprecated(since = "2026-03-27", note = "使用 cleanup_expired() 替代后台自循环")]
    pub fn start_loop(self: &Arc<Self>) -> watch::Sender<bool> {
        let processor = Arc::clone(self);
        processor.running.store(true, Ordering::SeqCst);

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        tracing::info!("SignalProcessor started with TTL={}s, max_day_symbols={}",
            self.ttl.as_secs(), self.max_day_symbols);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // 定期清理
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {
                        if !processor.running.load(Ordering::SeqCst) {
                            tracing::info!("SignalProcessor loop received shutdown signal");
                            break;
                        }

                        let removed = processor.cleanup_expired();
                        if removed > 0 {
                            tracing::debug!("SignalProcessor: cleaned up {} expired symbols", removed);
                        }

                        // 日级上限清理
                        processor.day_cleanup_if_needed();
                    }
                    // shutdown 信号
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("SignalProcessor shutting down");
                            break;
                        }
                    }
                }
            }
            processor.running.store(false, Ordering::SeqCst);
            tracing::info!("SignalProcessor loop stopped");
        });

        shutdown_tx
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 优雅停止
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("SignalProcessor stop requested");
    }
}

impl Default for SignalProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Import tokio for async tests
    #[allow(unused_imports)]
    use tokio::test as async_test;

    #[test]
    fn test_register_unregister() {
        let processor = SignalProcessor::new();

        processor.register_symbol("btcusdt");
        assert!(processor.is_registered("btcusdt"));
        assert!(processor.is_registered("BTCUSDT"));
        assert_eq!(processor.active_count(), 1);

        processor.unregister_symbol("btcusdt");
        assert!(!processor.is_registered("btcusdt"));
        assert_eq!(processor.active_count(), 0);
    }

    #[test]
    fn test_day_indicators() {
        let processor = SignalProcessor::new();

        // 模拟日线数据
        for i in 0..100 {
            let base = dec!(100) + Decimal::from(i);
            let high = base + dec!(2);
            let low = base - dec!(2);
            let close = base;
            assert!(processor.day_update("BTCUSDT", high, low, close).is_ok());
        }

        // 验证可以获取指标
        let tr_ratio = processor.day_get_tr_ratio("BTCUSDT");
        assert!(tr_ratio.is_some());

        let pine = processor.day_get_pine_20_50("BTCUSDT");
        assert!(pine.is_some());

        // 验证就绪检查
        assert!(processor.day_is_ready("BTCUSDT"));
        assert_eq!(processor.day_bar_count("BTCUSDT"), 100);
    }

    #[test]
    fn test_day_update_validation() {
        let processor = SignalProcessor::new();

        // 测试 high < low
        let result = processor.day_update("BTCUSDT", dec!(100), dec!(102), dec!(101));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("high"));

        // 测试 close < low
        let result = processor.day_update("BTCUSDT", dec!(102), dec!(100), dec!(99));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("close"));
    }

    #[test]
    fn test_min_update_validation() {
        let processor = SignalProcessor::new();
        processor.register_symbol("btcusdt");

        // 测试 high < low
        let result = processor.min_update("btcusdt", dec!(100), dec!(102), dec!(101), dec!(1000));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("high"));

        // 测试未注册
        processor.unregister_symbol("btcusdt");
        let result = processor.min_update("btcusdt", dec!(102), dec!(100), dec!(101), dec!(1000));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }

    #[test]
    fn test_running_state() {
        // Create a Tokio runtime for this test
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        let processor = Arc::new(SignalProcessor::new());
        assert!(!processor.is_running());

        // Use block_on to run the async start_loop in sync context
        rt.block_on(async {
            let _ = processor.start_loop();
        });
        assert!(processor.is_running());

        processor.stop();
        // stop 后需要等待一下让 loop 退出
        std::thread::sleep(Duration::from_millis(100));
    }
}
