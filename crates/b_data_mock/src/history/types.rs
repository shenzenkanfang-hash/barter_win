//! History Data Types
//!
//! 复制自 b_data_source::history::types

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// K线数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KLine {
    pub symbol: String,
    pub period: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp_ms: i64,
}

/// 历史数据请求
#[derive(Debug, Clone)]
pub struct HistoryRequest {
    pub symbol: String,
    pub period: String,
    pub end_time: DateTime<Utc>,
    pub limit: u32,
}

/// 数据来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Memory,
    Disk,
    Remote,
}

/// 历史数据响应
#[derive(Debug, Clone)]
pub struct HistoryResponse {
    pub klines: Vec<KLine>,
    pub current: Option<KLine>,
    pub has_more: bool,
    pub source: DataSource,
}

/// 数据异常类型
#[derive(Debug, Clone)]
pub enum DataIssue {
    MissingData { from: i64, to: i64 },
    BrokenSequence { last_timestamp: i64 },
    InvalidData { timestamp: i64 },
}

/// 历史数据管理错误
#[derive(Debug, Clone, Error)]
pub enum HistoryError {
    #[error("品种不存在: {0}")]
    SymbolNotFound(String),
    #[error("数据不足: 需要 {need} 条，只有 {has} 条")]
    InsufficientData { has: u32, need: u32 },
    #[error("内部错误: {0}")]
    Internal(String),
}

/// K线元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineMetadata {
    pub last_update: DateTime<Utc>,
    pub count: usize,
    pub oldest_timestamp_ms: i64,
    pub newest_timestamp_ms: i64,
}

impl Default for KlineMetadata {
    fn default() -> Self {
        Self {
            last_update: Utc::now(),
            count: 0,
            oldest_timestamp_ms: 0,
            newest_timestamp_ms: 0,
        }
    }
}

impl KlineMetadata {
    pub fn new(count: usize, oldest: i64, newest: i64) -> Self {
        Self {
            last_update: Utc::now(),
            count,
            oldest_timestamp_ms: oldest,
            newest_timestamp_ms: newest,
        }
    }
}
