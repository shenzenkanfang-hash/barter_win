#![forbid(unsafe_code)]

pub mod account_pool;
pub mod margin_config;
pub mod market_status;
pub mod pnl_manager;
pub mod round_guard;

pub use account_pool::{AccountInfo, AccountMargin, AccountPool, CircuitBreakerState};
pub use margin_config::{GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel};
pub use market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection};
pub use pnl_manager::PnlManager;
pub use round_guard::{RoundGuard, RoundGuardScope};
