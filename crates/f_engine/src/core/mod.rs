#![forbid(unsafe_code)]

pub mod engine;
pub mod pipeline;
pub mod strategy_pool;

pub use engine::TradingEngine;
pub use pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor};
pub use d_checktable::h_15m::pipeline_form::PipelineForm;
pub use strategy_pool::{StrategyAllocation, StrategyPool};
pub use crate::types::{StrategyId, ModeSwitcher, Mode, TradingDecision, OrderRequest, Side, OrderType};
