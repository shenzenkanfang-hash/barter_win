#![forbid(unsafe_code)]

pub mod common;
pub mod minute_risk;
pub mod trend;

// Re-exports from common
pub use common::{
    RiskPreChecker, RiskReChecker, OrderCheck, OrderCheckResult, OrderReservation,
    ThresholdConstants, VolatilityMode,
};

// Re-exports from minute_risk
pub use minute_risk::{
    calculate_hour_open_notional, calculate_minute_open_notional,
    calculate_open_qty_from_notional, MinuteOpenResult,
};

// Re-exports from trend
pub use trend::{TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit};
