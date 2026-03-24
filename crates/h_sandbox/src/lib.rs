#![forbid(unsafe_code)]

// pub mod mock_binance_gateway; // TODO: 修复依赖问题
pub mod shadow_config;
pub mod shadow_account;
pub mod shadow_gateway;

pub use shadow_config::ShadowConfig;
pub use shadow_account::{ShadowAccount, Side};
pub use shadow_gateway::ShadowBinanceGateway;
