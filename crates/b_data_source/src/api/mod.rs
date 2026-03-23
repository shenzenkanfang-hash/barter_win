//! API 数据接口层
//!
//! 封装所有 REST API 数据获取接口
//! 其他模块只能通过这里获取 API 数据，不能直接访问 a_common::api

pub use crate::futures::{FuturesAccount, FuturesAccountData, FuturesPosition, FuturesPositionData};
pub use crate::futures::{FuturesDataSyncer, FuturesSyncResult};
