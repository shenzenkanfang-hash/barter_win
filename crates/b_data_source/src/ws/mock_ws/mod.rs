//! mock_ws - 模拟WebSocket数据注入
//!
//! 用历史K线数据替代真实Binance WebSocket数据

pub mod noise;
pub mod tick_generator;

pub use tick_generator::{StreamTickGenerator, SimulatedTick};
