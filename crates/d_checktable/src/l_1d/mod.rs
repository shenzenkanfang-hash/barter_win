//! l_1d - 日线级策略模块
//!
//! 职责：
//! - 3组Pine颜色模式
//! - 数量计算
//! - 状态机管理
//!
//! ```text
//! 目录结构（简化后）：
//! l_1d/
//! ├── mod.rs              入口 + 流程
//! ├── signal.rs           3组Pine颜色模式
//! ├── status.rs           TrendStatus状态机
//! └── quantity_calculator.rs 数量计算
//! ```

#![forbid(unsafe_code)]

pub mod signal;
pub mod status;
pub mod quantity_calculator;

pub use signal::DaySignalGenerator;
pub use status::{TrendStatus, TrendStatusMachine};
pub use quantity_calculator::{DayQuantityCalculator, DayQuantityConfig};

use crate::types::{CheckChainContext, DaySignalInput, VolatilityTier};
use x_data::trading::signal::StrategySignal;

/// 日线级策略入口
///
/// 返回 Option<StrategySignal>：成功返回信号，None表示无信号
pub fn run_check_chain(
    _symbol: &str,
    input: &DaySignalInput,
    ctx: &CheckChainContext,
) -> Option<StrategySignal> {
    // 1. 确定波动率等级（使用日线TR）
    let vol_tier = determine_volatility_tier(input);

    // 2. 生成信号
    let signal_generator = DaySignalGenerator::new();
    let signal_output = signal_generator.generate(input, &vol_tier);

    // 3. 数量计算
    let calculator = DayQuantityCalculator::with_default();
    calculator.generate_signal(
        input,
        &signal_output,
        ctx.current_position_qty,
        &vol_tier,
        ctx.strategy_id.clone(),
        ctx.position_ref.clone(),
    )
}

/// 确定日线波动率等级
fn determine_volatility_tier(input: &DaySignalInput) -> VolatilityTier {
    use rust_decimal_macros::dec;
    // 日线使用 tr_ratio_20d_60d 判断波动率
    if input.tr_ratio_20d_60d > dec!(1) {
        VolatilityTier::High
    } else if input.tr_ratio_5d_20d > dec!(1) {
        VolatilityTier::Medium
    } else {
        VolatilityTier::Low
    }
}
