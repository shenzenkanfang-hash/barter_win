//! historical_replay - 历史数据回测模块
//!
//! 流式读取历史 K线数据，生成仿真 Tick，注入内存驱动引擎。
//!
//! ## 模块结构
//!
//! ```text
//! historical_replay/
//! ├── kline_loader.rs     - CSV K线加载器（已废弃，使用 ApiKlineFetcher）
//! ├── tick_generator.rs   - 仿真 Tick 生成器（流式）
//! ├── noise.rs            - 高斯噪声模块
//! ├── memory_injector.rs  - 内存写入适配器
//! ├── replay_controller.rs - 流式控制器
//! └── tick_to_ws.rs      - Tick → WS K线格式转换
//! ```
//!
//! ## 数据流
//!
//! ```text
//! ApiKlineFetcher (币安 API 直连)
//!         ↓
//! StreamTickGenerator (60 ticks/K线)
//!         ↓
//! TickToWsConverter (→ BinanceKlineMsg)
//!         ↓
//! DataFeeder / ShadowBinanceGateway
//! ```

#![forbid(unsafe_code)]

pub mod tick_generator;
pub mod noise;
pub mod memory_injector;
pub mod replay_controller;
pub mod tick_to_ws;

pub use tick_generator::{StreamTickGenerator, SimulatedTick};
pub use memory_injector::{MemoryInjector, MemoryInjectorConfig, SharedMarketData};
pub use replay_controller::{ReplayController, ReplayConfig, ReplayState, ReplayStats, ReplayError};
pub use tick_to_ws::TickToWsConverter;
