//! 数据模型模块
//!
//! 包含市场数据业务类型:
//! - Tick: 市场数据
//! - KLine: K线数据
//! - Period: 周期类型
//! - MarketStream: 市场流 trait

pub mod types;
pub mod ws;

pub use types::{KLine, Period, Tick};
pub use ws::{MarketStream, MockMarketStream};
