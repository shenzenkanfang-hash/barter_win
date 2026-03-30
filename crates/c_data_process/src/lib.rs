#![forbid(unsafe_code)]

pub mod types;

pub mod pine_indicator_full;

pub mod min;
pub mod day;
pub mod processor;

pub mod strategy_state;

pub mod indicator_store;

pub use pine_indicator_full::{PineColorDetector as PineColorDetectorV5, colors, EMA, RSI};
pub use types::*;
pub use processor::SignalProcessor;

// Re-export strategy_state types
pub use strategy_state::{
    StrategyStateManager, StrategyStateDb, StrategyState, PositionState, PositionSide,
    PnlState, TradingStats, RiskState, StrategyParams, TradeRecord, DailyPnl,
    StrategyStateError, Result as StrategyStateResult,
};

// Re-export indicator_store types
pub use indicator_store::{
    IndicatorStore, Indicator1mOutput, Indicator1dOutput, SignalProcessorIndicatorStore,
};
