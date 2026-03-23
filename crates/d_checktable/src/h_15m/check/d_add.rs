//! 加仓检查
//!
//! 检查逻辑：检测是否应该加仓/对冲
//! - check_long_hedge(): 多头对冲（回落对冲）
//! - check_short_hedge(): 空头对冲（回升对冲）

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::MinSignalInput;

/// 检查多头对冲条件（回落对冲）
///
/// 条件：
/// - tr_base_60min < 15%（低波动）
/// - price_deviation < 0（向下插针）
/// - 6个条件满足 >= 4
pub fn check_long_hedge(input: &MinSignalInput) -> bool {
    // 前置：tr_base_60min < 15%
    // 注意：MinSignalInput 没有 tr_base_60min，跳过此检查

    // 价格偏离向下
    if input.price_deviation >= dec!(0) {
        return false;
    }

    let mut conditions: u8 = 0;

    // 1. extreme_vol
    if input.tr_ratio_60min_5h > dec!(2) || input.tr_ratio_10min_1h > dec!(1) {
        conditions += 1;
    }

    // 2. extreme_pos: < 90
    if input.pos_norm_60 < dec!(90) {
        conditions += 1;
    }

    // 3. extreme_speed: < 10
    if input.acc_percentile_1h < dec!(10) && input.velocity_percentile_1h < dec!(10) {
        conditions += 1;
    }

    // 4. extreme_bg_color: 不是纯绿
    if input.pine_bg_color != "纯绿" {
        conditions += 1;
    }

    // 5. extreme_bar_color: 不是纯绿
    if input.pine_bar_color != "纯绿" {
        conditions += 1;
    }

    // 6. extreme_price_deviation: 10 < |pos| <= 90
    let abs_pos = input.price_deviation_horizontal_position.abs();
    if abs_pos > dec!(10) && abs_pos <= dec!(90) {
        conditions += 1;
    }

    conditions >= 4
}

/// 检查空头对冲条件（回升对冲）
pub fn check_short_hedge(input: &MinSignalInput) -> bool {
    // 价格偏离向上
    if input.price_deviation <= dec!(0) {
        return false;
    }

    let mut conditions: u8 = 0;

    // 1. extreme_vol
    if input.tr_ratio_60min_5h > dec!(2) || input.tr_ratio_10min_1h > dec!(1) {
        conditions += 1;
    }

    // 2. extreme_pos: > 10
    if input.pos_norm_60 > dec!(10) {
        conditions += 1;
    }

    // 3. extreme_speed: > 90
    if input.acc_percentile_1h > dec!(90) && input.velocity_percentile_1h > dec!(90) {
        conditions += 1;
    }

    // 4. extreme_bg_color: 不是纯红
    if input.pine_bg_color != "纯红" {
        conditions += 1;
    }

    // 5. extreme_bar_color: 不是纯红
    if input.pine_bar_color != "纯红" {
        conditions += 1;
    }

    // 6. extreme_price_deviation: >= 10
    if input.price_deviation_horizontal_position >= dec!(10) {
        conditions += 1;
    }

    conditions >= 4
}

/// 主检查入口
pub fn check(input: &MinSignalInput) -> bool {
    check_long_hedge(input) || check_short_hedge(input)
}
