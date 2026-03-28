//! History Data Manager - 简化版
//!
//! 与 b_data_source::history 对齐，使用简化内存存储

pub mod types;
pub mod provider;
pub mod manager;

pub use manager::{HistoryDataManager, MAX_KLINE_ENTRIES};
pub use provider::HistoryDataProvider;
pub use types::{
    KLine, HistoryRequest, HistoryResponse, DataSource,
    DataIssue, HistoryError, KlineMetadata,
};
