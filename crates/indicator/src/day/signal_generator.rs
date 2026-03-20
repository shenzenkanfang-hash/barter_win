#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{DaySignalInput, DaySignalOutput};

/// 日线级信号生成器
pub struct DaySignalGenerator;

impl DaySignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &DaySignalInput) -> DaySignalOutput {
        // Pine颜色分组验证
        let valid_groups = self.validate_pine_color_groups(input);
        if valid_groups.is_empty() {
            return DaySignalOutput::default();
        }

        let max_valid_period = self.get_max_valid_period(&valid_groups);

        // 入场信号
        let long_entry = self.check_long_entry(input, &valid_groups);
        let short_entry = self.check_short_entry(input, &valid_groups);

        // 平仓信号 (使用最大有效周期)
        let long_exit = self.check_long_exit(input, max_valid_period);
        let short_exit = self.check_short_exit(input, max_valid_period);

        // 对冲信号
        let long_hedge = self.check_long_hedge(input, max_valid_period);
        let short_hedge = self.check_short_hedge(input, max_valid_period);

        DaySignalOutput {
            long_entry,
            short_entry,
            long_exit,
            short_exit,
            long_hedge,
            short_hedge,
        }
    }

    /// 验证 Pine 颜色分组 (返回有效组列表)
    fn validate_pine_color_groups(&self, input: &DaySignalInput) -> Vec<&'static str> {
        let mut valid = Vec::new();

        // 12_26 组
        if !input.pine_color_12_26.is_empty() {
            valid.push("12_26");
        }
        // 20_50 组
        if !input.pine_color_20_50.is_empty() {
            valid.push("20_50");
        }
        // 100_200 组
        if !input.pine_color_100_200.is_empty() {
            valid.push("100_200");
        }

        valid
    }

    /// 获取最大有效周期 (优先级: 100_200 > 20_50 > 12_26)
    fn get_max_valid_period(&self, valid_groups: &[&str]) -> Option<&'static str> {
        for period in ["100_200", "20_50", "12_26"] {
            if valid_groups.contains(&period) {
                return Some(period);
            }
        }
        None
    }

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 检查最小周期 12_26 (如果有效必须纯绿)
        if valid_groups.contains(&"12_26") && input.pine_color_12_26 != "纯绿" {
            return false;
        }

        // 所有有效组必须为纯绿
        for &group in valid_groups {
            let color = match group {
                "12_26" => &input.pine_color_12_26,
                "20_50" => &input.pine_color_20_50,
                "100_200" => &input.pine_color_100_200,
                _ => continue,
            };
            if *color != "纯绿" {
                return false;
            }
        }

        // TR > 1
        let tr_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        // MA5 位置 > 70
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(70);

        tr_condition && pos_condition
    }

    /// 检查做空入场条件
    fn check_short_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 检查最小周期 12_26 (如果有效必须是紫色或纯红)
        if valid_groups.contains(&"12_26") {
            let color = &input.pine_color_12_26;
            if *color != "紫色" && *color != "纯红" {
                return false;
            }
        }

        // 所有有效组必须是紫色或纯红
        for &group in valid_groups {
            let color = match group {
                "12_26" => &input.pine_color_12_26,
                "20_50" => &input.pine_color_20_50,
                "100_200" => &input.pine_color_100_200,
                _ => continue,
            };
            if *color != "紫色" && *color != "纯红" {
                return false;
            }
        }

        let tr_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(30);

        tr_condition && pos_condition
    }

    /// 检查做多平仓条件
    fn check_long_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        let color_invalid = *bg_color != "纯绿";
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(50);

        color_invalid && pos_condition
    }

    /// 检查做空平仓条件
    fn check_short_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        let color_invalid = *bg_color != "纯红";
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(50);

        color_invalid && pos_condition
    }

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        bg_color == "淡绿" && input.ma5_in_20d_ma5_pos > dec!(50)
    }

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        bg_color == "淡红" && input.ma5_in_20d_ma5_pos < dec!(50)
    }
}

impl Default for DaySignalOutput {
    fn default() -> Self {
        Self {
            long_entry: false,
            short_entry: false,
            long_exit: false,
            short_exit: false,
            long_hedge: false,
            short_hedge: false,
        }
    }
}
