//! h_15m - 分钟级策略模块
//!
//! 职责：
//! - 双通道信号生成（高速/低速）
//! - 7条件Pin模式
//! - 数量计算
//! - 状态机管理
//! - 品种交易主循环
//!
//! ```text
//! 目录结构：
//! h_15m/
//! ├── mod.rs              入口 + 双通道分发 + 数量计算
//! ├── signal.rs           7条件Pin模式 + 双通道信号生成
//! ├── status.rs           PinStatus状态机
//! ├── quantity_calculator.rs 数量计算
//! └── trader.rs          品种交易主循环
//! ```

#![forbid(unsafe_code)]

pub mod signal;
pub mod status;
pub mod quantity_calculator;
pub mod trader;
pub mod executor;
pub mod repository;

pub use signal::MinSignalGenerator;
pub use status::{PinStatus, PinStatusMachine};
pub use quantity_calculator::{MinQuantityCalculator, MinQuantityConfig};
pub use trader::{Trader, TraderConfig, TraderHealth};
pub use executor::{Executor, ExecutorConfig, ExecutorError, OrderType};
pub use repository::{Repository, TradeRecord, RecordStatus, RepoError, PENDING_TIMEOUT_SECS};

use crate::types::{CheckChainContext, MinSignalInput, VolatilityTier};
use x_data::trading::signal::{PositionSide, StrategySignal};

/// 确定波动率通道
fn determine_volatility_tier(tr_ratio_15min: rust_decimal::Decimal) -> VolatilityTier {
    use a_common::config::VOLATILITY_CONFIG;
    let config = &*VOLATILITY_CONFIG;
    if tr_ratio_15min >= config.high_vol_15m {
        VolatilityTier::High
    } else if tr_ratio_15min >= config.high_vol_1m {
        VolatilityTier::Medium
    } else {
        VolatilityTier::Low
    }
}

/// 分钟级策略入口
///
/// 双通道分发：
/// - VolatilityTier::High → 高速通道
/// - VolatilityTier::Low/Medium → 低速通道（参考日线方向）
///
/// 返回 Option<StrategySignal>：成功返回信号，None表示无信号
pub fn run_check_chain(
    _symbol: &str,
    input: &MinSignalInput,
    day_direction: Option<PositionSide>,
    ctx: &CheckChainContext,
) -> Option<StrategySignal> {
    // 1. 判断波动率通道
    let vol_tier = determine_volatility_tier(input.tr_ratio_15min);

    // 2. 生成信号（双通道自动选择）
    let signal_generator = MinSignalGenerator::new();
    let signal_output = signal_generator.generate(input, &vol_tier, day_direction);

    // 3. 数量计算
    let calculator = MinQuantityCalculator::with_default();
    calculator.generate_signal(
        input,
        &signal_output,
        ctx.current_position_qty,
        &vol_tier,
        ctx.strategy_id.clone(),
        ctx.position_ref.clone(),
    )
}
