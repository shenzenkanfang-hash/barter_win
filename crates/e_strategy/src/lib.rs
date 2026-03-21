#![forbid(unsafe_code)]

pub mod strategy;
pub mod core;
pub mod order;
pub mod channel;
pub mod symbol;
pub mod shared;

// Re-exports
pub use strategy::{pin_strategy::PinStrategy, trend_strategy::TrendStrategy, traits::Strategy, types::{OrderRequest, Side, Signal}};
pub use core::{engine::TradingEngine, pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor}, pipeline_form::PipelineForm, strategy_pool::{StrategyAllocation, StrategyPool}};
pub use order::{gateway::ExchangeGateway, mock_binance_gateway::{MockBinanceGateway, MockAccount, MockPosition, MockOrder, MockTrade, ChannelState, GatewayChannelType, ExitSignal, OrderResult, OrderStatus, PineColorState, RejectReason, RiskConfig, SignalSynthesisLayer, TriggerLogEntry}, order::OrderExecutor};
pub use channel::{channel::{ChannelCheckpointCallback, ChannelType, VolatilityChannel}, mode::ModeSwitcher};
pub use shared::{check_table::{CheckEntry, CheckTable}};
pub use symbol::{symbol_rules::SymbolRules, symbol_rules_fetcher::SymbolRulesFetcher};
