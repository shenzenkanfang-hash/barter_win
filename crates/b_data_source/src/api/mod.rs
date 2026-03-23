//! API 数据接口层
//!
//! 封装所有 REST API 数据获取接口
//! 其他模块只能通过这里获取 API 数据

pub mod account;
pub mod position;
pub mod data_sync;

pub use account::{FuturesAccount, FuturesAccountData};
pub use position::{FuturesPosition, FuturesPositionData};
pub use data_sync::{FuturesDataSyncer, FuturesSyncResult};

pub use a_common::api::SymbolRulesFetcher;
