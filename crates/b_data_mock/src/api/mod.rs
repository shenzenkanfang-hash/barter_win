//! API 数据接口层
//!
//! 与 b_data_source::api 对齐，但使用模拟数据源

pub mod futures_account;  // 重命名：原 account.rs
pub mod position;
pub mod symbol_registry;
pub mod trade_settings;
pub mod data_feeder;
pub mod mock_account;
pub mod mock_gateway;
pub mod mock_config;
pub mod events;  // 新增：账户事件
pub mod account; // 账户状态管理（包含 Balance, Position, AccountState）

pub use futures_account::{FuturesAccount, FuturesAccountData};
pub use position::{FuturesPosition, FuturesPositionData};
pub use symbol_registry::SymbolRegistry;
pub use trade_settings::{TradeSettings, PositionMode};
pub use data_feeder::DataFeeder;
pub use mock_account::{Account, Side};
pub use mock_gateway::MockApiGateway;
pub use mock_config::{MockConfig, MockExecutionConfig};
pub use account::state::{Balance, Position, AccountState, MockAccountError};
pub use events::AccountEvent;
pub use a_common::api::SymbolRulesFetcher;
