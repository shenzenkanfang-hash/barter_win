//! 加仓/对冲检查
//!
//! 逻辑组合：调用 signal_generator 获取信号

use crate::types::DaySignalInput;
use crate::l_1d::signal_generator::DaySignalGenerator;
use crate::l_1d::market_status_generator::DayMarketStatusGenerator;

/// 检查多头对冲（回落对冲）
pub fn check_long_hedge(input: &DaySignalInput) -> bool {
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level_from_signal(input));

    output.long_hedge
}

/// 检查空头对冲（回升对冲）
pub fn check_short_hedge(input: &DaySignalInput) -> bool {
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level_from_signal(input));

    output.short_hedge
}

/// 主检查入口
pub fn check(input: &DaySignalInput) -> bool {
    check_long_hedge(input) || check_short_hedge(input)
}
