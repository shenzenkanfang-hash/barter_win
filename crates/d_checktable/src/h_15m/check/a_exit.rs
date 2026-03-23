//! 退出检查
//!
//! 逻辑组合：调用 signal_generator 获取信号，结合仓位状态判断

use crate::types::MinSignalInput;
use crate::h_15m::signal_generator::MinSignalGenerator;
use crate::h_15m::market_status_generator::MinMarketStatusGenerator;

/// 检查多头退出
pub fn check_long_exit(input: &MinSignalInput) -> bool {
    let generator = MinSignalGenerator::new();
    let status_gen = MinMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level(input.tr_ratio_15min));

    // 多头退出：signal 发出 long_exit 信号
    output.long_exit
}

/// 检查空头退出
pub fn check_short_exit(input: &MinSignalInput) -> bool {
    let generator = MinSignalGenerator::new();
    let status_gen = MinMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level(input.tr_ratio_15min));

    output.short_exit
}

/// 主检查入口
pub fn check(input: &MinSignalInput) -> bool {
    check_long_exit(input) || check_short_exit(input)
}
