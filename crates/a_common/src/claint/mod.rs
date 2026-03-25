//! 通用错误类型模块
//!
//! 包含:
//! - EngineError: 引擎错误 (风控、订单、模式切换等)
//! - MarketError: 市场数据错误 (WebSocket、序列化、K线等)

pub mod error;

pub use error::{EngineError, MarketError, AppError};
