//! 开仓检查
//!
//! 逻辑组合：调用 signal_generator 获取信号，结合仓位状态判断

use crate::types::MinSignalInput;
use crate::h_15m::signal_generator::MinSignalGenerator;
use crate::h_15m::market_status_generator::MinMarketStatusGenerator;

/// 检查做多开仓
pub fn check_long_entry(input: &MinSignalInput) -> bool {
    let generator = MinSignalGenerator::new();
    let status_gen = MinMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level(input.tr_ratio_15min));

    output.long_entry
}

/// 检查做空开仓
pub fn check_short_entry(input: &MinSignalInput) -> bool {
    let generator = MinSignalGenerator::new();
    let status_gen = MinMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level(input.tr_ratio_15min));

    output.short_entry
}

/// 主检查入口
pub fn check(input: &MinSignalInput) -> bool {
    check_long_entry(input) || check_short_entry(input)
}
