#![forbid(unsafe_code)]
#![allow(dead_code)]

//! b_data_mock - 模拟数据层
//!
//! 与 b_data_source 对齐的模拟实现，用于沙盒测试。
//!
//! # 与正式接口对齐
//!
//! | 正式接口 (b_data_source) | 模拟接口 (b_data_mock) |
//! |--------------------------|------------------------|
//! | WebSocket K线            | StreamTickGenerator    |
//! | FuturesDataSyncer        | MockApiGateway         |
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use b_data_mock::{MockApiGateway, StreamTickGenerator};
//!
//! // 创建模拟网关
//! let gateway = MockApiGateway::with_default_config(dec!(100000));
//! gateway.update_price("BTCUSDT", dec!(50000));
//!
//! // 创建 Tick 生成器
//! let klines = vec![...];
//! let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(klines.into_iter()));
//! ```

pub mod ws;
pub mod api;

// Re-export 主要类型
pub use ws::StreamTickGenerator;
pub use ws::SimulatedTick;
pub use api::MockApiGateway;
pub use api::MockConfig;
