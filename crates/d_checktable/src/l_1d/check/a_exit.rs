//! 退出检查
//!
//! 逻辑组合：调用 signal_generator 获取信号

use crate::types::DaySignalInput;
use crate::l_1d::signal_generator::DaySignalGenerator;
use crate::l_1d::market_status_generator::DayMarketStatusGenerator;

/// 检查多头退出
pub fn check_long_exit(input: &DaySignalInput) -> bool {
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level_from_signal(input));

    output.long_exit
}

/// 检查空头退出
pub fn check_short_exit(input: &DaySignalInput) -> bool {
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level_from_signal(input));

    output.short_exit
}

/// 主检查入口
pub fn check(input: &DaySignalInput) -> bool {
    check_long_exit(input) || check_short_exit(input)
}
