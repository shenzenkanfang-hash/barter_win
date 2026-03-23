#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MarketStatus, VolatilityLevel, DayMarketStatusInput, DayMarketStatusOutput, DaySignalInput};

/// 日线级市场状态生成器
pub struct DayMarketStatusGenerator {
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
        let volatility_level = self.determine_volatility_level(input);

        // 2. 判断市场状态
        let status = self.determine_status(input, &volatility_level);

        DayMarketStatusOutput {
            status,
            volatility_level,
        }
    }

    /// 判断波动率等级（基于 tr_ratio_5d_20d 和 tr_ratio_20d_60d）
    pub fn determine_volatility_level(&self, input: &DayMarketStatusInput) -> VolatilityLevel {
        // 日线级使用更大的阈值
        if input.tr_ratio_5d_20d > dec!(1.5) || input.tr_ratio_20d_60d > dec!(1.5) {
            VolatilityLevel::HIGH
        } else if input.tr_ratio_5d_20d < dec!(0.5) && input.tr_ratio_20d_60d < dec!(0.5) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 判断波动率等级（基于 DaySignalInput）
    pub fn determine_volatility_level_from_signal(&self, input: &DaySignalInput) -> VolatilityLevel {
        if input.tr_ratio_5d_20d > dec!(1.5) || input.tr_ratio_20d_60d > dec!(1.5) {
            VolatilityLevel::HIGH
        } else if input.tr_ratio_5d_20d < dec!(0.5) && input.tr_ratio_20d_60d < dec!(0.5) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 判断市场状态
    fn determine_status(&self, input: &DayMarketStatusInput, vol_level: &VolatilityLevel) -> MarketStatus {
        // 基于 Pine 颜色和位置判断市场状态
        let pine_color_valid = !input.pine_color.is_empty();

        // 如果 Pine 颜色有效且为纯色，可能是趋势市场
        if pine_color_valid {
            if input.pine_color == "纯绿" || input.pine_color == "纯红" {
                return MarketStatus::TREND;
            }
        }

        // 低波动率时为震荡
        if vol_level == &VolatilityLevel::LOW {
            return MarketStatus::RANGE;
        }

        MarketStatus::TREND
    }
}
