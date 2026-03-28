//! API 数据接口层
//!
//! 与 b_data_source::api 对齐，但使用模拟数据源

pub mod account;
pub mod position;
pub mod data_sync;
pub mod symbol_registry;
pub mod trade_settings;
pub mod data_feeder;
pub mod mock_account;
pub mod mock_gateway;
pub mod mock_config;

pub use account::{FuturesAccount, FuturesAccountData};
pub use position::{FuturesPosition, FuturesPositionData};
pub use data_sync::{FuturesDataSyncer, FuturesSyncResult};
pub use symbol_registry::SymbolRegistry;
pub use trade_settings::{TradeSettings, PositionMode};
pub use data_feeder::DataFeeder;
pub use mock_account::Account;
pub use mock_gateway::MockApiGateway;
pub use mock_config::MockConfig;
pub use a_common::api::SymbolRulesFetcher;
