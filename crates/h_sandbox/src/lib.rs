//! Sandbox - 沙盒测试模块
//!
//! 保留功能:
//! 1. historical_replay - K线转Tick推WS内存
//! 2. gateway/interceptor - 拦截API下单/账户/持仓
//! 3. simulator - 完整交易流程(账户/订单/风控)

#![forbid(unsafe_code)]

pub mod config;
pub mod simulator;
pub mod gateway;
pub mod historical_replay;
// pub mod verifier;  // TODO: 修复编译错误后启用

pub use config::ShadowConfig;
pub use simulator::{Account, OrderEngine, Position, Side, ShadowRiskChecker};
pub use gateway::ShadowBinanceGateway;
pub use historical_replay::{
    StreamTickGenerator,
    MemoryInjector, MemoryInjectorConfig, SharedMarketData,
    ReplayController, ReplayConfig, ReplayState, ReplayStats, ReplayError,
    TickToWsConverter, ShardCache, ShardReader, ShardReaderChain, ShardWriter,
};
