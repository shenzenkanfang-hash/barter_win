//! historical_replay - 历史数据回测模块
//!
//! 流式读取 Parquet 历史 K线数据，生成仿真 Tick，注入内存驱动引擎。
//!
//! ## 模块结构
//!
//! ```text
//! historical_replay/
//! ├── mod.rs              - 模块入口
//! ├── kline_loader.rs     - Parquet K线加载器（流式）
//! ├── tick_generator.rs   - 仿真 Tick 生成器（流式）
//! ├── noise.rs            - 高斯噪声模块
//! ├── memory_injector.rs  - 内存写入适配器
//! └── replay_controller.rs - 流式控制器
//! ```
//!
//! ## 数据格式
//!
//! 输入 Parquet（Python pandas 输出）:
//! ```text
//! timestamp, open, high, low, close, volume
//! 1700000060000, 50000.0, 50200.0, 49800.0, 50100.0, 100.5
//! ```
//!
//! 字段说明:
//! - timestamp: 毫秒时间戳（父级 1m K线时间戳）
//! - open/high/low/close: OHLC 价格
//! - volume: 成交量

#![forbid(unsafe_code)]

pub mod kline_loader;
pub mod tick_generator;
pub mod noise;
pub mod memory_injector;
pub mod replay_controller;

pub use kline_loader::{KlineLoader, KlineLoadError, ParquetInfo};
pub use tick_generator::{StreamTickGenerator, SimulatedTick};
pub use memory_injector::{MemoryInjector, MemoryInjectorConfig, SharedMarketData};
pub use replay_controller::{ReplayController, ReplayConfig, ReplayState, ReplayStats, ReplayError};
