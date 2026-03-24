#![forbid(unsafe_code)]

use rust_decimal_macros::dec;
use crate::types::{DaySignalInput, DaySignalOutput, VolatilityTier};

/// 日线级信号生成器（纯指标判断）
///
/// ```text
/// Day Signal Logic (不同于 h_15m 的 7 条件模式)
/// ─────────────────────────────────────────────────────────────────────
///
/// Pine Color Groups (3 组，需全部满足):
///   12_26 组: pine_bar_color_12_26 + pine_bg_color_12_26
///   20_50 组: pine_bar_color_20_50 + pine_bg_color_20_50
///   100_200 组: pine_bar_color_100_200 + pine_bg_color_100_200
///
/// Entry Conditions:
/// ───────────────
/// long_entry:
///   all_green AND (tr_ratio_5d_20d>1 OR tr_ratio_20d_60d>1)
///   AND ma5_in_20d_ma5_pos > 70
///   [2/2 conditions must pass]
///
/// short_entry:
///   all_red_purple AND (tr_ratio_5d_20d>1 OR tr_ratio_20d_60d>1)
///   AND ma5_in_20d_ma5_pos < 30
///   [2/2 conditions must pass]
///
/// Exit Conditions:
/// ──────────────
/// long_exit: max_valid_bg != "纯绿" AND ma5_pos > 50  [2/2]
/// short_exit: max_valid_bg != "纯红" AND ma5_pos < 50  [2/2]
///   (max_valid_bg: 优先取最大周期 100_200 > 20_50 > 12_26)
///
/// Hedge Conditions:
/// ───────────────
/// long_hedge: max_valid_bg == "淡绿" AND ma5_pos > 50
/// short_hedge: max_valid_bg == "淡红" AND ma5_pos < 50
///
/// Color Definitions:
/// ───────────────
/// 纯绿 = pure green (bullish)
/// 纯红 = pure red (bearish)
/// 紫色 = purple
/// 淡绿 = light green
/// 淡红 = light red
/// ```
pub struct DaySignalGenerator;

impl DaySignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &DaySignalInput, _vol_tier: &VolatilityTier) -> DaySignalOutput {
        // 检测 Pine 颜色条件
        let all_green = self.check_all_pine_green(input);
        let all_red_purple = self.check_all_pine_red_purple(input);

        // 入场信号
        let long_entry = self.check_long_entry(input, all_green);
        let short_entry = self.check_short_entry(input, all_red_purple);

        // 退出信号
        let long_exit = self.check_long_exit(input);
        let short_exit = self.check_short_exit(input);

        // 对冲信号
        let long_hedge = self.check_long_hedge(input);
        let short_hedge = self.check_short_hedge(input);

        DaySignalOutput {
            long_entry,
            short_entry,
            long_exit,
            short_exit,
            long_hedge,
            short_hedge,
        }
    }

    /// 检查所有有效组是否都是纯绿
    pub fn check_all_pine_green(&self, input: &DaySignalInput) -> bool {
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
                return false;
            }
        }

        has_valid
    }

    /// 检查所有有效组是否都是紫色/纯红
    pub fn check_all_pine_red_purple(&self, input: &DaySignalInput) -> bool {
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
    pub fn check_long_entry(&self, input: &DaySignalInput, all_green: bool) -> bool {
        if !all_green {
            return false;
        }

        let vol_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(70);

        [vol_condition, pos_condition].iter().filter(|&&x| x).count() >= 2
    }

    /// 检查做空入场条件
    pub fn check_short_entry(&self, input: &DaySignalInput, all_red_purple: bool) -> bool {
        if !all_red_purple {
            return false;
        }

        let vol_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(30);

        [vol_condition, pos_condition].iter().filter(|&&x| x).count() >= 2
    }

    /// 检查做多退出条件
    pub fn check_long_exit(&self, input: &DaySignalInput) -> bool {
        let max_bg = self.get_max_valid_bg(input);
        if max_bg.is_empty() {
            return false;
        }

        // 最大周期背景色非纯绿
        let color_ok = max_bg != "纯绿";
        // ma5 位置 > 50
        let pos_ok = input.ma5_in_20d_ma5_pos > dec!(50);

        [color_ok, pos_ok].iter().filter(|&&x| x).count() >= 2
    }

    /// 检查做空退出条件
    pub fn check_short_exit(&self, input: &DaySignalInput) -> bool {
        let max_bg = self.get_max_valid_bg(input);
        if max_bg.is_empty() {
            return false;
        }

        let color_ok = max_bg != "纯红";
        let pos_ok = input.ma5_in_20d_ma5_pos < dec!(50);

        [color_ok, pos_ok].iter().filter(|&&x| x).count() >= 2
    }

    /// 检查多头对冲条件
    pub fn check_long_hedge(&self, input: &DaySignalInput) -> bool {
        let max_bg = self.get_max_valid_bg(input);
        if max_bg != "淡绿" {
            return false;
        }
        input.ma5_in_20d_ma5_pos > dec!(50)
    }

    /// 检查空头对冲条件
    pub fn check_short_hedge(&self, input: &DaySignalInput) -> bool {
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
