#![forbid(unsafe_code)]

pub mod config;
pub mod simulator;
pub mod gateway;
pub mod tick_generator;
pub mod perf_test;
pub mod backtest;

pub use config::ShadowConfig;
pub use simulator::{Account, OrderEngine, Position, Side, ShadowRiskChecker};
pub use gateway::ShadowBinanceGateway;
pub use tick_generator::{TickGenerator, TickDriver, SimulatedTick, KLineInput};
pub use perf_test::{
    PerfTestConfig, PerfTestResult, PerformanceTracker,
    TickDriver as PerfTickDriver, EngineDriver, Reporter,
};
pub use backtest::{BacktestStrategy, BacktestTick, MaCrossStrategy, Signal};
