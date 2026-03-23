//! 加仓/对冲检查
//!
//! ```text
//! Add/Hedge Check Flow (日线)
//! ──────────────────────────────────────────────
//!   DaySignalInput
//!       |
//!       v
//!   DaySignalGenerator::generate(input, vol_level)
//!       |
//!       v
//!   DaySignalOutput { long_hedge, short_hedge, ... }
//!       |
//!       +---> output.long_hedge  (max_bg=="淡绿" AND ma5_pos>50)
//!       +---> output.short_hedge (max_bg=="淡红" AND ma5_pos<50)
//!       |
//!       v
//!   output.long_hedge OR output.short_hedge
//!       |
//!       v
//!   CheckSignal::Add
//! ──────────────────────────────────────────────
//!
//! 触发条件：
//! - long_hedge (回落对冲): 最大周期背景=淡绿 且 ma5_pos>50
//! - short_hedge (回升对冲): 最大周期背景=淡红 且 ma5_pos<50

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
