//! Tick Generator - 从 1m K 线生成模拟 Tick 数据
//!
//! 移植自 Python historical_retracement.py 的 TickGenerator
//! 1m K线 → 60个 tick（价格路径 + 累积 OHLCV）

mod generator;
mod driver;

pub use generator::{TickGenerator, SimulatedTick, KLineInput, TICKS_PER_1M};
pub use driver::TickDriver;
