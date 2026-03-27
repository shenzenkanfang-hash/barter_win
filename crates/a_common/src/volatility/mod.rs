//! 波动率计算器
//!
//! 输入每根 K 线，输出波动率统计
//! - 1m O-C 变化率
//! - 15m Close-Close 变化率
//! - 高波动判断

use crate::config::VOLATILITY_CONFIG;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// K 线数据（最小集合，用于波动率计算）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KLineInput {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 波动率统计
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolatilityStats {
    /// 是否高波动
    pub is_high_volatility: bool,
    /// 1m O-C 变化率
    pub vol_1m: Decimal,
    /// 15m Close-Close 变化率
    pub vol_15m: Decimal,
}

impl Default for VolatilityStats {
    fn default() -> Self {
        Self {
            is_high_volatility: false,
            vol_1m: dec!(0),
            vol_15m: dec!(0),
        }
    }
}

/// 波动率计算器
#[derive(Debug, Clone)]
pub struct VolatilityCalc {
    /// 15m K线窗口 (保留最近2根)
    kline_15m_window: Vec<KLineInput>,
    /// 1m K线计数（当前累积到第几根）
    kline_1m_count: u32,
    /// 阈值: 1m (从全局配置读取)
    threshold_1m: Decimal,
    /// 阈值: 15m (从全局配置读取)
    threshold_15m: Decimal,
    /// 上次更新时间
    last_update: DateTime<Utc>,
}

impl VolatilityCalc {
    pub fn new() -> Self {
        let config = &*VOLATILITY_CONFIG;
        Self {
            kline_15m_window: Vec::with_capacity(2),
            kline_1m_count: 0,
            threshold_1m: config.high_vol_1m,
            threshold_15m: config.high_vol_15m,
            last_update: Utc::now(),
        }
    }

    /// 从状态恢复
    pub fn restore(state: VolatilityState) -> Self {
        let config = &*VOLATILITY_CONFIG;
        Self {
            kline_15m_window: state.kline_15m_window,
            kline_1m_count: state.kline_1m_count,
            threshold_1m: config.high_vol_1m,
            threshold_15m: config.high_vol_15m,
            last_update: Utc::now(),
        }
    }

    /// 检查数据是否有效（延迟超过2分钟则无效）
    pub fn is_valid(&self) -> bool {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.last_update);
        elapsed < Duration::minutes(2)
    }

    /// 输入 1m K 线，返回波动率统计
    pub fn update(&mut self, kline: KLineInput) -> VolatilityStats {
        self.last_update = Utc::now();

        // 1. 计算 1m O-C 变化率
        let vol_1m = self.calc_1m_volatility(&kline);

        // 2. 累积 15 根 1m K 线后更新 15m 窗口
        self.kline_1m_count += 1;
        if self.kline_1m_count >= 15 {
            self.update_15m_window(kline);
            self.kline_1m_count = 0;
        }

        // 3. 计算 15m Close-Close 变化率
        let vol_15m = self.calc_15m_volatility();

        // 4. 判断是否高波动
        let is_high = vol_1m >= self.threshold_1m || vol_15m >= self.threshold_15m;

        VolatilityStats {
            is_high_volatility: is_high,
            vol_1m,
            vol_15m,
        }
    }

    /// 1m O-C 变化率
    fn calc_1m_volatility(&self, kline: &KLineInput) -> Decimal {
        if kline.open > dec!(0) {
            (kline.close - kline.open).abs() / kline.open
        } else {
            dec!(0)
        }
    }

    /// 15m Close-Close 变化率
    fn calc_15m_volatility(&self) -> Decimal {
        if self.kline_15m_window.len() < 2 {
            return dec!(0);
        }
        let prev = &self.kline_15m_window[0];
        let curr = &self.kline_15m_window[1];
        if prev.close > dec!(0) {
            (curr.close - prev.close).abs() / prev.close
        } else {
            dec!(0)
        }
    }

    /// 更新 15m 窗口
    fn update_15m_window(&mut self, kline: KLineInput) {
        if self.kline_15m_window.len() >= 2 {
            self.kline_15m_window.remove(0);
        }
        self.kline_15m_window.push(kline);
    }

    /// 获取当前状态（用于灾备）
    pub fn get_state(&self) -> VolatilityState {
        VolatilityState {
            kline_15m_window: self.kline_15m_window.clone(),
            kline_1m_count: self.kline_1m_count,
        }
    }

    pub fn thresholds(&self) -> (Decimal, Decimal) {
        (self.threshold_1m, self.threshold_15m)
    }
}

impl Default for VolatilityCalc {
    fn default() -> Self {
        Self::new()
    }
}

/// 波动率状态（用于灾备序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityState {
    pub kline_15m_window: Vec<KLineInput>,
    pub kline_1m_count: u32,
}

/// 波动率排名条目
#[derive(Debug, Clone)]
pub struct VolatilityEntry {
    pub symbol: String,
    pub vol_1m: Decimal,
    pub vol_15m: Decimal,
    pub is_high_volatility: bool,
}

impl VolatilityEntry {
    pub fn new(symbol: String, stats: VolatilityStats) -> Self {
        Self {
            symbol,
            vol_1m: stats.vol_1m,
            vol_15m: stats.vol_15m,
            is_high_volatility: stats.is_high_volatility,
        }
    }
}

/// 波动率排名器（用于全市场高波动品种排名）
pub struct VolatilityRank {
    entries: Vec<VolatilityEntry>,
}

impl VolatilityRank {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 更新单个品种波动率
    pub fn update(&mut self, symbol: &str, stats: VolatilityStats) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.symbol == symbol) {
            entry.vol_1m = stats.vol_1m;
            entry.vol_15m = stats.vol_15m;
            entry.is_high_volatility = stats.is_high_volatility;
        } else {
            self.entries.push(VolatilityEntry::new(symbol.to_string(), stats));
        }
    }

    /// 查询单个品种波动率
    pub fn get(&self, symbol: &str) -> Option<&VolatilityEntry> {
        self.entries.iter().find(|e| e.symbol == symbol)
    }

    /// 按1m波动率排名（降序）
    pub fn rank_by_1m(&self) -> Vec<&VolatilityEntry> {
        let mut sorted = self.entries.iter().collect::<Vec<_>>();
        sorted.sort_by(|a, b| b.vol_1m.cmp(&a.vol_1m));
        sorted
    }

    /// 按15m波动率排名（降序）
    pub fn rank_by_15m(&self) -> Vec<&VolatilityEntry> {
        let mut sorted = self.entries.iter().collect::<Vec<_>>();
        sorted.sort_by(|a, b| b.vol_15m.cmp(&a.vol_15m));
        sorted
    }

    /// 获取1m波动率超过阈值的品种（按1m降序）
    pub fn above_1m_threshold(&self, threshold: Decimal) -> Vec<&VolatilityEntry> {
        self.rank_by_1m()
            .into_iter()
            .filter(|e| e.vol_1m >= threshold)
            .collect()
    }

    /// 获取15m波动率超过阈值的品种（按15m降序）
    pub fn above_15m_threshold(&self, threshold: Decimal) -> Vec<&VolatilityEntry> {
        self.rank_by_15m()
            .into_iter()
            .filter(|e| e.vol_15m >= threshold)
            .collect()
    }

    /// 获取1m高波动品种（使用全局阈值）
    pub fn high_vol_1m(&self) -> Vec<&VolatilityEntry> {
        let config = &*VOLATILITY_CONFIG;
        self.above_1m_threshold(config.high_vol_1m)
    }

    /// 获取15m高波动品种（使用全局阈值）
    pub fn high_vol_15m(&self) -> Vec<&VolatilityEntry> {
        let config = &*VOLATILITY_CONFIG;
        self.above_15m_threshold(config.high_vol_15m)
    }
}

impl Default for VolatilityRank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_calc_default() {
        let calc = VolatilityCalc::new();
        let (th1m, th15m) = calc.thresholds();
        assert_eq!(th1m, dec!(0.03));
        assert_eq!(th15m, dec!(0.13));
    }

    #[test]
    fn test_volatility_high_1m() {
        let mut calc = VolatilityCalc::new();

        let k1 = KLineInput {
            open: dec!(100),
            high: dec!(101),
            low: dec!(99),
            close: dec!(100),
            timestamp: Utc::now(),
        };
        calc.update(k1);

        let k2 = KLineInput {
            open: dec!(100),
            high: dec!(104),
            low: dec!(99),
            close: dec!(104),
            timestamp: Utc::now(),
        };
        let stats = calc.update(k2);

        assert!(stats.vol_1m >= dec!(0.005));
    }

    #[test]
    fn test_volatility_stats_default() {
        let stats = VolatilityStats::default();
        assert!(!stats.is_high_volatility);
        assert_eq!(stats.vol_1m, dec!(0));
        assert_eq!(stats.vol_15m, dec!(0));
    }
}
