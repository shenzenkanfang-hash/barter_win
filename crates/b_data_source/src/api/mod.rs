//! API 数据接口层
//!
//! 封装所有 REST API 数据获取接口
//! 其他模块只能通过这里获取 API 数据

pub mod account;
pub mod position;
pub mod data_sync;
pub mod symbol_registry;
pub mod trade_settings;
pub mod data_feeder;

pub use account::{FuturesAccount, FuturesAccountData};
pub use position::{FuturesPosition, FuturesPositionData};
pub use data_sync::{FuturesDataSyncer, FuturesSyncResult};
pub use symbol_registry::SymbolRegistry;
pub use data_feeder::DataFeeder;
pub use a_common::api::SymbolRulesFetcher;
