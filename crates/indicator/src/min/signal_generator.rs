#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityLevel};

/// 分钟级信号生成器
pub struct MinSignalGenerator;

impl MinSignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &MinSignalInput, _vol_level: &VolatilityLevel) -> MinSignalOutput {
        // 前置条件: tr_base_60min > 15%
        if input.tr_base_60min <= dec!(0.15) {
            return MinSignalOutput::default();
        }

        // 检测插针条件
        let pin_satisfied = self.count_pin_conditions(input);

        // 入场信号 (前置: tr_base_60min > 15%)
        let long_entry = self.check_long_entry(input, pin_satisfied);
        let short_entry = self.check_short_entry(input, pin_satisfied);

        // 对冲信号 (前置: tr_base_60min < 15%)
        let long_hedge = self.check_long_hedge(input);
        let short_hedge = self.check_short_hedge(input);

        // 退出高波动
        let exit_high_volatility = self.check_exit_high_volatility(input);

        MinSignalOutput {
            long_entry,
            short_entry,
            long_exit: false, // 由 PriceControl 判断
            short_exit: false,
            long_hedge,
            short_hedge,
            exit_high_volatility,
        }
    }

    /// 统计满足的插针条件数量
    fn count_pin_conditions(&self, input: &MinSignalInput) -> u8 {
        let mut count: u8 = 0;

        if input.zscore_14_1m.abs() > dec!(2) || input.zscore_1h_1m.abs() > dec!(2) {
            count += 1;
        }
        if input.tr_ratio_60min_5h > dec!(1) || input.tr_ratio_10min_1h > dec!(1) {
            count += 1;
        }
        if input.pos_norm_60 > dec!(90) || input.pos_norm_60 < dec!(10) {
            count += 1;
        }
        if input.acc_percentile_1h > dec!(90) {
            count += 1;
        }
        if input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红" {
            count += 1;
        }
        if input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红" {
            count += 1;
        }
        if input.price_deviation_horizontal_position.abs() == dec!(100) {
            count += 1;
        }

        count
    }

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 价格偏离方向: 向下
        if input.price_deviation >= dec!(0) {
            return false;
        }
        // 7个条件满足 >= 4
        pin_satisfied >= 4
    }

    /// 检查做空入场条件
    fn check_short_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 价格偏离方向: 向上
        if input.price_deviation <= dec!(0) {
            return false;
        }
        // 7个条件满足 >= 4
        pin_satisfied >= 4
    }

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &MinSignalInput) -> bool {
        // 前置: tr_base_60min < 15%
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }
        // 价格偏离向下
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
        if dec!(10) < input.price_deviation_horizontal_position.abs() && input.price_deviation_horizontal_position.abs() <= dec!(90) {
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
        if input.price_deviation_horizontal_position >= dec!(10) {
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

        let satisfied = [cond1, cond2, cond3].iter().filter(|&&x| x).count();
        satisfied >= 2
    }
}

impl Default for MinSignalOutput {
    fn default() -> Self {
        Self {
            long_entry: false,
            short_entry: false,
            long_exit: false,
            short_exit: false,
            long_hedge: false,
            short_hedge: false,
            exit_high_volatility: false,
        }
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
        let gen = MinSignalGenerator::new();
        assert!(gen.generate(&MinSignalInput::default(), &VolatilityLevel::NORMAL).long_entry == false);
    }
}
