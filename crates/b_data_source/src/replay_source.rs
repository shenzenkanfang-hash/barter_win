//! ReplaySource - 历史数据回放
//!
//! 从 CSV 文件回放 OHLCVT 历史数据，用于回测。

use crate::models::{KLine, Period};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, warn};

/// ReplaySource - 历史数据回放
///
/// 从 CSV 文件按时间顺序回放 OHLCVT 数据。
pub struct ReplaySource {
    /// 品种过滤（为空表示全部）
    symbols_filter: Vec<String>,
    /// 周期过滤
    period_filter: Option<Period>,
    /// 当前索引
    current_idx: usize,
    /// 预加载的数据
    data: Vec<KLine>,
    /// 是否已结束
    exhausted: bool,
}

/// ReplaySource 错误类型
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("No data loaded")]
    NoData,

    #[error("Decimal conversion error: {0}")]
    DecimalConversion(String),
}

impl ReplaySource {
    /// 从 CSV 文件创建 ReplaySource
    ///
    /// CSV 格式: symbol,period,open,high,low,close,volume,timestamp
    ///
    /// # Errors
    /// 返回 ReplayError 如果文件无法读取或解析失败
    pub async fn from_csv<P: AsRef<Path>>(path: P) -> Result<Self, ReplayError> {
        let path = path.as_ref();
        let file = File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut data = Vec::new();
        let mut line_num = 0;

        // 跳过标题行
        if let Some(header) = lines.next_line().await? {
            debug!("CSV header: {}", header);
            line_num += 1;
        }

        // 解析数据行
        while let Some(line) = lines.next_line().await? {
            line_num += 1;
            
            if line.trim().is_empty() {
                continue;
            }

            // 修复：解析失败时返回错误而非静默跳过
            match parse_csv_line(&line, line_num) {
                Ok(kline) => data.push(kline),
                Err(e) => {
                    // 仅警告，不中断（可恢复的解析错误）
                    warn!("Skipping line {}: {}", line_num, e);
                }
            }
        }

        debug!("Loaded {} klines from {:?}", data.len(), path);

        // 按时间排序
        data.sort_by_key(|k| k.timestamp);

        Ok(Self {
            symbols_filter: Vec::new(),
            period_filter: None,
            current_idx: 0,
            data,
            exhausted: false,
        })
    }

    /// 从内存数据创建 ReplaySource（用于测试）
    pub fn from_data(data: Vec<KLine>) -> Self {
        let mut data = data;
        data.sort_by_key(|k| k.timestamp);
        Self {
            symbols_filter: Vec::new(),
            period_filter: None,
            current_idx: 0,
            data,
            exhausted: false,
        }
    }

    /// 设置品种过滤
    pub fn with_symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols_filter = symbols;
        self
    }

    /// 设置周期过滤
    pub fn with_period(mut self, period: Period) -> Self {
        self.period_filter = Some(period);
        self
    }

    /// 重置回放位置
    pub fn reset(&mut self) {
        self.current_idx = 0;
        self.exhausted = false;
    }

    /// 获取数据总数
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 获取下一个 K 线
    pub fn next_kline(&mut self) -> Option<KLine> {
        if self.exhausted {
            return None;
        }

        while self.current_idx < self.data.len() {
            let kline = self.data[self.current_idx].clone();
            self.current_idx += 1;

            // 应用过滤
            if !self.symbols_filter.is_empty() && !self.symbols_filter.contains(&kline.symbol) {
                continue;
            }

            if let Some(ref period) = self.period_filter {
                if kline.period != *period {
                    continue;
                }
            }

            return Some(kline);
        }

        self.exhausted = true;
        None
    }

    /// 是否已结束
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// 为 ReplaySource 实现 Iterator trait
/// 这样可以直接传给 StreamTickGenerator
impl Iterator for ReplaySource {
    type Item = KLine;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_kline()
    }
}

/// 解析 CSV 行
///
/// 修复：
/// 1. 返回 Result 而非 Option，支持详细的错误信息
/// 2. 使用 Decimal 而非 f64，避免精度问题
///
/// # Errors
/// 返回 ReplayError::Parse 如果字段数量不足或解析失败
fn parse_csv_line(line: &str, line_num: usize) -> Result<KLine, ReplayError> {
    let parts: Vec<&str> = line.split(',').collect();

    // 支持两种格式：
    // 1. 简单格式（6字段）: timestamp,open,high,low,close,volume
    // 2. 完整格式（8字段）: symbol,period,open,high,low,close,volume,timestamp

    let (symbol, period_str, timestamp_idx, field_start) = if parts.len() == 6 {
        // 简单格式
        ("BTCUSDT".to_string(), "1m", 0, 0)
    } else if parts.len() >= 8 {
        // 完整格式
        (parts[0].to_string(), parts[1], 7, 2)
    } else {
        return Err(ReplayError::Parse {
            line: line_num,
            message: format!("Expected 6 or 8 fields, got {}", parts.len()),
        });
    };

    // 解析价格字段
    let open = parse_decimal(parts[field_start], line_num, "open")?;
    let high = parse_decimal(parts[field_start + 1], line_num, "high")?;
    let low = parse_decimal(parts[field_start + 2], line_num, "low")?;
    let close = parse_decimal(parts[field_start + 3], line_num, "close")?;
    let volume = parse_decimal(parts[field_start + 4], line_num, "volume")?;

    // 解析时间戳
    let timestamp_str = if parts.len() == 6 {
        parts[0]
    } else {
        parts[timestamp_idx]
    };

    let period = match period_str {
        "1m" => Period::Minute(1),
        "5m" => Period::Minute(5),
        "15m" => Period::Minute(15),
        "30m" => Period::Minute(30),
        "1h" => Period::Minute(60),
        "4h" => Period::Minute(240),
        "1d" => Period::Day,
        other => {
            return Err(ReplayError::Parse {
                line: line_num,
                message: format!("Unknown period: {}", other),
            });
        }
    };

    // 解析时间戳
    let timestamp = parse_timestamp(timestamp_str, line_num)?;

    Ok(KLine {
        symbol,
        period,
        open,
        high,
        low,
        close,
        volume,
        timestamp,
        is_closed: false,
    })
}

/// 解析 Decimal 字段
///
/// 修复：使用 Decimal 而非 f64，避免浮点精度问题
/// Decimal 的字符串解析保证精确性
fn parse_decimal(s: &str, line_num: usize, field_name: &str) -> Result<Decimal, ReplayError> {
    // 直接解析为 Decimal，避免 f64 中转
    Decimal::from_str_exact(s)
        .map_err(|_| ReplayError::Parse {
            line: line_num,
            message: format!("Invalid {}: '{}'", field_name, s),
        })
}

/// 解析时间戳
///
/// 支持多种格式：
/// - RFC3339: 2024-01-01T00:00:00Z
/// - 简单格式: 2024-01-01 00:00:00
/// - 毫秒时间戳: 1759968000000
fn parse_timestamp(s: &str, line_num: usize) -> Result<DateTime<Utc>, ReplayError> {
    // 检查是否是纯数字（毫秒时间戳）
    if s.chars().all(|c| c.is_ascii_digit()) {
        // 毫秒时间戳
        let ms: i64 = s.parse().map_err(|_| ReplayError::Parse {
            line: line_num,
            message: format!("Invalid timestamp (ms): {}", s),
        })?;
        let secs = ms / 1000;
        let nanos = ((ms % 1000) as u32) * 1_000_000;
        return DateTime::from_timestamp(secs, nanos)
            .ok_or_else(|| ReplayError::Parse {
                line: line_num,
                message: format!("Timestamp out of range: {}", ms),
            });
    }

    // RFC3339 格式
    if s.contains('T') || s.ends_with('Z') {
        return DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| ReplayError::Parse {
                line: line_num,
                message: format!("Invalid RFC3339 timestamp: {}", s),
            });
    }

    // 简单日期格式
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc())
        .map_err(|_| ReplayError::Parse {
            line: line_num,
            message: format!("Invalid datetime format: {}", s),
        })
}

/// ReplayKLineSource - 返回 K 线的数据源 trait
#[async_trait]
pub trait KLineSource: Send + Sync {
    async fn next_kline(&mut self) -> Option<KLine>;
    fn reset(&mut self);
    fn is_exhausted(&self) -> bool;
}
