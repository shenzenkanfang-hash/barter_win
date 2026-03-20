#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{DaySignalInput, DaySignalOutput};

/// Pine 颜色常量
const PINE_GREEN: &str = "纯绿";
const PINE_RED: &str = "纯红";
const PINE_PURPLE: &str = "紫色";
const PINE_LIGHT_GREEN: &str = "淡绿";
const PINE_LIGHT_RED: &str = "淡红";

/// 周期常量
const PERIOD_12_26: &str = "12_26";
const PERIOD_20_50: &str = "20_50";
const PERIOD_100_200: &str = "100_200";

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
            valid.push(PERIOD_12_26);
        }
        // 20_50 组
        if !input.pine_color_20_50.is_empty() {
            valid.push(PERIOD_20_50);
        }
        // 100_200 组
        if !input.pine_color_100_200.is_empty() {
            valid.push(PERIOD_100_200);
        }

        valid
    }

    /// 获取最大有效周期 (优先级: 100_200 > 20_50 > 12_26)
    fn get_max_valid_period(&self, valid_groups: &[&str]) -> Option<&'static str> {
        for period in [PERIOD_100_200, PERIOD_20_50, PERIOD_12_26] {
            if valid_groups.contains(&period) {
                return Some(period);
            }
        }
        None
    }

    /// 根据周期获取 Pine 颜色
    fn get_pine_color_for_period(&self, input: &DaySignalInput, period: &str) -> Option<&str> {
        match period {
            PERIOD_100_200 => Some(&input.pine_color_100_200),
            PERIOD_20_50 => Some(&input.pine_color_20_50),
            PERIOD_12_26 => Some(&input.pine_color_12_26),
            _ => None,
        }
    }

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 检查最小周期 12_26 (如果有效必须纯绿)
        if valid_groups.contains(&PERIOD_12_26) && input.pine_color_12_26 != PINE_GREEN {
            return false;
        }

        // 所有有效组必须为纯绿
        for &group in valid_groups {
            if let Some(color) = self.get_pine_color_for_period(input, group) {
                if color != PINE_GREEN {
                    return false;
                }
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
        if valid_groups.contains(&PERIOD_12_26) {
            let color = &input.pine_color_12_26;
            if *color != PINE_PURPLE && *color != PINE_RED {
                return false;
            }
        }

        // 所有有效组必须是紫色或纯红
        for &group in valid_groups {
            if let Some(color) = self.get_pine_color_for_period(input, group) {
                if color != PINE_PURPLE && color != PINE_RED {
                    return false;
                }
            }
        }

        let tr_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(30);

        tr_condition && pos_condition
    }

    /// 检查做多平仓条件
    fn check_long_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let Some(bg_color) = self.get_pine_color_for_period(input, period) else { return false; };

        let color_invalid = bg_color != PINE_GREEN;
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(50);

        color_invalid && pos_condition
    }

    /// 检查做空平仓条件
    fn check_short_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let Some(bg_color) = self.get_pine_color_for_period(input, period) else { return false; };

        let color_invalid = bg_color != PINE_RED;
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(50);

        color_invalid && pos_condition
    }

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let Some(bg_color) = self.get_pine_color_for_period(input, period) else { return false; };

        bg_color == PINE_LIGHT_GREEN && input.ma5_in_20d_ma5_pos > dec!(50)
    }

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let Some(bg_color) = self.get_pine_color_for_period(input, period) else { return false; };

        bg_color == PINE_LIGHT_RED && input.ma5_in_20d_ma5_pos < dec!(50)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_input(
        pine_12_26: &str,
        pine_20_50: &str,
        pine_100_200: &str,
    ) -> DaySignalInput {
        DaySignalInput {
            pine_color_12_26: pine_12_26.to_string(),
            pine_color_20_50: pine_20_50.to_string(),
            pine_color_100_200: pine_100_200.to_string(),
            tr_ratio_5d_20d: dec!(1.5),
            tr_ratio_20d_60d: dec!(1.2),
            ma5_in_20d_ma5_pos: dec!(75),
        }
    }

    #[test]
    fn test_validate_pine_color_groups_all_empty() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("", "", "");
        let valid_groups = generator.validate_pine_color_groups(&input);
        assert!(valid_groups.is_empty());
    }

    #[test]
    fn test_validate_pine_color_groups_partial() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("纯绿", "", "");
        let valid_groups = generator.validate_pine_color_groups(&input);
        assert_eq!(valid_groups.len(), 1);
        assert!(valid_groups.contains(&PERIOD_12_26));
    }

    #[test]
    fn test_get_max_valid_period_priority() {
        let generator = DaySignalGenerator::new();

        // 只有 12_26
        let groups_12_26 = vec![PERIOD_12_26];
        assert_eq!(generator.get_max_valid_period(&groups_12_26), Some(PERIOD_12_26));

        // 有 20_50 和 12_26，应该返回 20_50
        let groups_20_50 = vec![PERIOD_20_50, PERIOD_12_26];
        assert_eq!(generator.get_max_valid_period(&groups_20_50), Some(PERIOD_20_50));

        // 有 100_200，应该返回 100_200
        let groups_all = vec![PERIOD_12_26, PERIOD_20_50, PERIOD_100_200];
        assert_eq!(generator.get_max_valid_period(&groups_all), Some(PERIOD_100_200));
    }

    #[test]
    fn test_get_pine_color_for_period() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("纯绿", "紫色", "淡红");

        assert_eq!(generator.get_pine_color_for_period(&input, PERIOD_12_26), Some("纯绿"));
        assert_eq!(generator.get_pine_color_for_period(&input, PERIOD_20_50), Some("紫色"));
        assert_eq!(generator.get_pine_color_for_period(&input, PERIOD_100_200), Some("淡红"));
        assert_eq!(generator.get_pine_color_for_period(&input, "unknown"), None);
    }

    #[test]
    fn test_check_long_entry_all_green_tr_high() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("纯绿", "纯绿", "纯绿");

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_long_entry(&input, &valid_groups);

        assert!(result); // TR=1.5>1, pos=75>70, all green
    }

    #[test]
    fn test_check_long_entry_not_all_green() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("纯绿", "紫色", "纯绿");

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_long_entry(&input, &valid_groups);

        assert!(!result); // 20_50 is not green
    }

    #[test]
    fn test_check_short_entry_purple_tr_high() {
        let generator = DaySignalGenerator::new();
        let mut input = create_test_input("紫色", "紫色", "紫色");
        input.ma5_in_20d_ma5_pos = dec!(25); // pos < 30

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_short_entry(&input, &valid_groups);

        assert!(result); // TR>1, pos<30, all purple
    }

    #[test]
    fn test_check_short_entry_red_pos_low() {
        let generator = DaySignalGenerator::new();
        let mut input = create_test_input("纯红", "纯红", "纯红");
        input.ma5_in_20d_ma5_pos = dec!(20); // pos < 30

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_short_entry(&input, &valid_groups);

        assert!(result); // TR>1, pos<30, all red
    }

    #[test]
    fn test_generate_returns_default_when_no_valid_groups() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("", "", "");

        let output = generator.generate(&input);

        // No valid groups, should return default (all false)
        assert!(!output.long_entry);
        assert!(!output.short_entry);
        assert!(!output.long_exit);
        assert!(!output.short_exit);
        assert!(!output.long_hedge);
        assert!(!output.short_hedge);
    }
}
