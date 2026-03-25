//! KlineLoader - 流式 Parquet K线加载器
//!
//! 从 Python pandas 输出的 Parquet 文件流式读取 1m K线数据。
//!
//! ## 与旧版 backtest/loader.rs 的区别
//!
//! | 特性 | 旧版 (批量) | 新版 (流式) |
//! |------|------------|------------|
//! | 加载方式 | `load() -> Vec<KLine>` 一次性 | `Iterator<Item=Result<KLine>>` 按需 |
//! | 内存占用 | 全量加载 | 仅保留当前行 |
//! | 适用场景 | 小数据量 | 大数据量回测 |
//!
//! ## Python 输出格式
//!
//! ```python
//! df = pd.DataFrame(data, columns=['timestamp', 'open', 'high', 'low', 'close', 'volume'])
//! df.to_parquet(file_full_path, compression='snappy', index=False)
//! ```

use std::path::Path;
use std::fs::File;
use std::fmt;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::file::metadata::RowGroupMetadata;

use b_data_source::{KLine, Period};

/// Parquet K线加载器（流式）
///
/// 支持流式读取 Parquet 文件，逐行解析 K线数据。
///
/// ## 使用示例
///
/// ```ignore
/// let loader = KlineLoader::new("data/BTCUSDT_1m.parquet")?;
/// for kline_result in loader {
///     let kline = kline_result?;
///     // 处理每根 K线
/// }
/// ```
pub struct KlineLoader {
    /// Parquet 文件路径
    path: String,
    /// 文件读取器
    reader: SerializedFileReader<File>,
    /// 当前行组索引
    current_row_group: usize,
    /// 当前行组中的行索引
    current_row: usize,
    /// 当前行组的总行数
    current_row_group_rows: usize,
    /// 当前行组各列数据
    current_columns: Option<ColumnData>,
    /// 验证：上一根 K线的时间戳（用于检查连续性）
    last_timestamp: Option<i64>,
    /// 交易对名称
    symbol: String,
}

/// 当前行组的列数据
struct ColumnData {
    timestamp: Vec<i64>,
    open: Vec<Decimal>,
    high: Vec<Decimal>,
    low: Vec<Decimal>,
    close: Vec<Decimal>,
    volume: Vec<Decimal>,
}

impl KlineLoader {
    /// 创建加载器
    pub fn new(path: &str) -> Result<Self, KlineLoadError> {
        if !Path::new(path).exists() {
            return Err(KlineLoadError::FileNotFound(path.to_string()));
        }

        let file = File::open(path)
            .map_err(|e| KlineLoadError::IoError(path.to_string(), e.to_string()))?;

        let reader = SerializedFileReader::new(file)
            .map_err(|e| KlineLoadError::ParquetError(path.to_string(), e.to_string()))?;

        let num_row_groups = reader.metadata().num_row_groups();
        if num_row_groups == 0 {
            return Err(KlineLoadError::EmptyFile(path.to_string()));
        }

        Ok(Self {
            path: path.to_string(),
            reader,
            current_row_group: 0,
            current_row: 0,
            current_row_group_rows: 0,
            current_columns: None,
            last_timestamp: None,
            symbol: "UNKNOWN".to_string(),
        })
    }

    /// 获取文件信息（不加载数据）
    pub fn info(&self) -> ParquetInfo {
        let metadata = self.reader.metadata();
        let num_rows: usize = (0..metadata.num_row_groups())
            .map(|i| {
                metadata.row_group(i)
                    .map(|rg| rg.num_rows() as usize)
                    .unwrap_or(0)
            })
            .sum();

        let columns: Vec<String> = metadata.schema()
            .get_columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        ParquetInfo {
            path: self.path.clone(),
            num_rows,
            num_row_groups: metadata.num_row_groups(),
            columns,
        }
    }

    /// 设置交易对名称
    pub fn with_symbol(mut self, symbol: &str) -> Self {
        self.symbol = symbol.to_string();
        self
    }

    /// 内部：加载当前行组的全部列数据
    fn load_current_row_group(&mut self) -> Result<(), KlineLoadError> {
        if self.current_row_group >= self.reader.metadata().num_row_groups() {
            self.current_columns = None;
            return Ok(());
        }

        let row_group = self.reader.read_row_group(self.current_row_group, None)
            .map_err(|e| KlineLoadError::ParquetError(
                self.path.clone(),
                format!("row_group {}: {}", self.current_row_group, e)
            ))?;

        self.current_row_group_rows = row_group.num_rows() as usize;
        self.current_row = 0;

        // 读取各列数据
        let timestamp_col = Self::read_int64_column(&row_group, "timestamp")?;
        let open_col = Self::read_decimal_column(&row_group, "open")?;
        let high_col = Self::read_decimal_column(&row_group, "high")?;
        let low_col = Self::read_decimal_column(&row_group, "low")?;
        let close_col = Self::read_decimal_column(&row_group, "close")?;
        let volume_col = Self::read_decimal_column(&row_group, "volume")?;

        // 验证所有列长度一致
        let len = timestamp_col.len();
        if open_col.len() != len || high_col.len() != len ||
           low_col.len() != len || close_col.len() != len || volume_col.len() != len {
            return Err(KlineLoadError::ColumnLengthMismatch);
        }

        self.current_columns = Some(ColumnData {
            timestamp: timestamp_col,
            open: open_col,
            high: high_col,
            low: low_col,
            close: close_col,
            volume: volume_col,
        });

        Ok(())
    }

    /// 读取 i64 列
    fn read_int64_column(
        row_group: &RowGroupMetadata,
        name: &str,
    ) -> Result<Vec<i64>, KlineLoadError> {
        let col = Self::get_column(row_group, name)?;
        let mut result = Vec::with_capacity(col.len());

        for i in 0..col.len() {
            let field = col.get(i);
            let val = match field {
                parquet::record::Field::Long(v) => *v,
                parquet::record::Field::Int(v) => *v as i64,
                parquet::record::Field::UnsignedLong(v) => *v as i64,
                parquet::record::Field::UnsignedInt(v) => *v as i64,
                _ => return Err(KlineLoadError::InvalidFieldType(
                    name.to_string(),
                    format!("expected i64, got {:?}", field),
                )),
            };
            result.push(val);
        }

        Ok(result)
    }

    /// 读取 Decimal 列
    fn read_decimal_column(
        row_group: &RowGroupMetadata,
        name: &str,
    ) -> Result<Vec<Decimal>, KlineLoadError> {
        let col = Self::get_column(row_group, name)?;
        let mut result = Vec::with_capacity(col.len());

        for i in 0..col.len() {
            let field = col.get(i);
            let val = match field {
                parquet::record::Field::Double(v) => {
                    Decimal::from_f64_retain(*v).unwrap_or(Decimal::ZERO)
                }
                parquet::record::Field::Float(v) => {
                    Decimal::from_f64_retain(*v as f64).unwrap_or(Decimal::ZERO)
                }
                parquet::record::Field::Long(v) => Decimal::from(*v),
                parquet::record::Field::Int(v) => Decimal::from(*v),
                parquet::record::Field::UnsignedLong(v) => Decimal::from(*v),
                parquet::record::Field::UnsignedInt(v) => Decimal::from(*v),
                _ => return Err(KlineLoadError::InvalidFieldType(
                    name.to_string(),
                    format!("expected decimal, got {:?}", field),
                )),
            };
            result.push(val);
        }

        Ok(result)
    }

    /// 获取列（兼容不同大小写）
    fn get_column<'a>(
        row_group: &'a RowGroupMetadata,
        name: &str,
    ) -> Result<&'a parquet::record::RowGroupAccessor, KlineLoadError> {
        let names = [name, name.to_uppercase(), name.to_lowercase()];
        for n in &names {
            if let Some(col) = row_group.column(n) {
                return Ok(col);
            }
        }
        Err(KlineLoadError::MissingColumn(name.to_string()))
    }

    /// 解析当前行的 K线
    fn parse_current_row(&self) -> Result<KLine, KlineLoadError> {
        let cols = self.current_columns.as_ref()
            .ok_or_else(|| KlineLoadError::NoData("no column data loaded".to_string()))?;

        if self.current_row >= cols.timestamp.len() {
            return Err(KlineLoadError::NoData("row index out of bounds".to_string()));
        }

        let timestamp_ms = cols.timestamp[self.current_row];
        let open = cols.open[self.current_row];
        let high = cols.high[self.current_row];
        let low = cols.low[self.current_row];
        let close = cols.close[self.current_row];
        let volume = cols.volume[self.current_row];

        // 解析时间戳
        let timestamp = DateTime::from_timestamp_millis(timestamp_ms)
            .ok_or_else(|| KlineLoadError::InvalidTimestamp(timestamp_ms))?;

        // 数据校验：OHLC 逻辑一致性
        Self::validate_ohlc(open, high, low, close, timestamp_ms)?;

        // 数据校验：时间戳连续性
        if let Some(last_ts) = self.last_timestamp {
            let diff = (timestamp_ms - last_ts).abs();
            let expected = 60_000i64; // 1m = 60000ms
            let tolerance = expected as f64 * 0.01;
            if (diff as f64 - expected).abs() > tolerance {
                tracing::warn!(
                    "K线时间戳不连续: expected_diff={}ms, actual_diff={}ms",
                    expected, diff
                );
            }
        }

        Ok(KLine {
            symbol: self.symbol.clone(),
            period: Period::Minute(1),
            open,
            high,
            low,
            close,
            volume,
            timestamp,
        })
    }

    /// 校验 OHLC 逻辑一致性
    fn validate_ohlc(
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        timestamp: i64,
    ) -> Result<(), KlineLoadError> {
        if low > open {
            return Err(KlineLoadError::OhlcViolation(
                timestamp,
                format!("low({}) > open({})", low, open),
            ));
        }
        if open > high {
            return Err(KlineLoadError::OhlcViolation(
                timestamp,
                format!("open({}) > high({})", open, high),
            ));
        }
        if low > close {
            return Err(KlineLoadError::OhlcViolation(
                timestamp,
                format!("low({}) > close({})", low, close),
            ));
        }
        if close > high {
            return Err(KlineLoadError::OhlcViolation(
                timestamp,
                format!("close({}) > high({})", close, high),
            ));
        }
        Ok(())
    }

    /// 获取总行数
    pub fn total_rows(&self) -> usize {
        (0..self.reader.metadata().num_row_groups())
            .map(|i| self.reader.metadata().row_group(i).map(|rg| rg.num_rows() as usize).unwrap_or(0))
            .sum()
    }
}

/// 流式迭代器实现
impl Iterator for KlineLoader {
    type Item = Result<KLine, KlineLoadError>;

    fn next(&mut self) -> Option<Self::Item> {
        // 如果当前行组未加载，先加载
        if self.current_columns.is_none() {
            if let Err(e) = self.load_current_row_group() {
                return Some(Err(e));
            }
            // 如果没有更多数据
            if self.current_columns.is_none() {
                return None;
            }
        }

        let cols = self.current_columns.as_ref()?;

        // 如果当前行组已读完
        if self.current_row >= self.current_row_group_rows {
            self.current_row_group += 1;
            self.current_row = 0;
            self.current_columns = None;
            return self.next();
        }

        // 解析当前行
        let row_idx = self.current_row;
        self.current_row += 1;

        match self.parse_current_row() {
            Ok(mut kline) => {
                // 更新最后时间戳
                let ts = kline.timestamp.timestamp_millis();
                self.last_timestamp = Some(ts);
                // 设置 symbol
                kline.symbol = self.symbol.clone();
                Some(Ok(kline))
            }
            Err(e) => {
                // 数据错误：跳过当前行，继续下一行
                tracing::warn!("跳过无效 K线 [row {}]: {:?}", row_idx, e);
                self.current_row += 1;
                self.next()
            }
        }
    }
}

/// K线加载错误类型
#[derive(Debug, Clone)]
pub enum KlineLoadError {
    /// 文件不存在
    FileNotFound(String),
    /// IO 错误
    IoError(String, String),
    /// Parquet 解析错误
    ParquetError(String, String),
    /// 缺少列
    MissingColumn(String),
    /// 字段类型错误
    InvalidFieldType(String, String),
    /// 解析错误
    ParseError(String, String),
    /// 无效时间戳
    InvalidTimestamp(i64),
    /// OHLC 逻辑错误
    OhlcViolation(i64, String),
    /// 空文件
    EmptyFile(String),
    /// 列长度不匹配
    ColumnLengthMismatch,
    /// 无数据
    NoData(String),
}

impl fmt::Display for KlineLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KlineLoadError::FileNotFound(path) => write!(f, "文件不存在: {}", path),
            KlineLoadError::IoError(path, msg) => write!(f, "IO 错误 [{}]: {}", path, msg),
            KlineLoadError::ParquetError(path, msg) => write!(f, "Parquet 解析错误 [{}]: {}", path, msg),
            KlineLoadError::MissingColumn(col) => write!(f, "缺少列: {}", col),
            KlineLoadError::InvalidFieldType(name, got) => write!(f, "字段类型错误 [{}]: {}", name, got),
            KlineLoadError::ParseError(ty, val) => write!(f, "解析 {} 失败: {}", ty, val),
            KlineLoadError::InvalidTimestamp(ts) => write!(f, "无效时间戳: {}", ts),
            KlineLoadError::OhlcViolation(ts, msg) => write!(f, "OHLC 校验失败 [timestamp={}]: {}", ts, msg),
            KlineLoadError::EmptyFile(path) => write!(f, "空文件: {}", path),
            KlineLoadError::ColumnLengthMismatch => write!(f, "列长度不匹配"),
            KlineLoadError::NoData(msg) => write!(f, "无数据: {}", msg),
        }
    }
}

impl std::error::Error for KlineLoadError {}

/// Parquet 文件信息
#[derive(Debug, Clone)]
pub struct ParquetInfo {
    /// 文件路径
    pub path: String,
    /// 总行数
    pub num_rows: usize,
    /// 行组数
    pub num_row_groups: usize,
    /// 列名列表
    pub columns: Vec<String>,
}

impl fmt::Display for ParquetInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parquet: {} ({} rows, {} groups)\nColumns: {}",
            self.path,
            self.num_rows,
            self.num_row_groups,
            self.columns.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation_nonexistent() {
        let loader = KlineLoader::new("nonexistent.parquet");
        assert!(loader.is_err());
        match loader.unwrap_err() {
            KlineLoadError::FileNotFound(_) => {}
            e => panic!("期望 FileNotFound, 得到: {:?}", e),
        }
    }

    #[test]
    fn test_ohlc_validation() {
        use rust_decimal_macros::dec;

        // 有效数据
        let result = KlineLoader::validate_ohlc(
            dec!(50000),
            dec!(50200),
            dec!(49900),
            dec!(50100),
            1700000060000,
        );
        assert!(result.is_ok());

        // 无效：low > open
        let result = KlineLoader::validate_ohlc(
            dec!(49900),
            dec!(50200),
            dec!(50000),
            dec!(50100),
            1700000060000,
        );
        assert!(result.is_err());
    }
}
