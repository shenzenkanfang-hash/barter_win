//! mock_api - 模拟API网关
//!
//! 用模拟账户替代真实Binance API

pub mod config;
pub mod account;
pub mod order_engine;
pub mod risk_checker;
pub mod gateway;

pub use config::MockConfig;
pub use account::{Account, Position, Side};
pub use order_engine::{OrderEngine, OrderRequest};
pub use risk_checker::{MockRiskChecker, RiskMode, RiskCheckResult};
pub use gateway::MockApiGateway;
