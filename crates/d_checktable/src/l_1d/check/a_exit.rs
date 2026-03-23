//! 退出检查
//!
//! ```text
//! Exit Check Flow (日线)
//! ──────────────────────────────────────────────
//!   DaySignalInput
//!       |
//!       v
//!   DaySignalGenerator::generate(input, vol_level)
//!       |
//!       v
//!   DaySignalOutput { long_exit, short_exit, ... }
//!       |
//!       +---> output.long_exit  (max_bg!="纯绿" AND ma5_pos>50)
//!       +---> output.short_exit (max_bg!="纯红" AND ma5_pos<50)
//!       |
//!       v
//!   output.long_exit OR output.short_exit
//!       |
//!       v
//!   CheckSignal::Exit
//! ──────────────────────────────────────────────
//!
//! 注意：日线级使用最大周期背景色判断 (100_200 > 20_50 > 12_26)
//! long_exit: 最大周期背景非纯绿 且 ma5 位置 > 50
//! short_exit: 最大周期背景非纯红 且 ma5 位置 < 50

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
