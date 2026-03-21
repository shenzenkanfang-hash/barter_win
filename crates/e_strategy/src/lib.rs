#![forbid(unsafe_code)]

pub mod strategy;
pub mod core;
pub mod order;
pub mod channel;
pub mod symbol;

// Re-exports
pub use strategy::{pin_strategy::PinStrategy, trend_strategy::TrendStrategy, traits::Strategy, types::{OrderRequest, Side, Signal}};
pub use core::{engine::TradingEngine, pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor}, pipeline_form::PipelineForm, strategy_pool::{StrategyAllocation, StrategyPool}};
pub use order::{gateway::ExchangeGateway, order::OrderExecutor};
pub use channel::{channel::{ChannelCheckpointCallback, ChannelType, VolatilityChannel}, mode::ModeSwitcher};
pub use symbol::{symbol_rules::SymbolRules, symbol_rules_fetcher::SymbolRulesFetcher};
