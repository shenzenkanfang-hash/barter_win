//! 开仓检查
//!
//! 检查逻辑：检测是否应该开仓
//! - check_long_entry(): 做多开仓
//! - check_short_entry(): 做空开仓
//!
//! 日线级开仓检查所有有效Pine颜色组

use rust_decimal_macros::dec;
use crate::types::DaySignalInput;

/// 检查做多开仓条件
///
/// 条件：
/// - 所有有效Pine颜色组必须为纯绿
/// - tr_ratio_5d_20d > 1 或 tr_ratio_20d_60d > 1
/// - ma5_in_20d_ma5_pos > 70
pub fn check_long_entry(input: &DaySignalInput) -> bool {
    // 检查所有有效组颜色是否都是纯绿
    if !all_pine_green_for_long(input) {
        return false;
    }

    // 条件判断
    let conditions = count_long_conditions(input);

    // 满足 >= 2 个条件
    conditions >= 2
}

/// 检查做空开仓条件
///
/// 条件：
/// - 所有有效Pine颜色组必须为紫色/纯红
/// - tr_ratio_5d_20d > 1 或 tr_ratio_20d_60d > 1
/// - ma5_in_20d_ma5_pos < 30
pub fn check_short_entry(input: &DaySignalInput) -> bool {
    // 检查所有有效组颜色是否都是紫色/纯红
    if !all_pine_red_purple_for_short(input) {
        return false;
    }

    // 条件判断
    let conditions = count_short_conditions(input);

    // 满足 >= 2 个条件
    conditions >= 2
}

/// 检查所有有效组是否都是纯绿（做多）
fn all_pine_green_for_long(input: &DaySignalInput) -> bool {
    let groups = [
        ("12_26", input.pine_bar_color_12_26.as_str(), input.pine_bg_color_12_26.as_str()),
        ("20_50", input.pine_bar_color_20_50.as_str(), input.pine_bg_color_20_50.as_str()),
        ("100_200", input.pine_bar_color_100_200.as_str(), input.pine_bg_color_100_200.as_str()),
    ];

    let mut has_valid_group = false;

    for (period, bar, bg) in groups.iter() {
        let bar = bar.trim();
        let bg = bg.trim();

        // 跳过空值组
        if bar.is_empty() && bg.is_empty() {
            continue;
        }

        // 如果有任一组有值，则认为该组有效
        if !bar.is_empty() || !bg.is_empty() {
            has_valid_group = true;

            // 有效组必须bar和bg都有值
            if bar.is_empty() || bg.is_empty() {
                return false;
            }

            // 最小周期12_26若有效，必须是纯绿
            if *period == "12_26" {
                if bar != "纯绿" || bg != "纯绿" {
                    return false;
                }
            } else {
                // 其他有效组也必须都是纯绿
                if bar != "纯绿" || bg != "纯绿" {
                    return false;
                }
            }
        }
    }

    has_valid_group
}

/// 检查所有有效组是否都是紫色/纯红（做空）
fn all_pine_red_purple_for_short(input: &DaySignalInput) -> bool {
    let groups = [
        ("12_26", input.pine_bar_color_12_26.as_str(), input.pine_bg_color_12_26.as_str()),
        ("20_50", input.pine_bar_color_20_50.as_str(), input.pine_bg_color_20_50.as_str()),
        ("100_200", input.pine_bar_color_100_200.as_str(), input.pine_bg_color_100_200.as_str()),
    ];

    let mut has_valid_group = false;

    for (period, bar, bg) in groups.iter() {
        let bar = bar.trim();
        let bg = bg.trim();

        // 跳过空值组
        if bar.is_empty() && bg.is_empty() {
            continue;
        }

        // 如果有任一组有值，则认为该组有效
        if !bar.is_empty() || !bg.is_empty() {
            has_valid_group = true;

            // 有效组必须bar和bg都有值
            if bar.is_empty() || bg.is_empty() {
                return false;
            }

            // 最小周期12_26若有效，必须是紫色/纯红
            if *period == "12_26" {
                if bar != "紫色" || bg != "纯红" {
                    return false;
                }
            } else {
                // 其他有效组也必须都是紫色/纯红
                if bar != "紫色" || bg != "纯红" {
                    return false;
                }
            }
        }
    }

    has_valid_group
}

/// 统计做多开仓满足的条件数量
fn count_long_conditions(input: &DaySignalInput) -> u8 {
    let mut count: u8 = 0;

    // 1. extreme_vol: tr_ratio > 1
    if input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1) {
        count += 1;
    }

    // 2. ma5_pos > 70
    if input.ma5_in_20d_ma5_pos > dec!(70) {
        count += 1;
    }

    count
}

/// 统计做空开仓满足的条件数量
fn count_short_conditions(input: &DaySignalInput) -> u8 {
    let mut count: u8 = 0;

    // 1. extreme_vol: tr_ratio > 1
    if input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1) {
        count += 1;
    }

    // 2. ma5_pos < 30
    if input.ma5_in_20d_ma5_pos < dec!(30) {
        count += 1;
    }

    count
}

/// 主检查入口
pub fn check(input: &DaySignalInput) -> bool {
    check_long_entry(input) || check_short_entry(input)
}
