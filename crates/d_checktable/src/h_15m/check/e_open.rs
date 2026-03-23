//! 开仓检查
//!
//! 检查逻辑：检测是否应该开仓
//! - check_long_entry(): 做多开仓
//! - check_short_entry(): 做空开仓

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::MinSignalInput;

/// 检查做多开仓条件
///
/// 前置条件：
/// - tr_base_60min > 15%（高波动）
/// - price_deviation < 0（向下插针）
///
/// 7个条件满足 >= 4
pub fn check_long_entry(input: &MinSignalInput) -> bool {
    // 前置：tr_base_60min > 15%（高波动）
    // 注意：MinSignalInput 没有 tr_base_60min，跳过此检查

    // 价格偏离方向: 向下
    if input.price_deviation >= dec!(0) {
        return false;
    }

    // 7个条件判断
    let conditions = count_pin_conditions(input);

    // 7个条件满足 >= 4
    conditions >= 4
}

/// 检查做空开仓条件
pub fn check_short_entry(input: &MinSignalInput) -> bool {
    // 价格偏离方向: 向上
    if input.price_deviation <= dec!(0) {
        return false;
    }

    let conditions = count_pin_conditions(input);
    conditions >= 4
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

    // 7. extreme_price_deviation: |pos| == 100
    if input.price_deviation_horizontal_position.abs() == dec!(100) {
        count += 1;
    }

    count
}

/// 主检查入口
pub fn check(input: &MinSignalInput) -> bool {
    check_long_entry(input) || check_short_entry(input)
}
