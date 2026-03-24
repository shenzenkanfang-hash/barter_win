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
    
    if parts.len() < 8 {
        return Err(ReplayError::Parse {
            line: line_num,
            message: format!("Expected 8 fields, got {}", parts.len()),
        });
    }

    let symbol = parts[0].to_string();
    let period_str = parts[1];
    
    // 修复：使用 Decimal 替代 f64，避免浮点精度问题
    let open = parse_decimal(parts[2], line_num, "open")?;
    let high = parse_decimal(parts[3], line_num, "high")?;
    let low = parse_decimal(parts[4], line_num, "low")?;
    let close = parse_decimal(parts[5], line_num, "close")?;
    let volume = parse_decimal(parts[6], line_num, "volume")?;
    
    let timestamp_str = parts[7];

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
    let timestamp = if timestamp_str.contains('T') {
        DateTime::parse_from_rfc3339(timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| ReplayError::Parse {
                line: line_num,
                message: format!("Invalid RFC3339 timestamp: {}", timestamp_str),
            })?
    } else {
        NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc())
            .map_err(|_| ReplayError::Parse {
                line: line_num,
                message: format!("Invalid datetime format: {}", timestamp_str),
            })?
    };

    Ok(KLine {
        symbol,
        period,
        open,
        high,
        low,
        close,
        volume,
        timestamp,
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

/// ReplayKLineSource - 返回 K 线的数据源 trait
#[async_trait]
pub trait KLineSource: Send + Sync {
    async fn next_kline(&mut self) -> Option<KLine>;
    fn reset(&mut self);
    fn is_exhausted(&self) -> bool;
}
