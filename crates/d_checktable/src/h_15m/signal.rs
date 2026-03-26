//! h_15m/signal.rs
//!
//! 分钟级信号生成器 - 7条件Pin模式 + 双通道支持
//!
//! ```text
//! 双通道架构:
//!   VolatilityTier::High  → 高速通道 (Fast)
//!     - 高频交易
//!     - 0.05基础开仓
//!     - 0.15最大持仓
//!     - 7条件Pin模式
//!
//!   VolatilityTier::Low/Medium → 低速通道 (Slow)
//!     - 低频交易
//!     - 保守策略
//!     - 参考日线方向
//!
//! Pin Condition Scoring (7 conditions, satisfied >= 4 triggers entry/exit)
//! ─────────────────────────────────────────────────────────────────────────
//!  #  Condition                      Threshold                       Count
//! ─────────────────────────────────────────────────────────────────────────
//!  1  extreme_zscore                |zscore_14_1m| > 2 OR           +1
//!                                    |zscore_1h_1m| > 2
//!  2  extreme_vol (tr_ratio)         tr_ratio_60min_5h > 1 OR       +1
//!                                    tr_ratio_10min_1h > 1
//!  3  extreme_pos                    pos_norm_60 > 80 OR < 20        +1
//!  4  extreme_speed (acc_percentile) acc_percentile_1h > 90          +1
//!  5  extreme_bg_color              pine_bg_color == "纯绿" OR       +1
//!                                    pine_bg_color == "纯红"
//!  6  extreme_bar_color             pine_bar_color == "纯绿" OR      +1
//!                                    pine_bar_color == "纯红"
//!  7  extreme_price_deviation        |price_deviation_horizontal_    +1
//!                                    position| == 100
//! ─────────────────────────────────────────────────────────────────────────
//!                                                     pin_satisfied: 0-7
//!
//! Signal Logic (高速通道):
//! ────────────
//! long_entry    : tr_base_60min > 15% AND price_deviation < 0 AND pin >= 4
//! short_entry   : tr_base_60min > 15% AND price_deviation > 0 AND pin >= 4
//! long_exit     : pin >= 4 AND pos_norm_60 > 80
//! short_exit    : pin >= 4 AND pos_norm_60 < 20
//! long_hedge    : tr_base_60min < 15% AND price_deviation < 0 AND 6 cond >= 4
//! short_hedge   : tr_base_60min < 15% AND price_deviation > 0 AND 6 cond >= 4
//! exit_high_vol : tr_base_60min < 15% AND 3 cond >= 2
//! ```

#![forbid(unsafe_code)]

use rust_decimal_macros::dec;
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityTier};
use x_data::position::PositionSide;

/// 分钟级信号生成器
pub struct MinSignalGenerator;

impl MinSignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 双通道主入口（自动选择高速/低速）
    ///
    /// - VolatilityTier::High → 高速通道
    /// - VolatilityTier::Low/Medium → 低速通道（参考日线方向）
    pub fn generate(
        &self,
        input: &MinSignalInput,
        vol_tier: &VolatilityTier,
        day_direction: Option<PositionSide>,
    ) -> MinSignalOutput {
        match vol_tier {
            VolatilityTier::High => self.generate_fast_signal(input),
            VolatilityTier::Low | VolatilityTier::Medium => {
                self.generate_slow_signal(input, day_direction)
            }
        }
    }

    /// 高速通道信号生成（高频交易）
    ///
    /// 使用完整的7条件Pin模式
    pub fn generate_fast_signal(&self, input: &MinSignalInput) -> MinSignalOutput {
        let pin_satisfied = self.count_pin_conditions(input);

        MinSignalOutput {
            long_entry: self.check_long_entry(input, pin_satisfied),
            short_entry: self.check_short_entry(input, pin_satisfied),
            long_exit: self.check_long_exit(input, pin_satisfied),
            short_exit: self.check_short_exit(input, pin_satisfied),
            long_hedge: self.check_long_hedge(input),
            short_hedge: self.check_short_hedge(input),
            exit_high_volatility: self.check_exit_high_volatility(input),
        }
    }

    /// 低速通道信号生成（保守策略，参考日线方向）
    ///
    /// 条件更严格，只在日线方向明确时发出信号
    pub fn generate_slow_signal(
        &self,
        input: &MinSignalInput,
        day_direction: Option<PositionSide>,
    ) -> MinSignalOutput {
        let pin_satisfied = self.count_pin_conditions(input);

        // 低速通道：只在日线方向明确时允许开仓
        let long_entry = day_direction.map(|d| d == PositionSide::Long).unwrap_or(false)
            && self.check_long_entry_slow(input, pin_satisfied);

        let short_entry = day_direction.map(|d| d == PositionSide::Short).unwrap_or(false)
            && self.check_short_entry_slow(input, pin_satisfied);

        MinSignalOutput {
            long_entry,
            short_entry,
            long_exit: self.check_long_exit(input, pin_satisfied),
            short_exit: self.check_short_exit(input, pin_satisfied),
            long_hedge: self.check_long_hedge(input),
            short_hedge: self.check_short_hedge(input),
            exit_high_volatility: self.check_exit_high_volatility(input),
        }
    }

    /// 统计满足的插针条件数量（7个条件）
    fn count_pin_conditions(&self, input: &MinSignalInput) -> u8 {
        let mut count: u8 = 0;

        // 1. extreme_z: |zscore| > 2
        if input.zscore_14_1m.abs() > dec!(2) || input.zscore_1h_1m.abs() > dec!(2) {
            count += 1;
        }
        // 2. extreme_vol: tr_ratio > 1
        if input.tr_ratio_60min_5h > dec!(1) || input.tr_ratio_10min_1h > dec!(1) {
            count += 1;
        }
        // 3. extreme_pos: > 80 或 < 20
        if input.pos_norm_60 > dec!(80) || input.pos_norm_60 < dec!(20) {
            count += 1;
        }
        // 4. extreme_speed: acc_percentile > 90
        if input.acc_percentile_1h > dec!(90) {
            count += 1;
        }
        // 5. extreme_bg_color: 纯绿或纯红
        if input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红" {
            count += 1;
        }
        // 6. extreme_bar_color: 纯绿或纯红
        if input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红" {
            count += 1;
        }
        // 7. extreme_price_deviation: |pos| == 100
        if input.price_deviation_horizontal_position.abs() == dec!(100) {
            count += 1;
        }

        count
    }

    /// 检查做多入场条件（前置: tr_base_60min > 15%）
    fn check_long_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        if input.tr_base_60min <= dec!(0.15) {
            return false;
        }
        if input.price_deviation >= dec!(0) {
            return false;
        }
        pin_satisfied >= 4
    }

    /// 检查做空入场条件（前置: tr_base_60min > 15%）
    fn check_short_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        if input.tr_base_60min <= dec!(0.15) {
            return false;
        }
        if input.price_deviation <= dec!(0) {
            return false;
        }
        pin_satisfied >= 4
    }

    /// 低速通道做多入场（更严格）
    fn check_long_entry_slow(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 低速通道：要求更高的插针强度
        if input.tr_base_60min <= dec!(0.10) {
            return false;
        }
        if input.price_deviation >= dec!(0) {
            return false;
        }
        pin_satisfied >= 5
    }

    /// 低速通道做空入场（更严格）
    fn check_short_entry_slow(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        if input.tr_base_60min <= dec!(0.10) {
            return false;
        }
        if input.price_deviation <= dec!(0) {
            return false;
        }
        pin_satisfied >= 5
    }

    /// 检查做多退出条件
    fn check_long_exit(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        let pos_extreme = input.pos_norm_60 > dec!(80);
        pin_satisfied >= 4 && pos_extreme
    }

    /// 检查做空退出条件
    fn check_short_exit(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        let pos_extreme = input.pos_norm_60 < dec!(20);
        pin_satisfied >= 4 && pos_extreme
    }

    /// 检查多头对冲条件（前置: tr_base_60min < 15%）
    fn check_long_hedge(&self, input: &MinSignalInput) -> bool {
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }
        if input.price_deviation >= dec!(0) {
            return false;
        }

        let mut conditions: u8 = 0;

        if input.tr_ratio_60min_5h > dec!(2) || input.tr_ratio_10min_1h > dec!(1) {
            conditions += 1;
        }
        if input.pos_norm_60 < dec!(90) {
            conditions += 1;
        }
        if input.acc_percentile_1h < dec!(10) && input.velocity_percentile_1h < dec!(10) {
            conditions += 1;
        }
        if input.pine_bg_color != "纯绿" {
            conditions += 1;
        }
        if input.pine_bar_color != "纯绿" {
            conditions += 1;
        }
        if dec!(10) < input.price_deviation_horizontal_position.abs()
            && input.price_deviation_horizontal_position.abs() <= dec!(90)
        {
            conditions += 1;
        }

        conditions >= 4
    }

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &MinSignalInput) -> bool {
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }
        if input.price_deviation <= dec!(0) {
            return false;
        }

        let mut conditions: u8 = 0;

        if input.tr_ratio_60min_5h > dec!(2) || input.tr_ratio_10min_1h > dec!(1) {
            conditions += 1;
        }
        if input.pos_norm_60 > dec!(10) {
            conditions += 1;
        }
        if input.acc_percentile_1h > dec!(90) && input.velocity_percentile_1h > dec!(90) {
            conditions += 1;
        }
        if input.pine_bg_color != "纯红" {
            conditions += 1;
        }
        if input.pine_bar_color != "纯红" {
            conditions += 1;
        }
        if dec!(10) < input.price_deviation_horizontal_position.abs()
            && input.price_deviation_horizontal_position.abs() <= dec!(90)
        {
            conditions += 1;
        }

        conditions >= 4
    }

    /// 检查退出高波动条件
    fn check_exit_high_volatility(&self, input: &MinSignalInput) -> bool {
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }

        let cond1 = input.tr_ratio_60min_5h < dec!(1) && input.tr_ratio_10min_1h < dec!(1);
        let cond2 = input.pos_norm_60 > dec!(20) && input.pos_norm_60 < dec!(80);
        let cond3 = dec!(10) < input.price_deviation_horizontal_position.abs()
            && input.price_deviation_horizontal_position.abs() <= dec!(90);

        [cond1, cond2, cond3].iter().filter(|&&x| x).count() >= 2
    }
}

impl Default for MinSignalGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_output() {
        let output = MinSignalOutput::default();
        assert!(!output.long_entry);
        assert!(!output.short_entry);
        assert!(!output.long_exit);
        assert!(!output.short_exit);
        assert!(!output.long_hedge);
        assert!(!output.short_hedge);
        assert!(!output.exit_high_volatility);
    }

    #[test]
    fn test_new_generator() {
        let r#gen = MinSignalGenerator::new();
        assert!(!r#gen.generate_fast_signal(&MinSignalInput::default()).long_entry);
    }

    #[test]
    fn test_high_channel_uses_full_conditions() {
        let r#gen = MinSignalGenerator::new();
        let mut input = MinSignalInput::default();
        input.tr_base_60min = dec!(0.20);
        input.price_deviation = dec!(-0.5);
        input.pos_norm_60 = dec!(90);
        input.acc_percentile_1h = dec!(95);

        let output = r#gen.generate_fast_signal(&input);
        assert!(output.long_entry || output.short_entry);
    }
}
