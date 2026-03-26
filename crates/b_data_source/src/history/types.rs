//! History Data Types - 历史数据类型定义
//!
//! 包含错误类型、请求/响应结构、数据源枚举等

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// K线数据结构（兼容现有定义）
// ============================================================================

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
    /// K线开始时间戳（毫秒）
    pub timestamp_ms: i64,
}

impl KLine {
    /// 从WS原始数据创建
    pub fn from_ws_data(
        symbol: &str,
        period: &str,
        open: &str,
        high: &str,
        low: &str,
        close: &str,
        volume: &str,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.to_string(),
            period: period.to_string(),
            open: decimal_from_str(open),
            high: decimal_from_str(high),
            low: decimal_from_str(low),
            close: decimal_from_str(close),
            volume: decimal_from_str(volume),
            timestamp_ms,
        }
    }

    /// 获取时间戳（秒）
    pub fn timestamp_sec(&self) -> i64 {
        self.timestamp_ms / 1000
    }
}

/// 从字符串转换为Decimal，处理空字符串和无效值
fn decimal_from_str(s: &str) -> Decimal {
    if s.is_empty() || s == "0" {
        return Decimal::ZERO;
    }
    s.parse().unwrap_or(Decimal::ZERO)
}

// ============================================================================
// 请求/响应结构
// ============================================================================

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
    /// 历史闭合K线（按时间升序）
    pub klines: Vec<KLine>,
    /// 当前未闭合K线（可能为空）
    pub current: Option<KLine>,
    /// 是否还有更早的数据
    pub has_more: bool,
    /// 数据来源
    pub source: DataSource,
}

/// 数据异常类型
#[derive(Debug, Clone)]
pub enum DataIssue {
    /// 缺失数据
    MissingData { from: i64, to: i64 },
    /// 数据断裂
    BrokenSequence { last_timestamp: i64 },
    /// 无效数据
    InvalidData { timestamp: i64 },
}

// ============================================================================
// 错误类型
// ============================================================================

/// 历史数据管理错误
#[derive(Debug, Clone, Error)]
pub enum HistoryError {
    #[error("品种不存在: {0}")]
    SymbolNotFound(String),

    #[error("数据不足: 需要 {need} 条，只有 {has} 条")]
    InsufficientData { has: u32, need: u32 },

    #[error("数据断裂: {symbol} 从 {from} 到 {to}")]
    DataBroken {
        symbol: String,
        from: i64,
        to: i64,
    },

    #[error("重复数据: {symbol} 时间戳 {timestamp}")]
    DuplicateData { symbol: String, timestamp: i64 },

    #[error("磁盘写入失败: {0}")]
    DiskWriteFailed(String),

    #[error("API请求失败: {0}")]
    ApiRequestFailed(String),

    #[error("无效时间戳: {0}")]
    InvalidTimestamp(i64),

    #[error("品种不符合条件: {symbol} 上市 {days} 天，需要 {required} 天")]
    NotQualified {
        symbol: String,
        days: u32,
        required: u32,
    },

    #[error("数据异常: {0}")]
    DataIssue(String),

    #[error("缓存未找到: {0}")]
    CacheNotFound(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

// ============================================================================
// 磁盘文件格式
// ============================================================================

/// 磁盘存储格式: [[o,h,l,c,v,t], ...]
pub type DiskKLineFormat = Vec<serde_json::Value>;

/// 从磁盘格式转换为KLine
pub fn disk_to_kline(symbol: &str, period: &str, arr: &[serde_json::Value]) -> Option<KLine> {
    if arr.len() < 6 {
        return None;
    }
    Some(KLine {
        symbol: symbol.to_string(),
        period: period.to_string(),
        open: decimal_from_json(arr.get(0)?),
        high: decimal_from_json(arr.get(1)?),
        low: decimal_from_json(arr.get(2)?),
        close: decimal_from_json(arr.get(3)?),
        volume: decimal_from_json(arr.get(4)?),
        timestamp_ms: arr.get(5)?.as_i64()?,
    })
}

/// 从KLine转换为磁盘格式
pub fn kline_to_disk(kline: &KLine) -> serde_json::Value {
    serde_json::json!([
        kline.open.to_string(),
        kline.high.to_string(),
        kline.low.to_string(),
        kline.close.to_string(),
        kline.volume.to_string(),
        kline.timestamp_ms
    ])
}

/// 从Vec<KLine>转换为磁盘格式（用于批量存储）
pub fn klines_to_disk(klines: &[KLine]) -> Vec<Vec<serde_json::Value>> {
    klines.iter().map(|k| {
        let v = kline_to_disk(k);
        if let serde_json::Value::Array(arr) = v {
            arr
        } else {
            vec![v]
        }
    }).collect()
}

/// 从磁盘格式数组转换为Vec<KLine>
pub fn klines_from_disk(symbol: &str, period: &str, data: Vec<Vec<serde_json::Value>>) -> Vec<KLine> {
    data.into_iter()
        .filter_map(|arr| disk_to_kline(symbol, period, &arr))
        .collect()
}

fn decimal_from_json(v: &serde_json::Value) -> Decimal {
    if let Some(s) = v.as_str() {
        decimal_from_str(s)
    } else if let Some(n) = v.as_f64() {
        Decimal::from_f64_retain(n).unwrap_or(Decimal::ZERO)
    } else if let Some(n) = v.as_i64() {
        Decimal::from_i64(n).unwrap_or(Decimal::ZERO)
    } else {
        Decimal::ZERO
    }
}

// ============================================================================
// 元数据
// ============================================================================

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
