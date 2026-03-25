//! ShardCache 分片缓存管理器
//!
//! 管理本地分片缓存的扫描、读取和写入。
//!
//! ## 缓存目录结构
//!
//! ```text
//! {cache_root}/{symbol}/{interval}/part_{start_ms}.csv
//! ```
//!
//! ## 分片大小
//!
//! 每分片包含 50000 条 K线数据。

use std::path::{Path, PathBuf};
use thiserror::Error;
use b_data_source::{KLine, Period};
use rust_decimal::Decimal;
use chrono::{TimeZone, Utc};
use csv::{ReaderBuilder, Writer};

// ============================================================================
// Error Types
// ============================================================================

/// ShardCache 通用错误
#[derive(Debug, Error)]
pub enum ShardCacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// ShardReader 错误
#[derive(Debug, Error)]
pub enum ShardReadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("Parse error: {0}")]
    Parse(String),
}

/// ShardWriter 错误
#[derive(Debug, Error)]
pub enum ShardWriteError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("Invalid path")]
    InvalidPath,
}

// ============================================================================
// ShardFile
// ============================================================================

/// 分片文件元数据
#[derive(Debug, Clone)]
pub struct ShardFile {
    /// 文件路径
    pub path: PathBuf,
    /// 分片起始时间戳 (毫秒)
    pub start_ms: i64,
    /// 分片结束时间戳 (毫秒) = start_ms + 50000 * 60000
    pub end_ms: i64,
}

impl ShardFile {
    /// 从文件路径解析分片信息
    ///
    /// 文件名格式: `part_{start_ms}.csv`
    pub fn from_path(path: &Path) -> Option<Self> {
        let filename = path.file_stem()?.to_str()?;
        if !filename.starts_with("part_") {
            return None;
        }
        let start_ms = filename.strip_prefix("part_")?.parse().ok()?;
        // 50000 K-lines * 1 minute * 60 seconds * 1000 ms
        let end_ms = start_ms + 50_000 * 60_000;
        Some(ShardFile {
            path: path.to_path_buf(),
            start_ms,
            end_ms,
        })
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
    /// 创建 ShardCache
    pub fn new(cache_root: PathBuf) -> Self {
        Self { cache_root }
    }

    /// 获取缓存根目录
    pub fn cache_root(&self) -> &PathBuf {
        &self.cache_root
    }

    /// 构建分片文件的完整路径
    fn shard_path(&self, symbol: &str, interval: &str, start_ms: i64) -> PathBuf {
        self.cache_root
            .join(symbol)
            .join(interval)
            .join(format!("part_{}.csv", start_ms))
    }

    /// 扫描指定时间范围内的本地分片 (按时间排序)
    pub fn find_shards(
        &self,
        symbol: &str,
        interval: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Vec<ShardFile> {
        let dir = self.cache_root.join(symbol).join(interval);
        if !dir.exists() {
            return Vec::new();
        }

        let mut shards: Vec<ShardFile> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("csv") {
                    return None;
                }
                ShardFile::from_path(&path)
            })
            .filter(|shard| {
                // Check if shard overlaps with requested range
                shard.end_ms >= start_ms && shard.start_ms <= end_ms
            })
            .collect();

        shards.sort_by_key(|s| s.start_ms);
        shards
    }

    /// 检查分片是否覆盖完整时间范围
    pub fn shards_cover_range(shards: &[ShardFile], start_ms: i64, end_ms: i64) -> bool {
        if shards.is_empty() {
            return false;
        }

        // Check continuity and boundary coverage
        for i in 0..shards.len() {
            if i == 0 && shards[i].start_ms > start_ms {
                return false; // Starts after query
            }
            if i > 0 && shards[i - 1].end_ms != shards[i].start_ms {
                return false; // Gap in coverage
            }
        }

        shards.last().map(|s| s.end_ms >= end_ms).unwrap_or(false)
    }

    /// 创建新分片写入器
    pub fn write_shard(
        &self,
        symbol: &str,
        interval: &str,
        start_ms: i64,
    ) -> ShardWriter {
        let path = self.shard_path(symbol, interval, start_ms);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        ShardWriter::new(path)
    }
}

// ============================================================================
// ShardWriter
// ============================================================================

/// 分片写入器
pub struct ShardWriter {
    writer: Writer<std::fs::File>,
    path: PathBuf,
    count: usize,
}

impl ShardWriter {
    /// 创建写入器
    pub fn new(path: PathBuf) -> Self {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open shard file");
        let writer = Writer::from_writer(file);
        Self {
            writer,
            path,
            count: 0,
        }
    }

    /// 写入一条 K线
    pub fn write(&mut self, kline: &KLine) -> Result<(), ShardWriteError> {
        let row = ShardRow {
            symbol: &kline.symbol,
            period: period_to_string(&kline.period),
            open: kline.open.to_string(),
            high: kline.high.to_string(),
            low: kline.low.to_string(),
            close: kline.close.to_string(),
            volume: kline.volume.to_string(),
            timestamp: kline.timestamp.timestamp_millis(),
        };
        self.writer
            .serialize(row)
            .map_err(|e| ShardWriteError::Csv(e))?;
        self.count += 1;
        Ok(())
    }

    /// 强制封片
    pub fn finish(&mut self) -> Result<ShardFile, ShardWriteError> {
        self.writer
            .flush()
            .map_err(|e| ShardWriteError::Io(e))?;

        let start_ms = parse_start_ms_from_path(&self.path).ok_or(ShardWriteError::InvalidPath)?;
        let end_ms = start_ms + self.count as i64 * 60_000;

        Ok(ShardFile {
            path: self.path.clone(),
            start_ms,
            end_ms,
        })
    }
}

impl Drop for ShardWriter {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}

/// CSV 行结构
#[derive(Debug, serde::Serialize)]
struct ShardRow<'a> {
    symbol: &'a str,
    period: &'a str,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    timestamp: i64,
}

/// Period 转字符串
fn period_to_string(period: &Period) -> &str {
    match period {
        Period::Minute(1) => "1m",
        Period::Minute(5) => "5m",
        Period::Minute(15) => "15m",
        Period::Day => "1d",
        _ => "1m",
    }
}

/// 从路径解析起始时间戳
fn parse_start_ms_from_path(path: &PathBuf) -> Option<i64> {
    let filename = path.file_stem()?.to_str()?;
    filename.strip_prefix("part_")?.parse().ok()
}

// ============================================================================
// ShardReader
// ============================================================================

/// 分片流式读取器
pub struct ShardReader {
    reader: csv::Reader<std::fs::File>,
    buffer: Vec<KLine>,
    buffer_index: usize,
}

impl ShardReader {
    /// 创建读取器
    pub fn new(path: &Path) -> Result<Self, ShardReadError> {
        let file = std::fs::File::open(path)?;
        let reader = ReaderBuilder::new()
            .has_headers(false)
            .from_reader(file);
        Ok(Self {
            reader,
            buffer: Vec::with_capacity(1000),
            buffer_index: 0,
        })
    }

    /// 填充缓冲区
    fn fill_buffer(&mut self) -> Result<(), ShardReadError> {
        self.buffer.clear();
        self.buffer_index = 0;

        for (i, result) in self.reader.byte_records().enumerate() {
            if i >= 1000 {
                break;
            }
            let record = result?;
            if record.len() < 8 {
                continue;
            }
            if let Ok(kline) = Self::parse_record(&record) {
                self.buffer.push(kline);
            }
        }

        Ok(())
    }

    fn parse_record(record: &csv::ByteRecord) -> Result<KLine, ShardReadError> {
        let symbol = std::str::from_utf8(&record[0])
            .map_err(|e| ShardReadError::Parse(e.to_string()))?
            .to_string();
        let period_str = std::str::from_utf8(&record[1])
            .map_err(|e| ShardReadError::Parse(e.to_string()))?;
        let period = string_to_period(period_str);

        let open = parse_decimal(&record[2])?;
        let high = parse_decimal(&record[3])?;
        let low = parse_decimal(&record[4])?;
        let close = parse_decimal(&record[5])?;
        let volume = parse_decimal(&record[6])?;
        let timestamp_ms: i64 = std::str::from_utf8(&record[7])
            .map_err(|e| ShardReadError::Parse(e.to_string()))?
            .parse::<i64>()
            .map_err(|e| ShardReadError::Parse(e.to_string()))?;
        let timestamp = Utc.timestamp_millis_opt(timestamp_ms)
            .single()
            .ok_or_else(|| ShardReadError::Parse("Invalid timestamp".to_string()))?;

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
}

impl Iterator for ShardReader {
    type Item = Result<KLine, ShardReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer_index >= self.buffer.len() {
            if let Err(e) = self.fill_buffer() {
                return Some(Err(e));
            }
            if self.buffer.is_empty() {
                return None;
            }
        }
        let kline = self.buffer[self.buffer_index].clone();
        self.buffer_index += 1;
        Some(Ok(kline))
    }
}

/// 字符串转 Period
fn string_to_period(s: &str) -> Period {
    match s {
        "1m" => Period::Minute(1),
        "5m" => Period::Minute(5),
        "15m" => Period::Minute(15),
        "1d" => Period::Day,
        _ => Period::Minute(1),
    }
}

/// 解析 Decimal
fn parse_decimal(bytes: &[u8]) -> Result<Decimal, ShardReadError> {
    let s = std::str::from_utf8(bytes)
        .map_err(|e| ShardReadError::Parse(e.to_string()))?;
    s.parse::<Decimal>()
        .map_err(|e| ShardReadError::Parse(e.to_string()))
}

// ============================================================================
// ShardReaderChain
// ============================================================================

/// 将多个分片读取器串联为单个迭代器
pub struct ShardReaderChain {
    readers: Vec<ShardReader>,
    current_index: usize,
}

impl ShardReaderChain {
    /// 从多个读取器创建链
    pub fn new(readers: Vec<ShardReader>) -> Self {
        Self {
            readers,
            current_index: 0,
        }
    }
}

impl Iterator for ShardReaderChain {
    type Item = Result<KLine, ShardReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_index < self.readers.len() {
            if let Some(item) = self.readers[self.current_index].next() {
                return Some(item);
            }
            self.current_index += 1;
        }
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use rust_decimal_macros::dec;

    static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn next_dir_id() -> u64 {
        DIR_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    #[test]
    fn test_period_conversion() {
        assert_eq!(period_to_string(&Period::Minute(1)), "1m");
        assert_eq!(period_to_string(&Period::Minute(5)), "5m");
        assert_eq!(period_to_string(&Period::Minute(15)), "15m");
        assert_eq!(period_to_string(&Period::Day), "1d");

        assert_eq!(string_to_period("1m"), Period::Minute(1));
        assert_eq!(string_to_period("5m"), Period::Minute(5));
        assert_eq!(string_to_period("15m"), Period::Minute(15));
        assert_eq!(string_to_period("1d"), Period::Day);
    }

    #[test]
    fn test_shard_file_from_path() {
        let path = Path::new("data/btc/usdt/1m/part_1742534400000.csv");
        let shard = ShardFile::from_path(path)
            .expect("测试用例：固定路径格式必须有效");
        assert_eq!(shard.start_ms, 1742534400000);
        assert_eq!(shard.end_ms, 1742534400000 + 50_000 * 60_000);
    }

    #[test]
    fn test_shard_file_invalid_path() {
        let path = Path::new("data/invalid.csv");
        let result = ShardFile::from_path(path);
        assert!(result.is_none());
    }

    #[test]
    fn test_shard_write_and_read() {
        let dir_id = next_dir_id();
        let temp_dir = std::env::temp_dir().join(format!("shard_test_{}", dir_id));
        std::fs::create_dir_all(&temp_dir).ok();

        let path = temp_dir.join("part_1000.csv");
        let mut writer = ShardWriter::new(path.clone());

        for i in 0..100 {
            let kline = KLine {
                symbol: "BTCUSDT".to_string(),
                period: Period::Minute(1),
                open: dec!(50000),
                high: dec!(50100),
                low: dec!(49900),
                close: dec!(50050),
                volume: dec!(100),
                timestamp: Utc.timestamp_millis_opt(1000 + i as i64 * 60_000)
                    .single()
                    .expect("测试用例：固定时间戳必须有效"),
            };
            writer.write(&kline).expect("测试用例：写入必须成功");
        }
        writer.finish().expect("测试用例：finish 必须成功");

        // Read back
        let reader = ShardReader::new(&path).expect("测试用例：Reader 创建必须成功");
        let klines: Vec<_> = reader.filter_map(|r| r.ok()).collect();
        assert_eq!(klines.len(), 100);
        assert_eq!(klines[0].symbol, "BTCUSDT");

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_find_shards_filters_correctly() {
        let dir_id = next_dir_id();
        let temp_dir = std::env::temp_dir().join(format!("shard_scan_test_{}", dir_id));
        std::fs::create_dir_all(&temp_dir).ok();

        let cache = ShardCache::new(temp_dir.clone());

        // Create shard files
        let shard_dir = temp_dir.join("BTCUSDT").join("1m");
        std::fs::create_dir_all(&shard_dir).ok();

        std::fs::write(shard_dir.join("part_1000.csv"), "").expect("测试用例：文件创建必须成功");
        std::fs::write(shard_dir.join("part_4000000000.csv"), "").expect("测试用例：文件创建必须成功");
        std::fs::write(shard_dir.join("part_7000000000.csv"), "").expect("测试用例：文件创建必须成功");

        // Query range [2000, 5000000000]
        let shards = cache.find_shards("BTCUSDT", "1m", 2000, 5000000000);
        // Should find part_1000 and part_4000000000 (part_7000000000 starts at 7000000000 > 5000000000)
        assert_eq!(shards.len(), 2);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_shards_cover_range_logic() {
        let shards = vec![
            ShardFile {
                path: PathBuf::from("part_0.csv"),
                start_ms: 0,
                end_ms: 0 + 50_000 * 60_000,
            },
            ShardFile {
                path: PathBuf::from("part_3000000000.csv"),
                start_ms: 0 + 50_000 * 60_000,
                end_ms: 0 + 50_000 * 60_000 * 2,
            },
        ];

        // Query [0, 4000000000] - covered
        let covered = ShardCache::shards_cover_range(&shards, 0, 4000000000);
        assert!(covered);

        // Query [1000, 4000000000] - covered (first shard starts at 0 < 1000, so query is within coverage)
        let covered = ShardCache::shards_cover_range(&shards, 1000, 4000000000);
        assert!(covered);

        // Query [0, 7000000000] - extends beyond second shard
        let covered = ShardCache::shards_cover_range(&shards, 0, 7000000000);
        assert!(!covered);

        // Empty list
        let covered = ShardCache::shards_cover_range(&[], 0, 4000000000);
        assert!(!covered);
    }
}
