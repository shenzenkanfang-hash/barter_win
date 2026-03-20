#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MarketStatus, VolatilityLevel, DayMarketStatusInput, DayMarketStatusOutput};

/// 日线级市场状态生成器
pub struct DayMarketStatusGenerator;

impl Default for DayMarketStatusGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DayMarketStatusGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 检测市场状态
    pub fn detect(&self, input: &DayMarketStatusInput) -> DayMarketStatusOutput {
        // 1. 判断波动率等级
        let volatility_level = self.determine_volatility_level(input);

        // 2. 判断市场状态
        let status = self.determine_status(input, &volatility_level);

        DayMarketStatusOutput {
            status,
            volatility_level,
        }
    }

    /// 判断波动率等级
    fn determine_volatility_level(&self, input: &DayMarketStatusInput) -> VolatilityLevel {
        // 日线: TR 极端判定
        if input.tr_ratio_5d_20d > dec!(2.0) || input.tr_ratio_20d_60d > dec!(2.0) {
            VolatilityLevel::HIGH
        } else if input.tr_ratio_5d_20d < dec!(0.5) && input.tr_ratio_20d_60d < dec!(0.5) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 判断市场状态
    fn determine_status(&self, input: &DayMarketStatusInput, vol_level: &VolatilityLevel) -> MarketStatus {
        // PIN: 日线 PineColor + TR 极端
        if vol_level == &VolatilityLevel::HIGH {
            return MarketStatus::PIN;
        }

        // RANGE: 低 TR + 无强趋势颜色 + 动能适中
        if input.tr_ratio_20d_60d < dec!(0.8) {
            let color_is_weak = input.pine_color == "浅绿" || input.pine_color == "浅红";
            let power_moderate = input.power_percentile > dec!(20) && input.power_percentile < dec!(80);
            if color_is_weak && power_moderate {
                return MarketStatus::RANGE;
            }
        }

        MarketStatus::TREND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trend_status() {
        let gen = DayMarketStatusGenerator::new();
        let input = DayMarketStatusInput {
            tr_ratio_5d_20d: dec!(1.0),
            tr_ratio_20d_60d: dec!(1.0),
            pine_color: "纯绿".to_string(),
            ma5_in_20d_ma5_pos: dec!(50),
            power_percentile: dec!(50),
        };

        let output = gen.detect(&input);
        assert_eq!(output.status, MarketStatus::TREND);
    }

    #[test]
    fn test_range_status() {
        let gen = DayMarketStatusGenerator::new();
        let input = DayMarketStatusInput {
            tr_ratio_5d_20d: dec!(0.5),
            tr_ratio_20d_60d: dec!(0.5),
            pine_color: "浅绿".to_string(),
            ma5_in_20d_ma5_pos: dec!(50),
            power_percentile: dec!(50),
        };

        let output = gen.detect(&input);
        assert_eq!(output.status, MarketStatus::RANGE);
    }
}