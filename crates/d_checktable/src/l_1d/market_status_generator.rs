#![forbid(unsafe_code)]

use a_common::config::VOLATILITY_CONFIG;
use crate::types::{MarketStatus, VolatilityTier, DayMarketStatusInput, DayMarketStatusOutput, DaySignalInput};

/// 日线级市场状态生成器
///
/// ```text
/// VolatilityTier Decision Tree (使用全局阈值配置)
/// ─────────────────────────────────────────────────
///   tr_ratio >= high_vol_15m  ──> High
///   tr_ratio >= high_vol_1m   ──> Medium
///   otherwise                 ──> Low
/// ─────────────────────────────────────────────────
///
/// Market Status Decision Tree:
/// ─────────────────────────────────────────────────
///   if pine_color == "纯绿" or "纯红"  ──> TREND
///   else if volatility_tier == Low       ──> RANGE
///   else                                  ──> TREND
/// ─────────────────────────────────────────────────
///
/// 注意：日线级使用相同的全局阈值配置
/// ```
pub struct DayMarketStatusGenerator {
    #[allow(dead_code)]
    data_timeout_seconds: i64,
}

impl Default for DayMarketStatusGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DayMarketStatusGenerator {
    pub fn new() -> Self {
        Self {
            data_timeout_seconds: 180,
        }
    }

    /// 检测市场状态
    pub fn detect(&self, input: &DayMarketStatusInput) -> DayMarketStatusOutput {
        // 1. 判断波动率等级
        let volatility_tier = self.determine_volatility_level(input);

        // 2. 判断市场状态
        let status = self.determine_status(input, &volatility_tier);

        DayMarketStatusOutput {
            status,
            volatility_tier,
        }
    }

    /// 判断波动率等级（基于 tr_ratio_5d_20d 和 tr_ratio_20d_60d，使用全局阈值）
    pub fn determine_volatility_level(&self, input: &DayMarketStatusInput) -> VolatilityTier {
        let config = &*VOLATILITY_CONFIG;
        let max_ratio = input.tr_ratio_5d_20d.max(input.tr_ratio_20d_60d);
        if max_ratio >= config.high_vol_15m {
            VolatilityTier::High
        } else if max_ratio >= config.high_vol_1m {
            VolatilityTier::Medium
        } else {
            VolatilityTier::Low
        }
    }

    /// 判断波动率等级（基于 DaySignalInput）
    pub fn determine_volatility_level_from_signal(&self, input: &DaySignalInput) -> VolatilityTier {
        let config = &*VOLATILITY_CONFIG;
        let max_ratio = input.tr_ratio_5d_20d.max(input.tr_ratio_20d_60d);
        if max_ratio >= config.high_vol_15m {
            VolatilityTier::High
        } else if max_ratio >= config.high_vol_1m {
            VolatilityTier::Medium
        } else {
            VolatilityTier::Low
        }
    }

    /// 判断市场状态
    fn determine_status(&self, input: &DayMarketStatusInput, vol_tier: &VolatilityTier) -> MarketStatus {
        // 基于 Pine 颜色和位置判断市场状态
        let pine_color_valid = !input.pine_color.is_empty();

        // 如果 Pine 颜色有效且为纯色，可能是趋势市场
        if pine_color_valid {
            if input.pine_color == "纯绿" || input.pine_color == "纯红" {
                return MarketStatus::TREND;
            }
        }

        // 低波动率时为震荡
        if *vol_tier == VolatilityTier::Low {
            return MarketStatus::RANGE;
        }

        MarketStatus::TREND
    }
}
