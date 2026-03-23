//! 开仓检查
//!
//! ```text
//! Open Check Flow (日线)
//! ──────────────────────────────────────────────
//!   DaySignalInput
//!       |
//!       v
//!   DaySignalGenerator::generate(input, vol_level)
//!       |
//!       v
//!   DaySignalOutput { long_entry, short_entry, ... }
//!       |
//!       +---> output.long_entry  (all_green AND vol AND ma5_pos>70)
//!       +---> output.short_entry (all_red_purple AND vol AND ma5_pos<30)
//!       |
//!       v
//!   output.long_entry OR output.short_entry
//!       |
//!       v
//!   CheckSignal::Open
//! ──────────────────────────────────────────────
//!
//! 触发条件：
//! - long_entry: 全部3组 Pine 颜色=纯绿 AND (tr>1 OR tr>1) AND ma5_pos>70
//! - short_entry: 全部3组 Pine 颜色=纯红/紫色 AND (tr>1 OR tr>1) AND ma5_pos<30

use crate::types::DaySignalInput;
use crate::l_1d::signal_generator::DaySignalGenerator;
use crate::l_1d::market_status_generator::DayMarketStatusGenerator;

/// 检查做多开仓
pub fn check_long_entry(input: &DaySignalInput) -> bool {
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level_from_signal(input));

    output.long_entry
}

/// 检查做空开仓
pub fn check_short_entry(input: &DaySignalInput) -> bool {
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let output = generator.generate(input, &status_gen.determine_volatility_level_from_signal(input));

    output.short_entry
}

/// 主检查入口
pub fn check(input: &DaySignalInput) -> bool {
    check_long_entry(input) || check_short_entry(input)
}
