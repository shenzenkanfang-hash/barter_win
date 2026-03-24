#![forbid(unsafe_code)]
#![allow(dead_code)]

pub mod channel;
pub mod core;
pub mod order;
pub mod strategy;
pub mod types;

// Re-exports - Strategy
pub use strategy::{
    Direction, MarketStatus, MarketStatusType, SignalType, SignalAggregator, Strategy, 
    StrategyExecutor, StrategyFactory, StrategyKLine, StrategyState, StrategyStatus, 
    TradingSignal, VolatilityLevel,
};
