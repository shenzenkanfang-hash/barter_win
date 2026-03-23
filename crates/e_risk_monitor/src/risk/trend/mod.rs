#![forbid(unsafe_code)]

pub mod trend_risk_limit;

pub use trend_risk_limit::{TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit};
