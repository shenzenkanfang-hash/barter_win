//! 高波动检测模块
//!
//! 每 tick 触发计算，维护 15m 滚动窗口
//! - 1m 3% / 15m 13% 高波动判断
//! - 进入高波动时触发日志
//! - 每分钟汇总所有高波动品种
//! - 窗口灾备：内存盘 → 同步盘 → 自行累计

use a_common::config::Paths;
use a_common::volatility::{KLineInput, VolatilityCalc, VolatilityStats, VolatilityState, VolatilityRank, VolatilityEntry};
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 单个品种的高波动检测器
pub struct SymbolVolatility {
    symbol: String,
    calc: VolatilityCalc,
    /// 上次是否高波动（用于检测进入/退出）
    was_high_volatility: bool,
    /// 当前统计值（用于日志输出）
    current_stats: VolatilityStats,
}

impl SymbolVolatility {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            calc: VolatilityCalc::new(),
            was_high_volatility: false,
            current_stats: VolatilityStats::default(),
        }
    }

    /// 从内存盘加载灾备
    pub fn load_from_memory(&mut self) -> bool {
        let paths = Paths::new();
        let path = Self::memory_path(&paths, &self.symbol);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str::<VolatilityState>(&content) {
                    self.calc = VolatilityCalc::restore(state);
                    if self.calc.is_valid() {
                        tracing::info!("[VOL] Loaded from memory: {}", self.symbol);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 从同步盘加载灾备
    pub fn load_from_sync(&mut self) -> bool {
        let paths = Paths::new();
        let path = Self::sync_path(&paths, &self.symbol);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str::<VolatilityState>(&content) {
                    self.calc = VolatilityCalc::restore(state);
                    if self.calc.is_valid() {
                        tracing::info!("[VOL] Loaded from sync: {}", self.symbol);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 尝试所有加载方式：内存 → 同步 → 自行累计
    pub fn try_load(&mut self) {
        if !self.load_from_memory() {
            if !self.load_from_sync() {
                tracing::info!("[VOL] No valid backup, starting fresh: {}", self.symbol);
            }
        }
    }

    /// 每 tick 更新
    pub fn update(&mut self, kline: KLineInput) -> VolatilityStats {
        let stats = self.calc.update(kline);
        self.current_stats = stats;

        // 检测是否进入高波动（1m 或 15m）
        let is_1m_high = stats.vol_1m >= self.calc.thresholds().0;
        let is_15m_high = stats.vol_15m >= self.calc.thresholds().1;

        if is_1m_high && !self.was_high_volatility {
            tracing::warn!(
                "[HIGH_VOL] 🔴 1m进入 | {} 1m={:.2}% 15m={:.2}% | {}",
                self.symbol,
                stats.vol_1m * dec!(100),
                stats.vol_15m * dec!(100),
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        } else if is_15m_high && !self.was_high_volatility {
            tracing::warn!(
                "[HIGH_VOL] 🟠 15m进入 | {} 1m={:.2}% 15m={:.2}% | {}",
                self.symbol,
                stats.vol_1m * dec!(100),
                stats.vol_15m * dec!(100),
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        }

        self.was_high_volatility = is_1m_high || is_15m_high;
        stats
    }

    /// 保存窗口到内存盘（K线闭合时调用）
    pub fn save_window_to_memory(&self) {
        let paths = Paths::new();
        let path = Self::memory_path(&paths, &self.symbol);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let state = self.calc.get_state();
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = std::fs::write(&path, json);
            tracing::debug!("[VOL] Saved window to memory: {}", self.symbol);
        }
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn is_high_volatility(&self) -> bool {
        self.was_high_volatility
    }

    pub fn get_stats(&self) -> VolatilityStats {
        self.current_stats
    }

    fn memory_path(paths: &Paths, symbol: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(format!(
            "{}/volatility_{}.json",
            paths.memory_backup_dir,
            symbol.to_lowercase()
        ))
    }

    fn sync_path(paths: &Paths, symbol: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(format!(
            "{}/volatility_{}.json",
            paths.disk_sync_dir,
            symbol.to_lowercase()
        ))
    }
}

/// 全局波动率管理器
pub struct VolatilityManager {
    detectors: HashMap<String, SymbolVolatility>,
    /// 波动率排名器
    rank: VolatilityRank,
    /// 上次汇总时间
    last_summary_time: Instant,
    /// 汇总间隔（1分钟）
    summary_interval: Duration,
}

impl VolatilityManager {
    pub fn new() -> Self {
        Self {
            detectors: HashMap::new(),
            rank: VolatilityRank::new(),
            last_summary_time: Instant::now(),
            summary_interval: Duration::from_secs(60),
        }
    }

    /// 获取或创建品种检测器（带灾备加载）
    pub fn get_or_create(&mut self, symbol: &str) -> &mut SymbolVolatility {
        self.detectors
            .entry(symbol.to_string())
            .or_insert_with(|| {
                let mut vol = SymbolVolatility::new(symbol.to_string());
                vol.try_load();
                vol
            })
    }

    /// 更新品种波动率（纯内存计算，不写文件）
    pub fn update(&mut self, symbol: &str, kline: KLineInput) -> VolatilityStats {
        let detector = self.get_or_create(symbol);
        let stats = detector.update(kline);
        // 更新排名器
        self.rank.update(symbol, stats);
        stats
    }

    /// 获取波动率排名（1m降序）
    pub fn rank_by_1m(&self) -> Vec<&VolatilityEntry> {
        self.rank.rank_by_1m()
    }

    /// 获取高波动品种列表
    pub fn high_volatility_list(&self) -> Vec<&VolatilityEntry> {
        self.rank.high_volatility_list()
    }

    /// K线闭合时保存窗口（由调用者触发）
    pub fn save_window_on_close(&mut self, symbol: &str) {
        if let Some(detector) = self.detectors.get_mut(symbol) {
            detector.save_window_to_memory();
        }
    }

    /// 检查是否需要输出每分钟汇总
    pub fn check_and_log_summary(&mut self) {
        if self.last_summary_time.elapsed() >= self.summary_interval {
            self.log_summary();
            self.last_summary_time = Instant::now();
        }
    }

    /// 输出每分钟汇总日志（使用排名器）
    fn log_summary(&self) {
        let high_vol_list = self.high_volatility_list();

        if high_vol_list.is_empty() {
            tracing::info!(
                "[HIGH_VOL] ⏳ 每分钟汇总 | 无高波动品种 | {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        } else {
            let summary: String = high_vol_list
                .iter()
                .map(|e| format!("{}: 1m={:.2}% 15m={:.2}%", e.symbol, e.vol_1m * dec!(100), e.vol_15m * dec!(100)))
                .collect::<Vec<_>>()
                .join(" | ");

            tracing::warn!(
                "[HIGH_VOL] 📊 每分钟汇总 | {}个高波动 | {} | {}",
                high_vol_list.len(),
                summary,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        }
    }
}

impl Default for VolatilityManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_manager() {
        let mut manager = VolatilityManager::new();
        let kline = KLineInput {
            open: dec!(100),
            high: dec!(101),
            low: dec!(99),
            close: dec!(100),
            timestamp: chrono::Utc::now(),
        };
        let stats = manager.update("BTCUSDT", kline);
        assert!(!stats.is_high_volatility);
    }
}
