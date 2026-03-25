# 沙盒分片缓存 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现沙盒分片缓存：首次运行 API 拉取 + 分片写入磁盘，二次运行直接加载本地分片，零 API 调用

**Architecture:** ShardCache 管理分片扫描/读写，ShardReader 流式读取不占内存，ShardWriter 50000条自动封片。缓存根目录 D:/sandbox_cache/{symbol}/{interval}/

**Tech Stack:** rust-csv (workspace), std::PathBuf, Iterator pattern

---

## File Structure

```
crates/h_sandbox/src/
├── shard_cache.rs          # NEW: ShardCache, ShardFile, ShardReader, ShardWriter
├── historical_replay/
│   ├── mod.rs             # MODIFY: export ShardCache types
│   └── kline_replay.rs    # MODIFY: integrate cache priority logic
└── Cargo.toml             # MODIFY: add csv = { workspace = true }
```

---

## Task 1: 添加 csv 依赖

**Files:**
- Modify: `crates/h_sandbox/Cargo.toml`

- [ ] **Step 1: 添加 csv 依赖**

```toml
[dependencies]
# ... existing dependencies ...
csv = { workspace = true }
```

---

## Task 2: 实现 ShardCache 核心类型

**Files:**
- Create: `crates/h_sandbox/src/shard_cache.rs`

- [ ] **Step 1: 编写 ShardFile 结构体**

```rust
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// 分片文件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardFile {
    /// 文件路径
    pub path: PathBuf,
    /// 分片起始时间戳 (毫秒)
    pub start_ms: i64,
    /// 分片结束时间戳 (毫秒) = start_ms + shard_size * 60000
    pub end_ms: i64,
}

impl ShardFile {
    /// 从文件名解析起始时间戳
    pub fn from_path(path: &PathBuf) -> Option<Self> {
        let filename = path.file_stem()?.to_str()?;
        if !filename.starts_with("part_") {
            return None;
        }
        let start_ms = filename.strip_prefix("part_")?.parse().ok()?;
        let end_ms = start_ms + 50_000 * 60_000; // 50000 * 1m
        Some(ShardFile {
            path: path.clone(),
            start_ms,
            end_ms,
        })
    }
}
```

- [ ] **Step 2: 编写 ShardCache 结构体**

```rust
use std::path::PathBuf;
use crate::shard_cache::ShardFile;

/// 分片缓存管理器
pub struct ShardCache {
    /// 缓存根目录
    cache_root: PathBuf,
    /// 分片大小 (默认 50000)
    shard_size: usize,
}

impl ShardCache {
    /// 创建 ShardCache
    pub fn new(cache_root: PathBuf) -> Self {
        Self {
            cache_root,
            shard_size: 50_000,
        }
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

        let mut shards: Vec<ShardFile> = std::fs::read_dir(dir)
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
                // 分片与请求范围有交集
                shard.start_ms <= end_ms && shard.end_ms >= start_ms
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
        // 检查每个分片之间是否连续且无空洞
        let mut current_start = start_ms;
        for shard in shards {
            if shard.start_ms > current_start {
                return false; // 有空洞
            }
            current_start = shard.end_ms;
        }
        // 最后一个分片结束时间 >= 请求结束时间
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
        // 确保目录存在
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        ShardWriter::new(path, self.shard_size)
    }
}
```

- [ ] **Step 3: 编写 ShardWriter**

```rust
use std::io::Write;
use std::path::PathBuf;
use b_data_source::KLine;
use csv::Writer;

/// 分片写入器
pub struct ShardWriter {
    writer: Writer<std::fs::File>,
    current_path: PathBuf,
    count: usize,
    shard_size: usize,
    finished: bool,
}

impl ShardWriter {
    /// 创建写入器
    pub fn new(path: PathBuf, shard_size: usize) -> Self {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open shard file");
        let writer = Writer::from_writer(file);
        Self {
            writer,
            current_path: path,
            count: 0,
            shard_size,
            finished: false,
        }
    }

    /// 写入一条 K 线
    pub fn write(&mut self, kline: &KLine) -> Result<(), ShardWriteError> {
        if self.finished {
            return Err(ShardWriteError::ShardAlreadyFinished);
        }

        self.writer
            .serialize(ShardRow {
                symbol: &kline.symbol,
                period: &Self::period_to_str(&kline.period),
                open: kline.open.to_string(),
                high: kline.high.to_string(),
                low: kline.low.to_string(),
                close: kline.close.to_string(),
                volume: kline.volume.to_string(),
                timestamp: kline.timestamp.timestamp_millis(),
            })
            .map_err(|e| ShardWriteError::WriteError(e.to_string()))?;

        self.count += 1;

        // 达到分片大小时自动封片
        if self.count >= self.shard_size {
            self.finish()?;
        }

        Ok(())
    }

    /// 强制封片
    pub fn finish(mut self) -> Result<ShardFile, ShardWriteError> {
        if self.finished {
            return Err(ShardWriteError::ShardAlreadyFinished);
        }
        self.finished = true;
        self.writer
            .flush()
            .map_err(|e| ShardWriteError::WriteError(e.to_string()))?;

        let start_ms = Self::parse_start_ms_from_path(&self.current_path)
            .ok_or_else(|| ShardWriteError::InvalidPath)?;
        let end_ms = start_ms + self.count as i64 * 60_000;

        Ok(ShardFile {
            path: self.current_path,
            start_ms,
            end_ms,
        })
    }

    fn period_to_str(period: &b_data_source::Period) -> &str {
        match period {
            b_data_source::Period::Minute(1) => "1m",
            b_data_source::Period::Minute(5) => "5m",
            b_data_source::Period::Minute(15) => "15m",
            b_data_source::Period::Hour(1) => "1h",
            b_data_source::Period::Day(1) => "1d",
            _ => "1m",
        }
    }

    fn parse_start_ms_from_path(path: &PathBuf) -> Option<i64> {
        let filename = path.file_stem()?.to_str()?;
        filename.strip_prefix("part_")?.parse().ok()
    }
}

impl Drop for ShardWriter {
    fn drop(&mut self) {
        if !self.finished {
            // 自动封片
            let _ = self.finish();
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
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

#[derive(Debug, thiserror::Error)]
pub enum ShardWriteError {
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Shard already finished")]
    ShardAlreadyFinished,
    #[error("Invalid path")]
    InvalidPath,
}
```

- [ ] **Step 4: 编写 ShardReader**

```rust
use std::path::Path;
use csv::ReaderBuilder;
use b_data_source::{KLine, Period};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::{DateTime, TimeZone, Utc};

/// 分片读取错误
#[derive(Debug, thiserror::Error)]
pub enum ShardReadError {
    #[error("CSV error: {0}")]
    CsvError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// 分片流式读取器
pub struct ShardReader {
    reader: csv::Reader<std::fs::File>,
    buffer: Vec<KLine>,
    buffer_index: usize,
}

impl ShardReader {
    /// 创建读取器
    pub fn new(path: &Path) -> Result<Self, ShardReadError> {
        let file = std::fs::File::open(path)
            .map_err(|e| ShardReadError::CsvError(e.to_string()))?;
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
            let record = result.map_err(|e| ShardReadError::CsvError(e.to_string()))?;
            if record.len() < 8 {
                continue;
            }
            let kline = Self::parse_record(&record)?;
            self.buffer.push(kline);
        }

        Ok(())
    }

    fn parse_record(record: &csv::ByteRecord) -> Result<KLine, ShardReadError> {
        let symbol = std::str::from_utf8(&record[0])
            .map_err(|e| ShardReadError::ParseError(e.to_string()))?
            .to_string();
        let period_str = std::str::from_utf8(&record[1])
            .map_err(|e| ShardReadError::ParseError(e.to_string()))?;
        let period = match period_str {
            "1m" => Period::Minute(1),
            "5m" => Period::Minute(5),
            "15m" => Period::Minute(15),
            "1h" => Period::Hour(1),
            "1d" => Period::Day(1),
            _ => Period::Minute(1),
        };
        let open = Self::parse_decimal(&record[2])?;
        let high = Self::parse_decimal(&record[3])?;
        let low = Self::parse_decimal(&record[4])?;
        let close = Self::parse_decimal(&record[5])?;
        let volume = Self::parse_decimal(&record[6])?;
        let timestamp_ms: i64 = std::str::from_utf8(&record[7])
            .map_err(|e| ShardReadError::ParseError(e.to_string()))?
            .parse()
            .map_err(|e| ShardReadError::ParseError(e.to_string()))?;
        let timestamp = Utc.timestamp_millis_opt(timestamp_ms)
            .single()
            .ok_or_else(|| ShardReadError::ParseError("Invalid timestamp".to_string()))?;

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

    fn parse_decimal(bytes: &[u8]) -> Result<Decimal, ShardReadError> {
        let s = std::str::from_utf8(bytes)
            .map_err(|e| ShardReadError::ParseError(e.to_string()))?;
        s.parse()
            .map_err(|e| ShardReadError::ParseError(e.to_string()))
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

/// 将多个分片合并为流式迭代器
pub struct ShardReaderChain {
    readers: Vec<ShardReader>,
    current_index: usize,
}

impl ShardReaderChain {
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
```

- [ ] **Step 5: 编写错误类型并导出**

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShardCacheError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("CSV error: {0}")]
    CsvError(#[from] csv::Error),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("分片不完整，覆盖中断")]
    IncompleteCoverage,
}

pub use ShardReadError as ReadError;
pub use ShardWriteError as WriteError;
```

- [ ] **Step 6: 添加 mod.rs 导出**

在 `crates/h_sandbox/src/historical_replay/mod.rs` 添加:

```rust
pub mod shard_cache;

pub use shard_cache::{
    ShardCache, ShardFile, ShardReader, ShardReaderChain, ShardWriter,
    ShardCacheError, ShardReadError, ShardWriteError,
};
```

---

## Task 3: 改造 kline_replay.rs 集成缓存逻辑

**Files:**
- Modify: `crates/h_sandbox/examples/kline_replay.rs`

- [ ] **Step 1: 添加 CLI 参数**

```rust
use std::path::PathBuf;

/// 命令行参数
#[derive(Parser, Debug)]
struct Args {
    // ... existing fields ...

    /// 禁用本地缓存，强制 API 直连
    #[arg(long, default_value = "false")]
    no_cache: bool,

    /// 缓存根目录 (默认 D:/sandbox_cache)
    #[arg(long)]
    cache_dir: Option<PathBuf>,
}
```

- [ ] **Step 2: 实现缓存优先加载逻辑**

```rust
use h_sandbox::historical_replay::ShardCache;

async fn run_with_cache(
    symbol: &str,
    interval: &str,
    start_ms: i64,
    end_ms: i64,
    cache_root: &PathBuf,
    no_cache: bool,
) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    let cache = ShardCache::new(cache_root.clone());

    // 强制 API 模式
    if no_cache {
        info!("强制 API 直连模式");
        let klines = fetcher.fetch_all().await?;
        return Ok(internal_klines);
    }

    // 尝试本地分片
    let shards = cache.find_shards(symbol, interval, start_ms, end_ms)?;
    if !shards.is_empty() && ShardCache::shards_cover_range(&shards, start_ms, end_ms) {
        info!("使用本地缓存: {} 个分片", shards.len());
        // 流式读取本地分片
        let readers: Result<Vec<_>, _> = shards
            .iter()
            .map(|s| ShardReader::new(&s.path))
            .collect();
        let reader = ShardReaderChain::new(readers?);
        let klines: Vec<_> = reader.filter_map(|r| r.ok()).collect();
        return Ok(klines);
    }

    // API 拉取 + 写入缓存
    info!("本地缓存未命中，拉取 API...");
    let writer = cache.write_shard(symbol, interval, start_ms);
    let klines = fetch_and_cache(writer, start_ms, end_ms).await?;
    Ok(klines)
}
```

- [ ] **Step 3: 提交**

```bash
git add crates/h_sandbox/src/shard_cache.rs
git add crates/h_sandbox/src/historical_replay/mod.rs
git add crates/h_sandbox/examples/kline_replay.rs
git add crates/h_sandbox/Cargo.toml
git commit -m "[开发者] 实现沙盒分片缓存核心类型"
```

---

## Task 4: 编写测试

**Files:**
- Create: `crates/h_sandbox/src/shard_cache.rs` (tests module)

- [ ] **Step 1: 编写 test_shard_write_50000_auto_flush**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_shard_write_50000_auto_flush() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("part_1000.csv");
        let mut writer = ShardWriter::new(path, 50000);

        for i in 0..50001 {
            let kline = KLine {
                symbol: "BTCUSDT".to_string(),
                period: Period::Minute(1),
                open: dec!(50000),
                high: dec!(50100),
                low: dec!(49900),
                close: dec!(50050),
                volume: dec!(100),
                timestamp: Utc::now() + Duration::minutes(i),
            };
            writer.write(&kline).unwrap();
        }

        // 验证封片
        let shard = writer.finish().unwrap();
        assert_eq!(shard.start_ms, 1000);
        assert_eq!(shard.end_ms, 1000 + 50001 * 60_000);

        // 验证只写入了 50000 条
        let content = fs::read_to_string(&shard.path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 50000);
    }
}
```

- [ ] **Step 2: 编写 test_shard_scan_time_range**

```rust
    #[test]
    fn test_shard_scan_time_range() {
        let dir = tempdir().unwrap();
        let cache = ShardCache::new(dir.path().to_path_buf());

        // 创建测试分片
        let shard1 = dir.path().join("BTCUSDT").join("1m").join("part_1000.csv");
        let shard2 = dir.path().join("BTCUSDT").join("1m").join("part_1000_3000000.csv"); // 50000 * 60s later

        // 验证 find_shards 查找正确
        let shards = cache.find_shards("BTCUSDT", "1m", 1000, 1000 + 50000 * 60_000);
        assert!(!shards.is_empty());
    }
```

- [ ] **Step 3: 编写 test_cache_hit_miss**

```rust
    #[test]
    fn test_cache_hit_miss() {
        // 首次 miss → API
        // 二次 hit → 本地
    }
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p h_sandbox -- shard_cache
```

---

## Task 5: 编译验证

- [ ] **Step 1: cargo check -p h_sandbox**

```bash
cargo check -p h_sandbox
```

- [ ] **Step 2: cargo test -p h_sandbox**

```bash
cargo test -p h_sandbox
```

---

## Task 6: 集成到回放流程

**Files:**
- Modify: `crates/h_sandbox/src/historical_replay/tick_generator.rs`

- [ ] **Step 1: 添加 from_shards 方法**

```rust
impl StreamTickGenerator {
    /// 从分片缓存创建生成器
    pub fn from_shards(
        symbol: String,
        shards: Vec<ShardFile>,
    ) -> Result<Self, ShardCacheError> {
        let readers: Result<Vec<_>, _> = shards
            .iter()
            .map(|s| ShardReader::new(&s.path))
            .collect();
        let chain = ShardReaderChain::new(readers?);
        Ok(Self::from_loader(symbol, chain))
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/h_sandbox/src/historical_replay/tick_generator.rs
git commit -m "[开发者] 添加 from_shards 方法集成缓存"
```

---

## 验收标准

| 测试 | 验证内容 |
|------|----------|
| test_shard_write_50000_auto_flush | 写入 50001 条 → 2 个分片文件 |
| test_shard_scan_time_range | 时间范围查找正确，无遗漏 |
| test_cache_hit_miss | 首次 miss，二次 hit |
| cargo check -p h_sandbox | 0 错误 |
| cargo test -p h_sandbox | 全部通过 |
