#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{DaySignalInput, DaySignalOutput, VolatilityLevel};

/// 日线级信号生成器
pub struct DaySignalGenerator;

impl DaySignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &DaySignalInput, _vol_level: &VolatilityLevel) -> DaySignalOutput {
        // 检测 Pine 颜色条件
        let all_green = self.check_all_pine_green(input);
        let all_red_purple = self.check_all_pine_red_purple(input);

        // 入场信号
        let long_entry = self.check_long_entry(input, all_green);
        let short_entry = self.check_short_entry(input, all_red_purple);

        // 对冲信号
        let long_hedge = self.check_long_hedge(input);
        let short_hedge = self.check_short_hedge(input);

        DaySignalOutput {
            long_entry,
            short_entry,
            long_exit: false, // 由 PriceControl 判断
            short_exit: false,
            long_hedge,
            short_hedge,
        }
    }

    /// 检查所有有效组是否都是纯绿
    fn check_all_pine_green(&self, input: &DaySignalInput) -> bool {
        let groups = [
            (input.pine_bar_color_12_26.as_str(), input.pine_bg_color_12_26.as_str()),
            (input.pine_bar_color_20_50.as_str(), input.pine_bg_color_20_50.as_str()),
            (input.pine_bar_color_100_200.as_str(), input.pine_bg_color_100_200.as_str()),
        ];

        let mut has_valid = false;

        for (bar, bg) in groups.iter() {
            let bar = bar.trim();
            let bg = bg.trim();

            if bar.is_empty() && bg.is_empty() {
                continue;
            }

            if !bar.is_empty() && !bg.is_empty() {
                has_valid = true;
                if bar != "纯绿" || bg != "纯绿" {
                    return false;
                }
            } else {
                return false; // 半有效组无效
            }
        }

        has_valid
    }

    /// 检查所有有效组是否都是紫色/纯红
    fn check_all_pine_red_purple(&self, input: &DaySignalInput) -> bool {
        let groups = [
            (input.pine_bar_color_12_26.as_str(), input.pine_bg_color_12_26.as_str()),
            (input.pine_bar_color_20_50.as_str(), input.pine_bg_color_20_50.as_str()),
            (input.pine_bar_color_100_200.as_str(), input.pine_bg_color_100_200.as_str()),
        ];

        let mut has_valid = false;

        for (bar, bg) in groups.iter() {
            let bar = bar.trim();
            let bg = bg.trim();

            if bar.is_empty() && bg.is_empty() {
                continue;
            }

            if !bar.is_empty() && !bg.is_empty() {
                has_valid = true;
                if bar != "紫色" && bar != "纯红" || bg != "纯红" {
                    return false;
                }
            } else {
                return false;
            }
        }

        has_valid
    }

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &DaySignalInput, all_green: bool) -> bool {
        if !all_green {
            return false;
        }

        // 条件判断
        let vol_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(70);

        [vol_condition, pos_condition].iter().filter(|&&x| x).count() >= 2
    }

    /// 检查做空入场条件
    fn check_short_entry(&self, input: &DaySignalInput, all_red_purple: bool) -> bool {
        if !all_red_purple {
            return false;
        }

        let vol_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(30);

        [vol_condition, pos_condition].iter().filter(|&&x| x).count() >= 2
    }

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &DaySignalInput) -> bool {
        // 日线级对冲条件基于最大有效周期
        let max_bg = self.get_max_valid_bg(input);

        if max_bg != "淡绿" {
            return false;
        }

        input.ma5_in_20d_ma5_pos > dec!(50)
    }

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &DaySignalInput) -> bool {
        let max_bg = self.get_max_valid_bg(input);

        if max_bg != "淡红" {
            return false;
        }

        input.ma5_in_20d_ma5_pos < dec!(50)
    }

    /// 获取最大有效周期的背景色
    fn get_max_valid_bg(&self, input: &DaySignalInput) -> String {
        let groups = [
            ("100_200", input.pine_bg_color_100_200.as_str()),
            ("20_50", input.pine_bg_color_20_50.as_str()),
            ("12_26", input.pine_bg_color_12_26.as_str()),
        ];

        for (_, bg) in groups.iter() {
            let bg = bg.trim();
            if !bg.is_empty() {
                return bg.to_string();
            }
        }

        String::new()
    }
}

impl Default for DaySignalGenerator {
    fn default() -> Self {
        Self::new()
    }
}
