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

/// Pine 颜色组 (bar + bg)
struct PineColorGroup {
    bar: String,
    bg: String,
}

impl PineColorGroup {
    fn new(bar: String, bg: String) -> Self {
        Self { bar, bg }
    }

    /// 检查组是否有效 (bar 和 bg 都有非空值)
    fn is_valid(&self) -> bool {
        !self.bar.trim().is_empty() && !self.bg.trim().is_empty()
    }

    /// 检查是否全绿
    fn is_all_green(&self) -> bool {
        self.bar == PINE_GREEN && self.bg == PINE_GREEN
    }

    /// 检查是否全红/紫 (bar=紫, bg=红)
    fn is_all_red_purple(&self) -> bool {
        self.bar == PINE_PURPLE && self.bg == PINE_RED
    }
}

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

    /// 获取指定周期的 Pine 颜色组
    fn get_color_group(&self, input: &DaySignalInput, period: &str) -> PineColorGroup {
        match period {
            PERIOD_12_26 => PineColorGroup::new(
                input.pine_bar_color_12_26.clone(),
                input.pine_bg_color_12_26.clone(),
            ),
            PERIOD_20_50 => PineColorGroup::new(
                input.pine_bar_color_20_50.clone(),
                input.pine_bg_color_20_50.clone(),
            ),
            PERIOD_100_200 => PineColorGroup::new(
                input.pine_bar_color_100_200.clone(),
                input.pine_bg_color_100_200.clone(),
            ),
            _ => PineColorGroup::new(String::new(), String::new()),
        }
    }

    /// 验证 Pine 颜色分组 (返回有效组列表)
    /// 规则:
    /// 1. 组有效 = bar 和 bg 都有非空值
    /// 2. 至少 1 个有效组才算数据有效
    fn validate_pine_color_groups(&self, input: &DaySignalInput) -> Vec<&'static str> {
        let mut valid = Vec::new();

        let group_12_26 = self.get_color_group(input, PERIOD_12_26);
        if group_12_26.is_valid() {
            valid.push(PERIOD_12_26);
        }

        let group_20_50 = self.get_color_group(input, PERIOD_20_50);
        if group_20_50.is_valid() {
            valid.push(PERIOD_20_50);
        }

        let group_100_200 = self.get_color_group(input, PERIOD_100_200);
        if group_100_200.is_valid() {
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

    /// 检查做多入场条件
    /// 规则:
    /// 1. 至少 1 个有效组
    /// 2. 最小周期 12_26 若有效，必须是纯绿
    /// 3. 所有有效组的 bar+bg 必须都是纯绿
    fn check_long_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 优先检查最小周期 12_26 (有效则必须纯绿)
        if valid_groups.contains(&PERIOD_12_26) {
            let group = self.get_color_group(input, PERIOD_12_26);
            if !group.is_all_green() {
                return false;
            }
        }

        // 所有有效组必须为纯绿
        for &group_name in valid_groups {
            let group = self.get_color_group(input, group_name);
            if !group.is_all_green() {
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
    /// 规则:
    /// 1. 至少 1 个有效组
    /// 2. 最小周期 12_26 若有效，必须是紫色/纯红
    /// 3. 所有有效组的 bar+bg 必须都是紫色/纯红
    fn check_short_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 优先检查最小周期 12_26 (有效则必须是紫色/纯红)
        if valid_groups.contains(&PERIOD_12_26) {
            let group = self.get_color_group(input, PERIOD_12_26);
            if !group.is_all_red_purple() {
                return false;
            }
        }

        // 所有有效组必须是紫色/纯红
        for &group_name in valid_groups {
            let group = self.get_color_group(input, group_name);
            if !group.is_all_red_purple() {
                return false;
            }
        }

        let tr_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(30);

        tr_condition && pos_condition
    }

    /// 检查做多平仓条件 (使用最大有效周期)
    /// 核心: 最大周期颜色非纯绿
    fn check_long_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let group = self.get_color_group(input, period);

        // 最大有效周期颜色非纯绿
        let color_invalid = group.bg != PINE_GREEN;
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(50);

        color_invalid && pos_condition
    }

    /// 检查做空平仓条件 (使用最大有效周期)
    /// 核心: 最大周期颜色非纯红
    fn check_short_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let group = self.get_color_group(input, period);

        // 最大有效周期颜色非纯红
        let color_invalid = group.bg != PINE_RED;
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(50);

        color_invalid && pos_condition
    }

    /// 检查多头对冲条件 (使用最大有效周期)
    /// 核心: 最大周期 bg = 淡绿
    fn check_long_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let group = self.get_color_group(input, period);

        group.bg == PINE_LIGHT_GREEN && input.ma5_in_20d_ma5_pos > dec!(50)
    }

    /// 检查空头对冲条件 (使用最大有效周期)
    /// 核心: 最大周期 bg = 淡红
    fn check_short_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };
        let group = self.get_color_group(input, period);

        group.bg == PINE_LIGHT_RED && input.ma5_in_20d_ma5_pos < dec!(50)
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
        bar_12_26: &str,
        bg_12_26: &str,
        bar_20_50: &str,
        bg_20_50: &str,
        bar_100_200: &str,
        bg_100_200: &str,
    ) -> DaySignalInput {
        DaySignalInput {
            pine_bar_color_12_26: bar_12_26.to_string(),
            pine_bg_color_12_26: bg_12_26.to_string(),
            pine_bar_color_20_50: bar_20_50.to_string(),
            pine_bg_color_20_50: bg_20_50.to_string(),
            pine_bar_color_100_200: bar_100_200.to_string(),
            pine_bg_color_100_200: bg_100_200.to_string(),
            tr_ratio_5d_20d: dec!(1.5),
            tr_ratio_20d_60d: dec!(1.2),
            ma5_in_20d_ma5_pos: dec!(75),
        }
    }

    #[test]
    fn test_validate_pine_color_groups_all_empty() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("", "", "", "", "", "");
        let valid_groups = generator.validate_pine_color_groups(&input);
        assert!(valid_groups.is_empty());
    }

    #[test]
    fn test_validate_pine_color_groups_partial() {
        let generator = DaySignalGenerator::new();
        // 只有 12_26 组有效 (bar 和 bg 都有值)
        let input = create_test_input("纯绿", "纯绿", "", "", "", "");
        let valid_groups = generator.validate_pine_color_groups(&input);
        assert_eq!(valid_groups.len(), 1);
        assert!(valid_groups.contains(&PERIOD_12_26));
    }

    #[test]
    fn test_validate_pine_color_groups_bar_only_invalid() {
        let generator = DaySignalGenerator::new();
        // 只有 bar 有值，bg 为空 -> 无效
        let input = create_test_input("纯绿", "", "", "", "", "");
        let valid_groups = generator.validate_pine_color_groups(&input);
        assert!(valid_groups.is_empty());
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
    fn test_check_long_entry_all_green_tr_high() {
        let generator = DaySignalGenerator::new();
        // 所有组全绿，TR > 1, pos > 70
        let input = create_test_input("纯绿", "纯绿", "纯绿", "纯绿", "纯绿", "纯绿");

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_long_entry(&input, &valid_groups);

        assert!(result); // TR=1.5>1, pos=75>70, all green
    }

    #[test]
    fn test_check_long_entry_not_all_green() {
        let generator = DaySignalGenerator::new();
        // 20_50 不是全绿
        let input = create_test_input("纯绿", "纯绿", "紫色", "纯红", "纯绿", "纯绿");

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_long_entry(&input, &valid_groups);

        assert!(!result); // 20_50 is not all green
    }

    #[test]
    fn test_check_long_entry_min_period_not_green() {
        let generator = DaySignalGenerator::new();
        // 最小周期 12_26 不是全绿
        let input = create_test_input("紫色", "纯红", "纯绿", "纯绿", "纯绿", "纯绿");

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_long_entry(&input, &valid_groups);

        assert!(!result); // 12_26 min period not green
    }

    #[test]
    fn test_check_short_entry_all_purple_tr_high() {
        let generator = DaySignalGenerator::new();
        // 所有组全紫/红，TR > 1, pos < 30
        let mut input = create_test_input("紫色", "纯红", "紫色", "纯红", "紫色", "纯红");
        input.ma5_in_20d_ma5_pos = dec!(25);

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_short_entry(&input, &valid_groups);

        assert!(result); // TR>1, pos<30, all purple/red
    }

    #[test]
    fn test_check_short_entry_red_pos_low() {
        let generator = DaySignalGenerator::new();
        let mut input = create_test_input("纯红", "纯红", "纯红", "纯红", "纯红", "纯红");
        input.ma5_in_20d_ma5_pos = dec!(20);

        let valid_groups = generator.validate_pine_color_groups(&input);
        let result = generator.check_short_entry(&input, &valid_groups);

        assert!(result); // TR>1, pos<30, all red
    }

    #[test]
    fn test_generate_returns_default_when_no_valid_groups() {
        let generator = DaySignalGenerator::new();
        let input = create_test_input("", "", "", "", "", "");

        let output = generator.generate(&input);

        // No valid groups, should return default (all false)
        assert!(!output.long_entry);
        assert!(!output.short_entry);
        assert!(!output.long_exit);
        assert!(!output.short_exit);
        assert!(!output.long_hedge);
        assert!(!output.short_hedge);
    }

    #[test]
    fn test_long_exit_with_max_period() {
        let generator = DaySignalGenerator::new();
        // 最大周期 100_200，bg 非纯绿
        let input = create_test_input("", "", "", "", "纯绿", "紫色");

        let valid_groups = generator.validate_pine_color_groups(&input);
        let max_period = generator.get_max_valid_period(&valid_groups);
        let result = generator.check_long_exit(&input, max_period);

        assert!(result); // bg=紫色 != 纯绿, pos>50
    }

    #[test]
    fn test_short_exit_with_max_period() {
        let generator = DaySignalGenerator::new();
        // 最大周期 100_200，bg 非纯红
        let mut input = create_test_input("", "", "", "", "纯绿", "纯绿");
        input.ma5_in_20d_ma5_pos = dec!(40);

        let valid_groups = generator.validate_pine_color_groups(&input);
        let max_period = generator.get_max_valid_period(&valid_groups);
        let result = generator.check_short_exit(&input, max_period);

        assert!(result); // bg=纯绿 != 纯红, pos<50
    }
}
