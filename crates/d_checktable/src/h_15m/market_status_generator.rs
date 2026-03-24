#![forbid(unsafe_code)]

use a_common::config::VOLATILITY_CONFIG;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MarketStatus, VolatilityTier, MinMarketStatusInput, MinMarketStatusOutput};

/// 分钟级市场状态生成器
///
/// ```text
/// VolatilityTier Decision Tree (使用全局阈值配置)
/// ─────────────────────────────────────────────
///   tr_ratio_15min >= high_vol_15m  ──> High
///   tr_ratio_15min >= high_vol_1m   ──> Medium
///   otherwise                        ──> Low
/// ─────────────────────────────────────────────
///
/// Market Status Decision Tree
/// ───────────────────────────────────────────────────────────
///   if tr_base_60min > 15% AND pin_conditions >= 2
///       ──> PIN (with reason)
///   else if volatility == Low AND tr_ratio_15min < 1.0
///       AND |zscore| < 0.5
///       ──> RANGE
///   else
///       ──> TREND
/// ───────────────────────────────────────────────────────────
///
/// PIN Conditions (simplified, 4 conditions):
///   1. |zscore| > 2
///   2. tr_ratio_15min >= 阈值
///   3. price_position > 90% or < 10%
///   4. tr_base_60min > 20%
/// ```
pub struct MinMarketStatusGenerator {
    #[allow(dead_code)]
    data_timeout_seconds: i64,
}

impl Default for MinMarketStatusGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MinMarketStatusGenerator {
    pub fn new() -> Self {
        Self {
            data_timeout_seconds: 180,
        }
    }

    /// 检测市场状态
    pub fn detect(&self, input: &MinMarketStatusInput) -> MinMarketStatusOutput {
        // 1. 判断波动率等级
        let volatility_tier = self.determine_volatility_level(input.tr_ratio_15min);

        // 2. 判断市场状态 (优先级: INVALID > PIN > RANGE > TREND)
        let (status, reason) = self.determine_status(input, &volatility_tier);

        MinMarketStatusOutput {
            status,
            volatility_tier,
            high_volatility_reason: reason,
        }
    }

    /// 判断波动率等级 (使用全局阈值配置)
    pub fn determine_volatility_level(&self, tr_15min: Decimal) -> VolatilityTier {
        let config = &*VOLATILITY_CONFIG;
        if tr_15min >= config.high_vol_15m {
            VolatilityTier::High
        } else if tr_15min >= config.high_vol_1m {
            VolatilityTier::Medium
        } else {
            VolatilityTier::Low
        }
    }

    /// 判断市场状态
    fn determine_status(&self, input: &MinMarketStatusInput, vol_tier: &VolatilityTier) -> (MarketStatus, Option<String>) {
        // PIN 条件检测 (前置: tr_base_60min > 15%)
        if input.tr_base_60min > dec!(0.15) {
            let pin_count = self.count_pin_conditions(input);
            if pin_count >= 2 {
                let reason = format!("PIN detected with {}/4 conditions satisfied", pin_count);
                return (MarketStatus::PIN, Some(reason));
            }
        }

        // RANGE 条件
        if *vol_tier == VolatilityTier::Low && input.tr_ratio_15min < dec!(1.0) {
            let zscore_near_zero = input.zscore.abs() < dec!(0.5);
            if zscore_near_zero {
                return (MarketStatus::RANGE, None);
            }
        }

        (MarketStatus::TREND, None)
    }

    /// 统计满足的插针条件数量 (基于 MinMarketStatusInput 可用字段)
    /// 简化版: 满足条件 >= 2 即为 PIN
    fn count_pin_conditions(&self, input: &MinMarketStatusInput) -> u8 {
        let mut satisfied: u8 = 0;
        let config = &*VOLATILITY_CONFIG;

        // 1. extreme_z: |zscore| > 2
        if input.zscore.abs() > dec!(2) {
            satisfied += 1;
        }

        // 2. extreme_vol: tr_ratio_15min >= 高波动阈值
        if input.tr_ratio_15min >= config.high_vol_15m {
            satisfied += 1;
        }

        // 3. extreme_pos: price_position > 90% 或 < 10%
        if input.price_position > dec!(90) || input.price_position < dec!(10) {
            satisfied += 1;
        }

        // 4. extreme_tr: tr_base_60min > 20% (极端波动)
        if input.tr_base_60min > dec!(0.20) {
            satisfied += 1;
        }

        satisfied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_volatility_tier_high() {
        let r#gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.06),  // >= 5%
            ..Default::default()
        };
        let output = r#gen.detect(&input);
        assert_eq!(output.volatility_tier, VolatilityTier::High);
    }

    #[test]
    fn test_volatility_tier_medium() {
        let r#gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.01),  // >= 0.5%
            ..Default::default()
        };
        let output = r#gen.detect(&input);
        assert_eq!(output.volatility_tier, VolatilityTier::Medium);
    }

    #[test]
    fn test_volatility_tier_low() {
        let r#gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.001),  // < 0.5%
            ..Default::default()
        };
        let output = r#gen.detect(&input);
        assert_eq!(output.volatility_tier, VolatilityTier::Low);
    }

    #[test]
    fn test_status_trend_default() {
        let r#gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.01),
            ..Default::default()
        };
        let output = r#gen.detect(&input);
        assert_eq!(output.status, MarketStatus::TREND);
    }

    #[test]
    fn test_status_pin_with_high_tr_base() {
        let r#gen = MinMarketStatusGenerator::new();
        // tr_base_60min > 15% and zscore > 2 (1 condition)
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.06),   // >= 5%, satisfies extreme_vol
            tr_base_60min: dec!(0.16),   // > 15%, triggers PIN check
            zscore: dec!(3),              // > 2, satisfies extreme_z
            price_position: dec!(50),    // not extreme
            ..Default::default()
        };
        let output = r#gen.detect(&input);
        // 2 conditions satisfied: extreme_z, extreme_vol
        assert_eq!(output.status, MarketStatus::PIN);
    }

    #[test]
    fn test_status_range_low_volatility() {
        let r#gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.001),  // < 0.5%, Low volatility
            zscore: dec!(0.3),           // < 0.5, near zero
            ..Default::default()
        };
        let output = r#gen.detect(&input);
        assert_eq!(output.status, MarketStatus::RANGE);
    }

    #[test]
    fn test_count_pin_conditions() {
        let r#gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.06),   // >= 5%
            tr_base_60min: dec!(0.25),    // > 20%
            zscore: dec!(2.5),            // > 2
            price_position: dec!(95),     // > 90
            ..Default::default()
        };
        // Should satisfy all 4 conditions
        let output = r#gen.detect(&input);
        assert_eq!(output.status, MarketStatus::PIN);
        assert!(output.high_volatility_reason.is_some());
    }
}