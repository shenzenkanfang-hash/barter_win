//! ShardCache - 分片缓存管理
//!
//! 提供历史 K线数据的分片存储和读取功能，支持：
//! - 分片文件管理 (ShardFile)
//! - 分片写入 (ShardWriter) - 只在 finish()/Drop 时 flush
//! - 分片读取 (ShardReader) - Iterator 模式
//! - 多分片链式读取 (ShardReaderChain)
//!
//! ## 分片文件命名规则
//!
//! `part_{start_ms}.csv` - 例如 `part_1742534400000.csv`
//!
//! ## CSV 格式
//!
//! `symbol,period,open,high,low,close,volume,timestamp_ms`

use b_data_source::{KLine, Period};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// ShardCache 通用错误
#[derive(Debug, Error)]
pub enum ShardCacheError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Path does not exist: {0}")]
    PathNotFound(String),

    #[error("Invalid filename format: {0}")]
    InvalidFilename(String),

    #[error("Decimal parse error: {0}")]
    DecimalParse(String),
}

/// ShardReader 错误
#[derive(Debug, Error)]
pub enum ShardReadError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("End of shards")]
    EndOfShards,
}

/// ShardWriter 错误
#[derive(Debug, Error)]
pub enum ShardWriteError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
}

// ============================================================================
// ShardFile
// ============================================================================

/// 分片文件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardFile {
    /// 文件完整路径
    pub path: PathBuf,
    /// 分片起始时间 (Unix timestamp ms)
    pub start_ms: i64,
    /// 分片结束时间 (Unix timestamp ms)
    pub end_ms: i64,
}

impl ShardFile {
    /// 从文件路径解析 ShardFile
    ///
    /// 文件名格式: `part_{start_ms}.csv`
    ///
    /// 注意: end_ms 需要通过读取文件内容来确定（最后一行的时间戳）
    pub fn from_path(path: &Path) -> Result<Self, ShardCacheError> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ShardCacheError::InvalidPath(path.display().to_string()))?;

        // 解析 part_{timestamp}.csv 格式
        let start_ms = filename
            .strip_prefix("part_")
            .and_then(|s| s.strip_suffix(".csv"))
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| ShardCacheError::InvalidFilename(filename.to_string()))?;

        Ok(Self {
            path: path.to_path_buf(),
            start_ms,
            end_ms: 0, // 需要通过读取文件内容确定
        })
    }

    /// 设置结束时间（通常在读取文件后设置）
    pub fn with_end_ms(mut self, end_ms: i64) -> Self {
        self.end_ms = end_ms;
        self
    }
}

// ============================================================================
// ShardCache
// ============================================================================

/// 分片缓存管理器
pub struct ShardCache {
    /// 缓存根目录
    cache_root: PathBuf,
}

impl ShardCache {
    /// 创建 ShardCache 实例
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
        }
    }

    /// 返回缓存根目录
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// 查找指定时间范围内的分片文件
    ///
    /// 扫描 `cache_root/{symbol}/{interval}/` 目录
    /// 过滤 `part_*.csv` 模式文件
    /// 按 start_ms 升序排列
    pub fn find_shards(
        &self,
        symbol: &str,
        interval: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ShardFile>, ShardCacheError> {
        let dir_path = self.cache_root.join(symbol).join(interval);

        if !dir_path.exists() {
            return Err(ShardCacheError::PathNotFound(dir_path.display().to_string()));
        }

        let mut shards = Vec::new();

        for entry in fs::read_dir(&dir_path)? {
            let entry = entry?;
            let path = entry.path();

            // 只处理 .csv 文件
            if path.extension().and_then(|s| s.to_str()) != Some("csv") {
                continue;
            }

            // 解析分片文件
            if let Ok(mut shard) = ShardFile::from_path(&path) {
                // 检查时间范围是否有重叠
                if shard.start_ms <= end_ms && shard.start_ms >= start_ms {
                    shards.push(shard);
                }
            }
        }

        // 按 start_ms 升序排列
        shards.sort_by_key(|s| s.start_ms);

        Ok(shards)
    }

    /// 检查分片是否覆盖指定时间范围（连续无间隙）
    ///
    /// 要求：
    /// 1. 分片按 start_ms 连续（无间隙）
    /// 2. 最后一个分片的 end_ms >= 请求的 end_ms
    pub fn shards_cover_range(
        &self,
        shards: &[ShardFile],
        start_ms: i64,
        end_ms: i64,
    ) -> bool {
        if shards.is_empty() {
            return false;
        }

        // 检查第一个分片是否从请求的起始时间开始（或之前）
        let first = &shards[0];
        if first.start_ms > start_ms {
            return false;
        }

        // 检查分片是否连续
        for i in 0..shards.len() - 1 {
            let current = &shards[i];
            let next = &shards[i + 1];

            // 当前分片的 end_ms 应该等于下一个分片的 start_ms（连续）
            if current.end_ms != next.start_ms {
                return false;
            }
        }

        // 检查最后一个分片是否覆盖到请求的结束时间
        let last = shards.last().unwrap();
        if last.end_ms < end_ms {
            return false;
        }

        true
    }

    /// 写入分片文件
    ///
    /// 创建 `cache_root/{symbol}/{interval}/part_{start_ms}.csv`
    pub fn write_shard(
        &self,
        symbol: &str,
        interval: &str,
        klines: &[KLine],
    ) -> Result<ShardFile, ShardCacheError> {
        if klines.is_empty() {
            return Err(ShardCacheError::InvalidData("klines is empty".to_string()));
        }

        let start_ms = klines[0]
            .timestamp
            .timestamp_millis();

        // 构建目录和文件路径
        let dir_path = self.cache_root.join(symbol).join(interval);
        fs::create_dir_all(&dir_path)?;

        let filename = format!("part_{}.csv", start_ms);
        let file_path = dir_path.join(&filename);

        // 使用 ShardWriter 写入
        let mut writer = ShardWriter::new(&file_path)?;
        for kline in klines {
            writer.write(kline)?;
        }
        let end_ms = writer.finish()?.end_ms;

        Ok(ShardFile {
            path: file_path,
            start_ms,
            end_ms,
        })
    }
}

// ============================================================================
// ShardWriter
// ============================================================================

/// 分片写入器
///
/// CSV 格式: `symbol,period,open,high,low,close,volume,timestamp_ms`
///
/// 注意: 不自动封片，只在 `finish()` 或 Drop 时 flush
pub struct ShardWriter {
    writer: csv::Writer<io::BufWriter<std::fs::File>>,
    file_path: PathBuf,
    count: usize,
    last_timestamp_ms: i64,
}

impl ShardWriter {
    /// 创建新的 ShardWriter
    pub fn new(file_path: impl Into<PathBuf>) -> Result<Self, ShardWriteError> {
        let file_path = file_path.into();

        // 确保父目录存在
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(&file_path)?;
        let writer = csv::WriterBuilder::new()
            .has_headers(false) // 无 CSV header
            .from_writer(io::BufWriter::new(file));

        Ok(Self {
            writer,
            file_path,
            count: 0,
            last_timestamp_ms: 0,
        })
    }

    /// 写入一条 K线
    ///
    /// 注意: 不检查 shard_size 限制，只负责写入
    /// 调用者负责在适当时候调用 `finish()` 封片
    pub fn write(&mut self, kline: &KLine) -> Result<(), ShardWriteError> {
        let timestamp_ms = kline.timestamp.timestamp_millis();
        let period_str = period_to_string(&kline.period);

        self.writer.write_record(&[
            &kline.symbol,
            &period_str,
            &kline.open.to_string(),
            &kline.high.to_string(),
            &kline.low.to_string(),
            &kline.close.to_string(),
            &kline.volume.to_string(),
            &timestamp_ms.to_string(),
        ])?;

        self.count += 1;
        self.last_timestamp_ms = timestamp_ms;

        Ok(())
    }

    /// 手动 flush
    pub fn flush(&mut self) -> Result<(), ShardWriteError> {
        self.writer.flush()?;
        Ok(())
    }

    /// 完成写入，返回分片信息
    ///
    /// 只是 flush 并返回 ShardFile，不阻止后续写入
    pub fn finish(&mut self) -> Result<ShardFile, ShardWriteError> {
        self.writer.flush()?;

        // 解析 start_ms 从文件名
        let filename = self.file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("part_0.csv");

        let start_ms = filename
            .strip_prefix("part_")
            .and_then(|s| s.strip_suffix(".csv"))
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        Ok(ShardFile {
            path: self.file_path.clone(),
            start_ms,
            end_ms: self.last_timestamp_ms,
        })
    }
}

impl Drop for ShardWriter {
    fn drop(&mut self) {
        // 自动 flush 确保数据写入磁盘
        let _ = self.writer.flush();
    }
}

// ============================================================================
// ShardReader
// ============================================================================

/// 分片读取器 - Iterator 模式
pub struct ShardReader {
    reader: csv::Reader<io::BufReader<std::fs::File>>,
    buffer: Vec<KLine>,
    buffer_index: usize,
    file_path: PathBuf,
    start_ms: i64,
    end_ms: i64,
    finished: bool,
}

impl ShardReader {
    /// 创建新的 ShardReader
    pub fn new(file_path: impl Into<PathBuf>) -> Result<Self, ShardReadError> {
        let file_path = file_path.into();

        let file = std::fs::File::open(&file_path)?;
        let reader = csv::ReaderBuilder::new()
            .has_headers(false) // 无 CSV header
            .from_reader(io::BufReader::new(file));

        // 从文件名解析 start_ms
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("part_0.csv");

        let start_ms = filename
            .strip_prefix("part_")
            .and_then(|s| s.strip_suffix(".csv"))
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        Ok(Self {
            reader,
            buffer: Vec::with_capacity(1000),
            buffer_index: 0,
            file_path,
            start_ms,
            end_ms: 0,
            finished: false,
        })
    }

    /// 读取一批 K线到缓冲区
    fn fill_buffer(&mut self) -> Result<(), ShardReadError> {
        self.buffer.clear();
        self.buffer_index = 0;

        let mut count = 0;
        const BATCH_SIZE: usize = 1000;

        for result in self.reader.records() {
            let record = result?;
            let kline = parse_kline_from_record(&record)?;

            self.buffer.push(kline);
            count += 1;

            if count >= BATCH_SIZE {
                break;
            }
        }

        if self.buffer.is_empty() {
            self.finished = true;
        }

        Ok(())
    }

    /// 获取分片信息
    pub fn shard_info(&self) -> ShardFile {
        ShardFile {
            path: self.file_path.clone(),
            start_ms: self.start_ms,
            end_ms: self.end_ms,
        }
    }

    /// 获取起始时间
    pub fn start_ms(&self) -> i64 {
        self.start_ms
    }

    /// 获取结束时间
    pub fn end_ms(&self) -> i64 {
        self.end_ms
    }
}

impl Iterator for ShardReader {
    type Item = Result<KLine, ShardReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // 如果缓冲区为空，填充缓冲区
        if self.buffer_index >= self.buffer.len() {
            if let Err(e) = self.fill_buffer() {
                self.finished = true;
                return Some(Err(e));
            }

            if self.buffer.is_empty() {
                self.finished = true;
                return None;
            }
        }

        let kline = self.buffer[self.buffer_index].clone();
        self.end_ms = kline.timestamp.timestamp_millis();
        self.buffer_index += 1;

        Some(Ok(kline))
    }
}

// ============================================================================
// ShardReaderChain
// ============================================================================

/// 多分片链式读取器
///
/// 按顺序读取多个分片文件，无缝连接数据
pub struct ShardReaderChain {
    readers: Vec<ShardReader>,
    current_index: usize,
}

impl ShardReaderChain {
    /// 创建新的 ShardReaderChain
    pub fn new(readers: Vec<ShardReader>) -> Self {
        Self {
            readers,
            current_index: 0,
        }
    }

    /// 从分片文件列表创建链式读取器
    pub fn from_shards(shards: &[ShardFile]) -> Result<Self, ShardReadError> {
        let mut readers = Vec::new();

        for shard in shards {
            let reader = ShardReader::new(&shard.path)?;
            readers.push(reader);
        }

        Ok(Self {
            readers,
            current_index: 0,
        })
    }
}

impl Iterator for ShardReaderChain {
    type Item = Result<KLine, ShardReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        // 找到还有数据的 reader
        while self.current_index < self.readers.len() {
            if let Some(item) = self.readers[self.current_index].next() {
                return Some(item);
            }
            // 当前 reader 已耗尽，移动到下一个
            self.current_index += 1;
        }

        None
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// 将 Period 转换为字符串
pub fn period_to_string(period: &Period) -> String {
    match period {
        Period::Minute(m) => format!("{}m", m),
        Period::Day => "1d".to_string(),
    }
}

/// 将字符串转换为 Period
pub fn string_to_period(s: &str) -> Option<Period> {
    if s.ends_with('m') {
        let num_str = s.trim_end_matches('m');
        let num: u8 = num_str.parse().ok()?;
        Some(Period::Minute(num))
    } else if s == "1d" {
        Some(Period::Day)
    } else {
        None
    }
}

/// 从 CSV record 解析 KLine
fn parse_kline_from_record(record: &csv::Record<'_>) -> Result<KLine, ShardReadError> {
    if record.len() != 8 {
        return Err(ShardReadError::InvalidData(format!(
            "Expected 8 fields, got {}",
            record.len()
        )));
    }

    let symbol = record[0].to_string();
    let period = string_to_period(&record[1])
        .ok_or_else(|| ShardReadError::InvalidData(format!("Invalid period: {}", &record[1])))?;

    let open = parse_decimal(&record[2])
        .map_err(|e| ShardReadError::InvalidData(format!("Invalid open: {}", e)))?;
    let high = parse_decimal(&record[3])
        .map_err(|e| ShardReadError::InvalidData(format!("Invalid high: {}", e)))?;
    let low = parse_decimal(&record[4])
        .map_err(|e| ShardReadError::InvalidData(format!("Invalid low: {}", e)))?;
    let close = parse_decimal(&record[5])
        .map_err(|e| ShardReadError::InvalidData(format!("Invalid close: {}", e)))?;
    let volume = parse_decimal(&record[6])
        .map_err(|e| ShardReadError::InvalidData(format!("Invalid volume: {}", e)))?;

    let timestamp_ms: i64 = record[7].parse()
        .map_err(|e| ShardReadError::InvalidData(format!("Invalid timestamp: {}", e)))?;
    let timestamp = Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .ok_or_else(|| ShardReadError::InvalidData(format!("Invalid timestamp: {}", timestamp_ms)))?;

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

/// 解析 Decimal
fn parse_decimal(s: &str) -> Result<Decimal, ShardCacheError> {
    s.parse::<Decimal>()
        .map_err(|_| ShardCacheError::DecimalParse(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_period_conversion() {
        assert_eq!(period_to_string(&Period::Minute(1)), "1m");
        assert_eq!(period_to_string(&Period::Minute(5)), "5m");
        assert_eq!(period_to_string(&Period::Minute(15)), "15m");
        assert_eq!(period_to_string(&Period::Day), "1d");

        assert_eq!(string_to_period("1m"), Some(Period::Minute(1)));
        assert_eq!(string_to_period("5m"), Some(Period::Minute(5)));
        assert_eq!(string_to_period("15m"), Some(Period::Minute(15)));
        assert_eq!(string_to_period("1d"), Some(Period::Day));
        assert_eq!(string_to_period("60m"), Some(Period::Minute(60)));
        assert_eq!(string_to_period("1h"), None); // 不支持 1h
    }

    #[test]
    fn test_shard_file_from_path() {
        let path = Path::new("data/btc/usdt/1m/part_1742534400000.csv");
        let shard = ShardFile::from_path(path).unwrap();
        assert_eq!(shard.start_ms, 1742534400000);
    }

    #[test]
    fn test_shard_file_invalid_path() {
        let path = Path::new("data/invalid.csv");
        let result = ShardFile::from_path(path);
        assert!(result.is_err());
    }
}
