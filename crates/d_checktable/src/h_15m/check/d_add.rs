//! 加仓/对冲检查
//!
//! ```text
//! Add/Hedge Check Flow
//! ──────────────────────────────────────────────
//!   MinSignalInput
//!       |
//!       v
//!   MinSignalGenerator::generate(input, vol_level)
//!       |
//!       v
//!   MinSignalOutput { long_hedge, short_hedge, ... }
//!       |
//!       +---> output.long_hedge  (tr<15% AND dev<0 AND 6cond>=4)
//!       +---> output.short_hedge (tr<15% AND dev>0 AND 6cond>=4)
//!       |
//!       v
//!   output.long_hedge OR output.short_hedge
//!       |
//!       v
//!   CheckSignal::Add
//! ──────────────────────────────────────────────
//!
//! 触发条件：基础波动率<15% 且 价格偏离方向正确 且 6个条件>=4
//! - long_hedge: 回落对冲，tr_base_60min<15% AND price_deviation<0 AND 6cond>=4
//! - short_hedge: 回升对冲，tr_base_60min<15% AND price_deviation>0 AND 6cond>=4

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
