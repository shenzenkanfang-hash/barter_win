//! 开仓检查
//!
//! ```text
//! Open Check Flow
//! ──────────────────────────────────────────────
//!   MinSignalInput
//!       |
//!       v
//!   MinSignalGenerator::generate(input, vol_level)
//!       |
//!       v
//!   MinSignalOutput { long_entry, short_entry, ... }
//!       |
//!       +---> output.long_entry  (tr>15% AND dev<0 AND pin>=4)
//!       +---> output.short_entry (tr>15% AND dev>0 AND pin>=4)
//!       |
//!       v
//!   output.long_entry OR output.short_entry
//!       |
//!       v
//!   CheckSignal::Open
//! ──────────────────────────────────────────────
//!
//! 触发条件：基础波动率>15% 且 价格偏离方向正确 且 pin条件>=4
//! - long_entry: tr_base_60min>15% AND price_deviation<0 AND pin>=4
//! - short_entry: tr_base_60min>15% AND price_deviation>0 AND pin>=4

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
