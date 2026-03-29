//! ReplaySource - 历史数据回放
//!
//! 复制自 b_data_source::replay_source

use crate::models::{KLine, Period};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, warn};

/// 历史数据回放
pub struct ReplaySource {
    symbols_filter: Vec<String>,
    period_filter: Option<Period>,
    current_idx: usize,
    data: Vec<KLine>,
    exhausted: bool,
}

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
    /// 从 CSV 文件创建
    pub async fn from_csv<P: AsRef<Path>>(path: P) -> Result<Self, ReplayError> {
        let path = path.as_ref();
        let file = File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut data = Vec::new();
        let mut line_num = 0;

        if let Some(header) = lines.next_line().await? {
            debug!("CSV header: {}", header);
            line_num += 1;
        }

        while let Some(line) = lines.next_line().await? {
            line_num += 1;
            if line.trim().is_empty() {
                continue;
            }

            match parse_csv_line(&line, line_num) {
                Ok(kline) => data.push(kline),
                Err(e) => {
                    warn!("Skipping line {}: {}", line_num, e);
                }
            }
        }

        data.sort_by_key(|k| k.timestamp);

        Ok(Self {
            symbols_filter: Vec::new(),
            period_filter: None,
            current_idx: 0,
            data,
            exhausted: false,
        })
    }

    /// 从内存数据创建
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

    pub fn with_symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols_filter = symbols;
        self
    }

    pub fn with_period(mut self, period: Period) -> Self {
        self.period_filter = Some(period);
        self
    }

    pub fn reset(&mut self) {
        self.current_idx = 0;
        self.exhausted = false;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 获取首根 K 线的时间戳（不消费数据，用于初始化时钟）
    pub fn first_timestamp(&self) -> Option<DateTime<Utc>> {
        self.data.first().map(|k| k.timestamp)
    }

    pub fn next_kline(&mut self) -> Option<KLine> {
        if self.exhausted {
            return None;
        }

        // 从当前位置向后找一根符合过滤条件的 K 线
        let found_idx = loop {
            if self.current_idx >= self.data.len() {
                self.exhausted = true;
                return None;
            }

            let kline = &self.data[self.current_idx];

            if !self.symbols_filter.is_empty() && !self.symbols_filter.contains(&kline.symbol) {
                self.current_idx += 1;
                continue;
            }

            if let Some(ref period) = self.period_filter {
                if kline.period != *period {
                    self.current_idx += 1;
                    continue;
                }
            }

            break self.current_idx;
        };

        let kline = self.data[found_idx].clone();
        self.current_idx += 1;

        // 如果这是最后一根（下次越界），立即标记 exhausted
        if self.current_idx >= self.data.len() {
            self.exhausted = true;
        }

        Some(kline)
    }

    /// 检查数据是否已耗尽
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// 转换为 b_data_source store 所需的 KlineData 格式（用于预加载历史数据）
    ///
    /// 用于沙盒启动时将 CSV 历史数据预填充到共享 Store，
    /// 让 Trader 在第一根 tick 到来前就能读取历史 K线。
    pub fn to_store_klines(&self) -> Vec<b_data_source::ws::kline_1m::ws::KlineData> {
        use b_data_source::ws::kline_1m::ws::KlineData as StoreKline;

        self.data
            .iter()
            .map(|k| StoreKline {
                kline_start_time: k.timestamp.timestamp_millis(),
                kline_close_time: k.timestamp.timestamp_millis() + 60_000,
                symbol: k.symbol.clone(),
                interval: "1m".to_string(),
                open: k.open.to_string(),
                close: k.close.to_string(),
                high: k.high.to_string(),
                low: k.low.to_string(),
                volume: k.volume.to_string(),
                is_closed: k.is_closed,
            })
            .collect()
    }
}

impl Iterator for ReplaySource {
    type Item = KLine;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_kline()
    }
}

fn parse_csv_line(line: &str, line_num: usize) -> Result<KLine, ReplayError> {
    let parts: Vec<&str> = line.split(',').collect();

    let (symbol, period_str, timestamp_idx, field_start) = if parts.len() == 6 {
        ("HOTUSDT".to_string(), "1m", 0, 1)
    } else if parts.len() >= 8 {
        (parts[0].to_string(), parts[1], 7, 2)
    } else {
        return Err(ReplayError::Parse {
            line: line_num,
            message: format!("Expected 6 or 8 fields, got {}", parts.len()),
        });
    };

    let open = parse_decimal(parts[field_start], line_num, "open")?;
    let high = parse_decimal(parts[field_start + 1], line_num, "high")?;
    let low = parse_decimal(parts[field_start + 2], line_num, "low")?;
    let close = parse_decimal(parts[field_start + 3], line_num, "close")?;
    let volume = parse_decimal(parts[field_start + 4], line_num, "volume")?;

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

fn parse_decimal(s: &str, line_num: usize, field_name: &str) -> Result<Decimal, ReplayError> {
    Decimal::from_str_exact(s)
        .map_err(|_| ReplayError::Parse {
            line: line_num,
            message: format!("Invalid {}: '{}'", field_name, s),
        })
}

fn parse_timestamp(s: &str, line_num: usize) -> Result<DateTime<Utc>, ReplayError> {
    if s.chars().all(|c| c.is_ascii_digit()) {
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

    if s.contains('T') || s.ends_with('Z') {
        return DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| ReplayError::Parse {
                line: line_num,
                message: format!("Invalid RFC3339 timestamp: {}", s),
            });
    }

    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc())
        .map_err(|_| ReplayError::Parse {
            line: line_num,
            message: format!("Invalid datetime format: {}", s),
        })
}

/// K线数据源 trait
#[async_trait]
pub trait KLineSource: Send + Sync {
    async fn next_kline(&mut self) -> Option<KLine>;
    fn reset(&mut self);
    fn is_exhausted(&self) -> bool;
}
