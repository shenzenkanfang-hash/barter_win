#![forbid(unsafe_code)]

pub mod channel;
pub mod check_table;
pub mod engine;
pub mod error;
pub mod market_status;
pub mod mode;
pub mod order;
pub mod pipeline_form;
pub mod pnl_manager;
pub mod position_exclusion;
pub mod risk;
pub mod risk_rechecker;
pub mod round_guard;
pub mod symbol_rules;
pub mod thresholds;

pub use channel::{ChannelType, VolatilityChannel};
pub use check_table::{CheckEntry, CheckTable};
pub use engine::TradingEngine;
pub use error::EngineError;
pub use market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection};
pub use mode::ModeSwitcher;
pub use order::OrderExecutor;
pub use pipeline_form::PipelineForm;
pub use pnl_manager::PnlManager;
pub use position_exclusion::{PositionDirection, PositionExclusionChecker, PositionInfo};
pub use risk::{RiskPreChecker, VolatilityMode};
pub use risk_rechecker::RiskReChecker;
pub use round_guard::{RoundGuard, RoundGuardScope};
pub use symbol_rules::SymbolRules;
pub use thresholds::ThresholdConstants;
