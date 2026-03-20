#![forbid(unsafe_code)]

pub mod channel;
pub mod check_table;
pub mod engine;
pub mod error;
pub mod mode;
pub mod order;
pub mod pipeline_form;
pub mod risk;
pub mod round_guard;
pub mod symbol_rules;

pub use channel::{ChannelType, VolatilityChannel};
pub use check_table::{CheckEntry, CheckTable};
pub use engine::TradingEngine;
pub use error::EngineError;
pub use mode::ModeSwitcher;
pub use order::OrderExecutor;
pub use pipeline_form::PipelineForm;
pub use risk::{RiskPreChecker, VolatilityMode};
pub use round_guard::{RoundGuard, RoundGuardScope};
pub use symbol_rules::SymbolRules;
