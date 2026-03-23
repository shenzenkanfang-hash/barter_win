//! 高波动检测模块
//!
//! 每 tick 触发计算，维护 15m 滚动窗口
//! - 1m 3% / 15m 13% 高波动判断
//! - 进入高波动时触发日志
//! - 每分钟汇总所有高波动品种

use a_common::volatility::{KLineInput, VolatilityCalc, VolatilityStats};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 单个品种的高波动检测器
pub struct SymbolVolatility {
    symbol: String,
    calc: VolatilityCalc,
    /// 上次是否高波动（用于检测进入/退出）
    was_high_volatility: bool,
}

impl SymbolVolatility {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            calc: VolatilityCalc::new(),
            was_high_volatility: false,
        }
    }

    /// 每 tick 更新
    pub fn update(&mut self, kline: KLineInput) -> VolatilityStats {
        let stats = self.calc.update(kline);

        // 检测是否进入高波动（1m 或 15m）
        let is_1m_high = stats.vol_1m >= self.calc.thresholds().0;
        let is_15m_high = stats.vol_15m >= self.calc.thresholds().1;

        if is_1m_high && !self.was_high_volatility {
            tracing::warn!(
                "[HIGH_VOL] 🔴 1m进入 | {} 1m={:.2}% 15m={:.2}% | {}",
                self.symbol,
                stats.vol_1m * 100,
                stats.vol_15m * 100,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        } else if is_15m_high && !self.was_high_volatility {
            tracing::warn!(
                "[HIGH_VOL] 🟠 15m进入 | {} 1m={:.2}% 15m={:.2}% | {}",
                self.symbol,
                stats.vol_1m * 100,
                stats.vol_15m * 100,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        }

        self.was_high_volatility = is_1m_high || is_15m_high;
        stats
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn is_high_volatility(&self) -> bool {
        self.was_high_volatility
    }
}

/// 全局波动率管理器
pub struct VolatilityManager {
    detectors: HashMap<String, SymbolVolatility>,
    /// 上次汇总时间
    last_summary_time: Instant,
    /// 汇总间隔（1分钟）
    summary_interval: Duration,
}

impl VolatilityManager {
    pub fn new() -> Self {
        Self {
            detectors: HashMap::new(),
            last_summary_time: Instant::now(),
            summary_interval: Duration::from_secs(60),
        }
    }

    /// 获取或创建品种检测器
    pub fn get_or_create(&mut self, symbol: &str) -> &mut SymbolVolatility {
        self.detectors
            .entry(symbol.to_string())
            .or_insert_with(|| SymbolVolatility::new(symbol.to_string()))
    }

    /// 更新品种波动率（纯内存计算，不写文件）
    pub fn update(&mut self, symbol: &str, kline: KLineInput) -> VolatilityStats {
        let detector = self.get_or_create(symbol);
        detector.update(kline)
    }

    /// 检查是否需要输出每分钟汇总
    pub fn check_and_log_summary(&mut self) {
        if self.last_summary_time.elapsed() >= self.summary_interval {
            self.log_summary();
            self.last_summary_time = Instant::now();
        }
    }

    /// 输出每分钟汇总日志
    fn log_summary(&self) {
        let high_vol_symbols: Vec<&SymbolVolatility> = self
            .detectors
            .values()
            .filter(|v| v.is_high_volatility())
            .collect();

        if high_vol_symbols.is_empty() {
            tracing::info!(
                "[HIGH_VOL] ⏳ 每分钟汇总 | 无高波动品种 | {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            );
        } else {
            let summary: String = high_vol_symbols
                .iter()
                .map(|v| {
                    let stats = v.get_stats();
                    format!("{}: 1m={:.2}% 15m={:.2}%", v.symbol(), stats.vol_1m * 100, stats.vol_15m * 100)
                })
                .collect::<Vec<_>>()
                .join(" | ");

            tracing::warn!(
                "[HIGH_VOL] 📊 每分钟汇总 | {}个高波动 | {} | {}",
                high_vol_symbols.len(),
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
