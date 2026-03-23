//! 加仓/对冲检查
//!
//! 检查逻辑：检测是否应该加仓或对冲
//! - check_long_hedge(): 多头对冲（回落对冲）
//! - check_short_hedge(): 空头对冲（回升对冲）
//!
//! 日线级对冲基于最大有效周期判断

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::DaySignalInput;

/// 检查多头对冲条件（回落对冲）
///
/// 条件：
/// - 基于最大有效周期判断
/// - ma5_in_20d_ma5_pos > 50
/// - 最大周期背景色为淡绿
pub fn check_long_hedge(input: &DaySignalInput) -> bool {
    let (is_valid, max_period) = get_max_valid_period(input);
    if !is_valid {
        return false;
    }

    let max_bg = get_bg_color(input, max_period);

    // 条件1: ma5_in_20d_ma5_pos > 50
    let ma5_pos_ok = input.ma5_in_20d_ma5_pos > dec!(50);

    // 条件2: 最大周期背景色为淡绿
    let color_ok = max_bg == "淡绿";

    // 满足 >= 2 个条件
    [ma5_pos_ok, color_ok].iter().filter(|&&x| x).count() >= 2
}

/// 检查空头对冲条件（回升对冲）
///
/// 条件：
/// - 基于最大有效周期判断
/// - ma5_in_20d_ma5_pos < 50
/// - 最大周期背景色为淡红
pub fn check_short_hedge(input: &DaySignalInput) -> bool {
    let (is_valid, max_period) = get_max_valid_period(input);
    if !is_valid {
        return false;
    }

    let max_bg = get_bg_color(input, max_period);

    // 条件1: ma5_in_20d_ma5_pos < 50
    let ma5_pos_ok = input.ma5_in_20d_ma5_pos < dec!(50);

    // 条件2: 最大周期背景色为淡红
    let color_ok = max_bg == "淡红";

    // 满足 >= 2 个条件
    [ma5_pos_ok, color_ok].iter().filter(|&&x| x).count() >= 2
}

/// 获取最大有效周期
/// 优先级：100_200 > 20_50 > 12_26
fn get_max_valid_period(input: &DaySignalInput) -> (bool, &'static str) {
    let groups = [
        ("100_200", input.pine_bar_color_100_200.as_str(), input.pine_bg_color_100_200.as_str()),
        ("20_50", input.pine_bar_color_20_50.as_str(), input.pine_bg_color_20_50.as_str()),
        ("12_26", input.pine_bar_color_12_26.as_str(), input.pine_bg_color_12_26.as_str()),
    ];

    for (period, bar, bg) in groups.iter() {
        if !bar.is_empty() && !bg.is_empty() && !bar.trim().is_empty() && !bg.trim().is_empty() {
            return (true, *period);
        }
    }

    (false, "")
}

/// 获取指定周期的背景色
fn get_bg_color(input: &DaySignalInput, period: &str) -> String {
    match period {
        "100_200" => input.pine_bg_color_100_200.clone(),
        "20_50" => input.pine_bg_color_20_50.clone(),
        "12_26" => input.pine_bg_color_12_26.clone(),
        _ => String::new(),
    }
}

/// 主检查入口
pub fn check(input: &DaySignalInput) -> bool {
    check_long_hedge(input) || check_short_hedge(input)
}
