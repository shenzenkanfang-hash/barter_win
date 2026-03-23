//! 风控检查
//!
//! 检查逻辑：检测风控条件
//! - check_exit_high_volatility(): 高波动退出
//! - 其他风控条件检测

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::MinSignalInput;

/// 检查退出高波动条件
///
/// 条件：
/// 1. tr_ratio_60min_5h < 1 且 tr_ratio_10min_1h < 1（波动率回落）
/// 2. pos_norm_60 在 20~80 之间（仓位正常）
/// 3. price_deviation_horizontal_position 在 10~90 之间
///
/// 需要满足 >= 2 个条件
pub fn check_exit_high_volatility(input: &MinSignalInput) -> bool {
    // 前置：tr_base_60min < 15%（低波动才考虑退出）
    // 注意：MinSignalInput 没有 tr_base_60min，跳过此检查

    // 条件1：波动率回落
    let cond1 = input.tr_ratio_60min_5h < dec!(1) && input.tr_ratio_10min_1h < dec!(1);

    // 条件2：仓位正常
    let cond2 = input.pos_norm_60 > dec!(20) && input.pos_norm_60 < dec!(80);

    // 条件3：价格偏离度合理
    let cond3 = input.price_deviation_horizontal_position.abs() > dec!(10)
        && input.price_deviation_horizontal_position.abs() <= dec!(90);

    let satisfied = [cond1, cond2, cond3].iter().filter(|&&x| x).count();
    satisfied >= 2
}

/// 主检查入口
pub fn check(input: &MinSignalInput) -> bool {
    check_exit_high_volatility(input)
}
