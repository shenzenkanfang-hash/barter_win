//! 高波动检测模块
//!
//! 设计原则：
//! - 被动驱动：只有收到 1m K线闭合时才计算
//! - 每个品种独立文件：rolling_15m/{symbol}.json（15min close列表 + timestamp）
//! - 汇总文件：volatility/summary.json（所有品种的波动率排名）
//! - 加锁计算：所有品种一次性计算，不逐个处理

use a_common::config::Paths;
use a_common::volatility::{KLineInput, VolatilityCalc, VolatilityStats, VolatilityRank, VolatilityEntry};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 单个品种的15min rolling window文件格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rolling15mWindow {
    pub close: Vec<Decimal>,
    pub timestamp: i64,
}

/// 单个品种的高波动检测器
pub struct SymbolVolatility {
    symbol: String,
    calc: VolatilityCalc,
    was_high_volatility: bool,
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

    pub fn update(&mut self, kline: KLineInput) -> VolatilityStats {
        let stats = self.calc.update(kline);
        self.current_stats = stats;
        self.was_high_volatility = stats.is_high_volatility;
        stats
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

    pub fn get_state(&self) -> (String, Vec<Decimal>, u32) {
        let state = self.calc.get_state();
        let closes: Vec<Decimal> = state.kline_15m_window.iter().map(|k| k.close).collect();
        (self.symbol.clone(), closes, state.kline_1m_count)
    }

    pub fn calc_from_closes(&mut self, closes: &[Decimal], count: u32) {
        // 从 close 列表重建状态
        let now = chrono::Utc::now();
        self.calc = VolatilityCalc::restore(a_common::volatility::VolatilityState {
            kline_15m_window: closes
                .iter()
                .map(|&c| KLineInput {
                    open: c,
                    high: c,
                    low: c,
                    close: c,
                    timestamp: now,
                })
                .collect(),
            kline_1m_count: count,
        });
    }
}

/// 汇总文件格式：所有品种的波动率数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VolatilitySummary {
    /// 更新时间的秒时间戳
    pub updated_at: i64,
    /// 各品种波动率条目
    pub entries: Vec<VolatilitySummaryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilitySummaryEntry {
    pub symbol: String,
    /// 1m 波动率 (O-C 变化率)
    pub vol_1m: Decimal,
    /// 15m 波动率 (close-close 变化率)
    pub vol_15m: Decimal,
    /// 是否高波动
    pub is_high: bool,
    /// 15m 窗口 close 列表（最多2个）
    pub closes: Vec<Decimal>,
}

/// 全局波动率管理器（线程安全）
pub struct VolatilityManager {
    /// 所有品种检测器，Arc+RwLock 实现线程安全共享
    detectors: RwLock<HashMap<String, Arc<RwLock<SymbolVolatility>>>>,
    rank: RwLock<VolatilityRank>,
    last_summary_time: RwLock<Instant>,
    summary_interval: Duration,
    summary_path: std::path::PathBuf,
    /// 15min rolling window 文件目录
    rolling_15m_dir: std::path::PathBuf,
}

impl VolatilityManager {
    pub fn new() -> Self {
        let paths = Paths::new();
        let summary_path = std::path::PathBuf::from(format!("{}/volatility/summary.json", paths.memory_backup_dir));
        let rolling_15m_dir = std::path::PathBuf::from(format!("{}/rolling_15m", paths.memory_backup_dir));
        Self {
            detectors: RwLock::new(HashMap::new()),
            rank: RwLock::new(VolatilityRank::new()),
            last_summary_time: RwLock::new(Instant::now()),
            summary_interval: Duration::from_secs(60),
            summary_path,
            rolling_15m_dir,
        }
    }

    /// 保存单个品种的15min rolling window到文件
    pub fn save_rolling_15m(&self, symbol: &str) {
        let detectors = self.detectors.read();
        if let Some(vol) = detectors.get(symbol) {
            let state = vol.read().get_state();
            let (_symbol, closes, _count) = state;
            if closes.is_empty() {
                return;
            }
            let window = Rolling15mWindow {
                close: closes,
                timestamp: chrono::Utc::now().timestamp(),
            };
            let path = self.rolling_15m_dir.join(format!("{}.json", symbol.to_lowercase()));
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string(&window) {
                let _ = std::fs::write(&path, json);
            }
        }
    }

    /// 从文件加载单个品种的15min rolling window
    pub fn load_rolling_15m(&self, symbol: &str) -> Option<Rolling15mWindow> {
        let path = self.rolling_15m_dir.join(format!("{}.json", symbol.to_lowercase()));
        if !path.exists() {
            return None;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(window) = serde_json::from_str::<Rolling15mWindow>(&content) {
                return Some(window);
            }
        }
        None
    }

    /// 获取或创建品种检测器
    pub fn get_or_create(&self, symbol: &str) -> Arc<RwLock<SymbolVolatility>> {
        let detectors = self.detectors.read();
        if let Some(vol) = detectors.get(symbol) {
            return vol.clone();
        }
        drop(detectors);

        let mut detectors = self.detectors.write();
        if let Some(vol) = detectors.get(symbol) {
            return vol.clone();
        }

        let vol = Arc::new(RwLock::new(SymbolVolatility::new(symbol.to_string())));
        detectors.insert(symbol.to_string(), vol.clone());
        vol
    }

    /// 更新品种波动率（纯内存计算）
    pub fn update(&self, symbol: &str, kline: KLineInput) -> VolatilityStats {
        let vol = self.get_or_create(symbol);
        let mut guard = vol.write();
        let stats = guard.update(kline);
        drop(guard);

        // 更新排名器
        let mut rank = self.rank.write();
        rank.update(symbol, stats);

        stats
    }

    /// 批量保存所有品种到单一文件（被动调用，非每tick）
    pub fn save_summary(&self) {
        let detectors = self.detectors.read();
        let rank = self.rank.read();

        // 使用 rank_by_1m() 获取所有条目
        let entries: Vec<VolatilitySummaryEntry> = rank
            .rank_by_1m()
            .into_iter()
            .map(|e| {
                let closes = detectors
                    .get(&e.symbol)
                    .map(|v| v.read().get_state().1)
                    .unwrap_or_default();
                VolatilitySummaryEntry {
                    symbol: e.symbol.clone(),
                    vol_1m: e.vol_1m,
                    vol_15m: e.vol_15m,
                    is_high: e.is_high_volatility,
                    closes,
                }
            })
            .filter(|e| e.vol_1m > dec!(0) || e.vol_15m > dec!(0))
            .collect();

        let summary = VolatilitySummary {
            updated_at: chrono::Utc::now().timestamp(),
            entries,
        };

        drop(detectors);
        drop(rank);

        // 写入文件
        if let Some(parent) = self.summary_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&summary) {
            let _ = std::fs::write(&self.summary_path, json);
            tracing::debug!("[VOL] Saved summary: {} entries", summary.entries.len());
        }
    }

    /// 从文件加载汇总（启动时调用）
    pub fn load_summary(&self) {
        if !self.summary_path.exists() {
            tracing::info!("[VOL] No summary file, starting fresh");
            return;
        }

        if let Ok(content) = std::fs::read_to_string(&self.summary_path) {
            if let Ok(summary) = serde_json::from_str::<VolatilitySummary>(&content) {
                let now = chrono::Utc::now().timestamp();
                let age = now - summary.updated_at;

                // 超过2分钟认为数据过期
                if age < 120 {
                    let mut detectors = self.detectors.write();
                    let mut rank = self.rank.write();

                    for entry in &summary.entries {
                        let mut vol = SymbolVolatility::new(entry.symbol.clone());
                        vol.calc_from_closes(&entry.closes, 0);
                        vol.current_stats = VolatilityStats {
                            is_high_volatility: entry.is_high,
                            vol_1m: entry.vol_1m,
                            vol_15m: entry.vol_15m,
                        };
                        vol.was_high_volatility = entry.is_high;

                        let vol_arc = Arc::new(RwLock::new(vol));
                        rank.update(&entry.symbol, vol_arc.read().get_stats());
                        detectors.insert(entry.symbol.clone(), vol_arc);
                    }
                    tracing::info!("[VOL] Loaded {} entries from summary (age: {}s)", summary.entries.len(), age);
                } else {
                    tracing::info!("[VOL] Summary too old ({}s), starting fresh", age);
                }
            }
        }
    }

    /// 检查并输出每分钟汇总
    pub fn check_and_log_summary(&self) {
        let mut last_time = self.last_summary_time.write();
        if last_time.elapsed() >= self.summary_interval {
            self.log_summary();
            self.save_summary();
            *last_time = Instant::now();
        }
    }

    fn log_summary(&self) {
        let rank = self.rank.read();
        let high_1m = rank.high_vol_1m();
        let high_15m = rank.high_vol_15m();

        if high_1m.is_empty() && high_15m.is_empty() {
            tracing::info!(
                "[HIGH_VOL] ⏳ 每分钟汇总 | 无高波动 | {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        } else {
            if !high_1m.is_empty() {
                let summary: String = high_1m
                    .iter()
                    .take(10) // 最多显示10个
                    .map(|e| format!("{}:{:.2}%", e.symbol, e.vol_1m * dec!(100)))
                    .collect::<Vec<_>>()
                    .join(" ");
                tracing::warn!(
                    "[HIGH_VOL] 🔴 1m高波动({}个) | {} | {}",
                    high_1m.len(),
                    summary,
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                );
            }

            if !high_15m.is_empty() {
                let summary: String = high_15m
                    .iter()
                    .take(10)
                    .map(|e| format!("{}:{:.2}%", e.symbol, e.vol_15m * dec!(100)))
                    .collect::<Vec<_>>()
                    .join(" ");
                tracing::warn!(
                    "[HIGH_VOL] 🟠 15m高波动({}个) | {} | {}",
                    high_15m.len(),
                    summary,
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                );
            }
        }
    }

    pub fn rank_by_1m(&self) -> Vec<VolatilityEntry> {
        self.rank.read().rank_by_1m().into_iter().cloned().collect()
    }

    pub fn rank_by_15m(&self) -> Vec<VolatilityEntry> {
        self.rank.read().rank_by_15m().into_iter().cloned().collect()
    }

    pub fn high_vol_1m(&self) -> Vec<VolatilityEntry> {
        self.rank.read().high_vol_1m().into_iter().cloned().collect()
    }

    pub fn high_vol_15m(&self) -> Vec<VolatilityEntry> {
        self.rank.read().high_vol_15m().into_iter().cloned().collect()
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
        let manager = VolatilityManager::new();
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
