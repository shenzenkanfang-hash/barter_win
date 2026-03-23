//! 加仓/对冲检查
//!
//! 逻辑组合：调用 signal_generator 获取信号，结合仓位状态判断

use crate::types::MinSignalInput;
use crate::h_15m::signal_generator::MinSignalGenerator;
use crate::h_15m::market_status_generator::MinMarketStatusGenerator;

/// 检查多头对冲（回落对冲）
pub fn check_long_hedge(input: &MinSignalInput) -> bool {
    let generator = MinSignalGenerator::new();
    let status_gen = MinMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level(input.tr_ratio_15min));

    output.long_hedge
}

/// 检查空头对冲（回升对冲）
pub fn check_short_hedge(input: &MinSignalInput) -> bool {
    let generator = MinSignalGenerator::new();
    let status_gen = MinMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level(input.tr_ratio_15min));

    output.short_hedge
}

/// 主检查入口
pub fn check(input: &MinSignalInput) -> bool {
    check_long_hedge(input) || check_short_hedge(input)
}
