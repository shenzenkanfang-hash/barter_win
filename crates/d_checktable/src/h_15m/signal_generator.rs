#![forbid(unsafe_code)]

use rust_decimal_macros::dec;
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityTier};

/// 分钟级信号生成器（纯指标判断）
///
/// ```text
/// Pin Condition Scoring (7 conditions, satisfied >= 4 triggers entry/exit)
/// ─────────────────────────────────────────────────────────────────────────
///  #  Condition                      Threshold                       Count
/// ─────────────────────────────────────────────────────────────────────────
///  1  extreme_zscore                |zscore_14_1m| > 2 OR           +1
///                                    |zscore_1h_1m| > 2
///  2  extreme_vol (tr_ratio)         tr_ratio_60min_5h > 1 OR       +1
///                                    tr_ratio_10min_1h > 1
///  3  extreme_pos                    pos_norm_60 > 80 OR < 20        +1
///  4  extreme_speed (acc_percentile) acc_percentile_1h > 90          +1
///  5  extreme_bg_color              pine_bg_color == "纯绿" OR       +1
///                                    pine_bg_color == "纯红"
///  6  extreme_bar_color             pine_bar_color == "纯绿" OR      +1
///                                    pine_bar_color == "纯红"
///  7  extreme_price_deviation        |price_deviation_horizontal_    +1
///                                    position| == 100
/// ─────────────────────────────────────────────────────────────────────────
///                                                     pin_satisfied: 0-7
///
/// Signal Logic:
/// ────────────
/// long_entry    : tr_base_60min > 15% AND price_deviation < 0 AND pin >= 4
/// short_entry   : tr_base_60min > 15% AND price_deviation > 0 AND pin >= 4
/// long_exit    : pin >= 4 AND pos_norm_60 > 80
/// short_exit   : pin >= 4 AND pos_norm_60 < 20
/// long_hedge   : tr_base_60min < 15% AND price_deviation < 0 AND 6 cond >= 4
/// short_hedge  : tr_base_60min < 15% AND price_deviation > 0 AND 6 cond >= 4
/// exit_high_vol: tr_base_60min < 15% AND 3 cond >= 2
/// ```
pub struct MinSignalGenerator;

impl MinSignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &MinSignalInput, _vol_tier: &VolatilityTier) -> MinSignalOutput {
        // 检测插针条件
        let pin_satisfied = self.count_pin_conditions(input);

        // 入场信号
        let long_entry = self.check_long_entry(input, pin_satisfied);
        let short_entry = self.check_short_entry(input, pin_satisfied);

        // 退出信号
        let long_exit = self.check_long_exit(input, pin_satisfied);
        let short_exit = self.check_short_exit(input, pin_satisfied);

        // 对冲信号
        let long_hedge = self.check_long_hedge(input);
        let short_hedge = self.check_short_hedge(input);

        // 退出高波动
        let exit_high_volatility = self.check_exit_high_volatility(input);

        MinSignalOutput {
            long_entry,
            short_entry,
            long_exit,
            short_exit,
            long_hedge,
            short_hedge,
            exit_high_volatility,
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
    pub fn check_long_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 前置: tr_base_60min > 15%
        if input.tr_base_60min <= dec!(0.15) {
            return false;
        }
        // 价格偏离方向: 向下
        if input.price_deviation >= dec!(0) {
            return false;
        }
        // 7个条件满足 >= 4
        pin_satisfied >= 4
    }

    /// 检查做空入场条件（前置: tr_base_60min > 15%）
    pub fn check_short_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        if input.tr_base_60min <= dec!(0.15) {
            return false;
        }
        if input.price_deviation <= dec!(0) {
            return false;
        }
        pin_satisfied >= 4
    }

    /// 检查做多退出条件
    pub fn check_long_exit(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 位置在 80 以上
        let pos_extreme = input.pos_norm_60 > dec!(80);
        // 7个条件满足 >= 4 且位置极端
        pin_satisfied >= 4 && pos_extreme
    }

    /// 检查做空退出条件
    pub fn check_short_exit(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        let pos_extreme = input.pos_norm_60 < dec!(20);
        pin_satisfied >= 4 && pos_extreme
    }

    /// 检查多头对冲条件（前置: tr_base_60min < 15%）
    pub fn check_long_hedge(&self, input: &MinSignalInput) -> bool {
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
    pub fn check_short_hedge(&self, input: &MinSignalInput) -> bool {
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
    pub fn check_exit_high_volatility(&self, input: &MinSignalInput) -> bool {
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
    use rust_decimal_macros::dec;

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
        assert!(!r#gen.generate(&MinSignalInput::default(), &VolatilityTier::Low).long_entry);
    }
}
