//! 检查链入口
//!
//! ```text
//! Check Chain Flow (优先级从左到右，第一个信号胜出)
//!
//!   run_check_chain(symbol, input, ctx)
//!           |
//!     +-----+-----+-----+-----+-----+
//!     |     |     |     |     |     |
//!     v     v     v     v     v     v
//!   a_exit b_close c_hedge d_add e_open
//!     |     |      |      |     |
//!     v     v      v      v     v
//!   Exit  Close  Hedge  Add   Open
//!   (最高)                (最低)
//!     |           |        |
//!     +-----+-----+--------+
//!           |
//!           v
//!     StrategySignal
//!           |
//!           v
//!     TradingEngine
//! ```
//!
//! 注意：检查函数为 CPU 密集型纯函数，顺序执行比并发更高效（避免线程调度开销）。
//! 信号优先级：Exit > Close > Hedge > Add > Open（由 Vec 中的顺序决定）。

use rust_decimal::Decimal;
use crate::h_15m::quantity_calculator::MinQuantityCalculator;
use crate::h_15m::signal_generator::MinSignalGenerator;
use crate::h_15m::market_status_generator::MinMarketStatusGenerator;
use crate::types::MinSignalInput;
use x_data::trading::signal::{StrategySignal, StrategyId, PositionRef};

/// 检查链上下文（双周期通用）
#[derive(Debug, Clone)]
pub struct CheckChainContext {
    /// 当前持仓数量
    pub current_position_qty: Decimal,
    /// 策略标识
    pub strategy_id: StrategyId,
    /// 仓位引用（加仓/平仓时必须）
    pub position_ref: Option<PositionRef>,
}

/// 检查信号枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSignal {
    Exit,   // 退出信号
    Close,  // 关仓信号
    Hedge,  // 对冲信号
    Add,    // 加仓信号
    Open,   // 开仓信号
}

/// 检查链结果
#[derive(Debug, Clone, Default)]
pub struct CheckChainResult {
    pub signals: Vec<CheckSignal>,
}

impl CheckChainResult {
    pub fn new() -> Self {
        Self { signals: Vec::new() }
    }

    /// 添加信号
    pub fn add_signal(&mut self, signal: CheckSignal) {
        self.signals.push(signal);
    }

    /// 检查是否有特定信号
    pub fn has(&self, signal: CheckSignal) -> bool {
        self.signals.contains(&signal)
    }

    /// 是否有任何信号
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

/// 执行完整检查链（接收指标输入和上下文）
///
/// 返回 Option<StrategySignal>：成功返回信号，None表示无信号
pub fn run_check_chain(
    symbol: &str,
    input: &MinSignalInput,
    ctx: &CheckChainContext,
) -> Option<StrategySignal> {
    // 1. 生成信号输出（纯指标判断）
    let signal_generator = MinSignalGenerator::new();
    let market_status_gen = MinMarketStatusGenerator::new();

    // 2. 确定波动率等级（使用 tr_ratio_15min）
    let vol_tier = market_status_gen.determine_volatility_level(input.tr_ratio_15min);

    // 3. 生成信号
    let signal_output = signal_generator.generate(input, &vol_tier);

    // 4. 计算器生成完整策略信号
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
