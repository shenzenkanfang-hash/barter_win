//! KlineLoader - 流式 CSV K线加载器
//!
//! 从 Python pandas 输出的 CSV 文件流式读取 1m K线数据。

use std::path::Path;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::fmt;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

use b_data_source::{KLine, Period};

/// CSV K线加载器（流式）
pub struct KlineLoader {
    klines: std::vec::IntoIter<KLine>,
    symbol: String,
    total_rows: usize,
}

impl KlineLoader {
    /// 创建加载器
    pub fn new(path: &str) -> Result<Self, KlineLoadError> {
        if !Path::new(path).exists() {
            return Err(KlineLoadError::FileNotFound(path.to_string()));
        }

        let file = File::open(path)
            .map_err(|e| KlineLoadError::IoError(path.to_string(), e.to_string()))?;

        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // 跳过表头
        let header = lines.next()
            .ok_or_else(|| KlineLoadError::IoError(path.to_string(), "空文件".to_string()))?
            .map_err(|e| KlineLoadError::IoError(path.to_string(), e.to_string()))?;

        // 解析表头
        let headers: Vec<&str> = header.split(',').map(|s| s.trim()).collect();
        let col_idx = |name: &str| -> Result<usize, KlineLoadError> {
            headers.iter()
                .position(|&h| h.eq_ignore_ascii_case(name))
                .ok_or_else(|| KlineLoadError::ParseError(format!("找不到列: {}", name)))
        };

        let ts_idx = col_idx("timestamp")?;
        let open_idx = col_idx("open")?;
        let high_idx = col_idx("high")?;
        let low_idx = col_idx("low")?;
        let close_idx = col_idx("close")?;
        let volume_idx = col_idx("volume")?;

        let mut klines = Vec::new();

        for (line_no, line) in lines.enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("跳过行 {}: {}", line_no + 2, e);
                    continue;
                }
            };

            let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

            let ts = fields.get(ts_idx)
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let timestamp = DateTime::from_timestamp_millis(ts)
                .unwrap_or_else(Utc::now);

            let parse_decimal = |idx: usize| -> Decimal {
                fields.get(idx)
                    .and_then(|s| s.parse::<f64>().ok())
                    .and_then(|v| Decimal::from_f64_retain(v))
                    .unwrap_or(Decimal::ZERO)
            };

            klines.push(KLine {
                symbol: "UNKNOWN".to_string(),
                period: Period::Minute(1),
                open: parse_decimal(open_idx),
                high: parse_decimal(high_idx),
                low: parse_decimal(low_idx),
                close: parse_decimal(close_idx),
                volume: parse_decimal(volume_idx),
                timestamp,
            });
        }

        // 按时间排序
        klines.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let total_rows = klines.len();

        Ok(Self {
            klines: klines.into_iter(),
            symbol: "UNKNOWN".to_string(),
            total_rows,
        })
    }

    /// 获取总行数
    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    /// 设置交易对名称
    pub fn with_symbol(mut self, symbol: &str) -> Self {
        self.symbol = symbol.to_string();
        self
    }

    /// 获取文件信息
    pub fn info(&self) -> ParquetInfo {
        ParquetInfo {
            path: "csv_file".to_string(),
            num_rows: self.total_rows,
            num_row_groups: 0,
            columns: vec![
                "timestamp".to_string(),
                "open".to_string(),
                "high".to_string(),
                "low".to_string(),
                "close".to_string(),
                "volume".to_string(),
            ],
        }
    }
}

/// 迭代器实现
impl Iterator for KlineLoader {
    type Item = Result<KLine, KlineLoadError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.klines.next().map(|mut kline| {
            kline.symbol = self.symbol.clone();
            Ok(kline)
        })
    }
}

/// K线加载错误类型
#[derive(Debug, Clone)]
pub enum KlineLoadError {
    FileNotFound(String),
    IoError(String, String),
    ParseError(String),
}

impl fmt::Display for KlineLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KlineLoadError::FileNotFound(path) => write!(f, "文件不存在: {}", path),
            KlineLoadError::IoError(path, msg) => write!(f, "IO 错误 [{}]: {}", path, msg),
            KlineLoadError::ParseError(msg) => write!(f, "解析错误: {}", msg),
        }
    }
}

impl std::error::Error for KlineLoadError {}

/// Parquet 文件信息（复用）
#[derive(Debug, Clone)]
pub struct ParquetInfo {
    pub path: String,
    pub num_rows: usize,
    pub num_row_groups: usize,
    pub columns: Vec<String>,
}

impl fmt::Display for ParquetInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CSV: {} ({} rows)", self.path, self.num_rows)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_nonexistent_file() {
        let loader = super::KlineLoader::new("nonexistent.csv");
        assert!(loader.is_err());
    }
}
