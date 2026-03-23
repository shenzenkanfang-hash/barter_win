//! 退出检查
//!
//! 检查逻辑：检测是否应该退出当前仓位
//! - check_long_exit(): 多头退出
//! - check_short_exit(): 空头退出

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::MinSignalInput;

/// 检查多头退出条件
pub fn check_long_exit(input: &MinSignalInput) -> bool {
    // 7个插针条件判断
    let conditions = count_pin_conditions(input);

    // 多头退出：位置在 80 以上
    let pos_extreme = input.pos_norm_60 > dec!(80);

    // 7个条件满足 >= 4 且位置极端
    conditions >= 4 && pos_extreme
}

/// 检查空头退出条件
pub fn check_short_exit(input: &MinSignalInput) -> bool {
    let conditions = count_pin_conditions(input);

    // 空头退出：位置在 20 以下
    let pos_extreme = input.pos_norm_60 < dec!(20);

    conditions >= 4 && pos_extreme
}

/// 统计满足的插针条件数量
fn count_pin_conditions(input: &MinSignalInput) -> u8 {
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

    // 4. extreme_speed: acc_percentile > 90 或 < 10
    if input.acc_percentile_1h > dec!(90) || input.acc_percentile_1h < dec!(10) {
        count += 1;
    }

    // 5. extreme_bg_color
    if input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红" {
        count += 1;
    }

    // 6. extreme_bar_color
    if input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红" {
        count += 1;
    }

    // 7. extreme_price_deviation
    if input.price_deviation_horizontal_position.abs() == dec!(100) {
        count += 1;
    }

    count
}

/// 主检查入口（兼容 check_chain 调用）
pub fn check(input: &MinSignalInput) -> bool {
    // 退出检查不区分多空，统一返回是否有退出信号
    check_long_exit(input) || check_short_exit(input)
}
