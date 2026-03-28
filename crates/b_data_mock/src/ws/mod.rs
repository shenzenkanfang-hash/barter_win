//! 模拟 WebSocket 数据模块
//!
//! 与 b_data_source::ws 对齐的模拟实现

mod tick_generator;
mod noise;

pub use tick_generator::*;
pub use noise::*;
