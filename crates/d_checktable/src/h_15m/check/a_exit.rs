//! 退出检查
//!
//! ```text
//! Exit Check Flow
//! ──────────────────────────────────────────────
//!   MinSignalInput
//!       |
//!       v
//!   MinSignalGenerator::generate(input, vol_level)
//!       |
//!       v
//!   MinSignalOutput { long_exit, short_exit, ... }
//!       |
//!       +---> output.long_exit  (pin>=4 AND pos_norm_60>80)
//!       +---> output.short_exit (pin>=4 AND pos_norm_60<20)
//!       |
//!       v
//!   output.long_exit OR output.short_exit
//!       |
//!       v
//!   CheckSignal::Exit
//! ──────────────────────────────────────────────
//!
//! 触发条件：pin条件>=4 且 价格位置到周期极点
//! - long_exit: pos_norm_60 > 80 (多头持仓，到达周期高点)
//! - short_exit: pos_norm_60 < 20 (空头持仓，到达周期低点)

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
