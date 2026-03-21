#![forbid(unsafe_code)]

//! TR 波动率排名模块
//!
//! 维护所有品种的 TR 比率数组，支持排序获取 Top N 波动率品种。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

/// TR 波动率排名条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityEntry {
    /// 交易对
    pub symbol: String,
    /// TR 比率
    pub tr_ratio: Decimal,
}

/// TR 波动率排名管理器
pub struct VolatilityRank {
    /// 品种 TR 比率数组
    entries: RwLock<Vec<VolatilityEntry>>,
}

impl VolatilityRank {
    /// 创建新的波动率排名管理器
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }

    /// 更新单个品种的 TR 比率
    pub fn update_tr_ratio(&self, symbol: &str, tr_ratio: Decimal) {
        if let Ok(mut entries) = self.entries.write() {
            if let Some(entry) = entries.iter_mut().find(|e| e.symbol == symbol) {
                entry.tr_ratio = tr_ratio;
            } else {
                entries.push(VolatilityEntry {
                    symbol: symbol.to_string(),
                    tr_ratio,
                });
            }
        }
    }

    /// 获取 TR 比率排名（降序）
    pub fn get_ranking(&self) -> Vec<VolatilityEntry> {
        if let Ok(entries) = self.entries.read() {
            let mut sorted = entries.clone();
            sorted.sort_by(|a, b| b.tr_ratio.partial_cmp(&a.tr_ratio).unwrap_or(std::cmp::Ordering::Equal));
            sorted
        } else {
            Vec::new()
        }
    }

    /// 获取 Top N 波动率最高的品种
    pub fn get_top(&self, n: usize) -> Vec<VolatilityEntry> {
        let ranking = self.get_ranking();
        ranking.into_iter().take(n).collect()
    }

    /// 获取指定品种的 TR 比率
    pub fn get_tr_ratio(&self, symbol: &str) -> Option<Decimal> {
        if let Ok(entries) = self.entries.read() {
            entries.iter().find(|e| e.symbol == symbol).map(|e| e.tr_ratio)
        } else {
            None
        }
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
    fn test_update_and_get() {
        let rank = VolatilityRank::new();
        rank.update_tr_ratio("BTCUSDT", Decimal::new(100, 2));
        rank.update_tr_ratio("ETHUSDT", Decimal::new(50, 2));

        let tr = rank.get_tr_ratio("BTCUSDT");
        assert_eq!(tr, Some(Decimal::new(100, 2)));
    }

    #[test]
    fn test_ranking_order() {
        let rank = VolatilityRank::new();
        rank.update_tr_ratio("BTCUSDT", Decimal::new(100, 2));
        rank.update_tr_ratio("ETHUSDT", Decimal::new(300, 2));
        rank.update_tr_ratio("BNBUSDT", Decimal::new(200, 2));

        let top = rank.get_top(2);
        assert_eq!(top[0].symbol, "ETHUSDT");
        assert_eq!(top[1].symbol, "BNBUSDT");
    }
}
