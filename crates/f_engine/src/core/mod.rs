#![forbid(unsafe_code)]

pub mod engine;
pub mod pipeline;
pub mod strategy_pool;

pub use engine::TradingEngine;
pub use pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor};
pub use c_data_process::PipelineForm;
pub use strategy_pool::{StrategyAllocation, StrategyPool};
