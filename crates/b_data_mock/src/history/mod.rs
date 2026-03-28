//! History Data Manager - 简化版
//!
//! 复制自 b_data_source::history

pub mod types;

pub use types::{
    KLine, HistoryRequest, HistoryResponse, DataSource,
    DataIssue, HistoryError, KlineMetadata,
};
