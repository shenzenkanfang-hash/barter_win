#![forbid(unsafe_code)]

pub mod account_pool;
pub mod channel;
pub mod check_table;
pub mod engine;
pub mod error;
pub mod market_status;
pub mod mock_binance_gateway;
pub mod mode;
pub mod order;
pub mod order_check;
pub mod persistence;
pub mod pipeline_form;
pub mod pnl_manager;
pub mod position_exclusion;
pub mod position_manager;
pub mod risk;
pub mod risk_rechecker;
pub mod round_guard;
pub mod sqlite_persistence;
pub mod strategy_pool;
pub mod symbol_rules;
pub mod thresholds;

pub use account_pool::{AccountInfo, AccountPool, CircuitBreakerState};
pub use channel::{ChannelType, VolatilityChannel};
pub use check_table::{CheckEntry, CheckTable};
pub use engine::TradingEngine;
pub use error::EngineError;
pub use market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection};
pub use mock_binance_gateway::{
    ChannelState, GatewayChannelType as MockChannelType, ExitSignal, MockAccount, MockBinanceGateway,
    MockOrder, MockPosition, MockTrade, OrderResult, OrderStatus, PineColorState, RejectReason,
    RiskConfig, SignalSynthesisLayer, TriggerLogEntry,
};
pub use mode::ModeSwitcher;
pub use order::OrderExecutor;
pub use order_check::{OrderCheck, OrderCheckResult, OrderReservation};
pub use persistence::{KLineCache, KLineData, PersistenceConfig, PersistenceService, PersistenceStats, PositionSnapshot, TradeRecord};
pub use pipeline_form::PipelineForm;
pub use pnl_manager::PnlManager;
pub use position_exclusion::{PositionDirection, PositionExclusionChecker, PositionInfo};
pub use position_manager::{Direction, LocalPosition, LocalPositionManager, PositionStats};
pub use risk::{RiskPreChecker, VolatilityMode};
pub use risk_rechecker::RiskReChecker;
pub use round_guard::{RoundGuard, RoundGuardScope};
pub use sqlite_persistence::{
    AccountSnapshotRecord, ChannelEventRecord, EventRecorder, ExchangePositionRecord,
    IndicatorCsvWriter, IndicatorComparisonRow, IndicatorEventRecord,
    LocalPositionRecord, NoOpEventRecorder, RiskEventRecord, SqliteEventRecorder,
    SqliteRecordService, format_decimal,
};
pub use strategy_pool::{StrategyAllocation, StrategyPool};
pub use symbol_rules::SymbolRules;
pub use thresholds::ThresholdConstants;
