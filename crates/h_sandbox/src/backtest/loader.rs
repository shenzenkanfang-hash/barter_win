//! Parquet 数据加载器
//!
//! 从 parquet 文件读取历史 K线数据

use std::path::Path;
use chrono::{DateTime, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

use b_data_source::{KLine, Period};

/// Parquet K线数据加载器
pub struct ParquetLoader {
    path: String,
}

impl ParquetLoader {
    /// 创建加载器
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }

    /// 加载 K线数据
    pub fn load(&self) -> Result<Vec<KLine>, String> {
        if !Path::new(&self.path).exists() {
            return Err(format!("文件不存在: {}", self.path));
        }

        let file = std::fs::File::open(&self.path)
            .map_err(|e| format!("打开文件失败: {}", e))?;

        let reader = SerializedFileReader::new(file)
            .map_err(|e| format!("读取 parquet 失败: {}", e))?;

        let mut klines = Vec::new();

        for i in 0..reader.metadata().num_row_groups() {
            let row_group = reader.read_row_group(i, None)
                .map_err(|e| format!("读取行组失败: {}", e))?;

            // 获取各列（兼容不同的列名格式）
            let symbol_col = Self::get_column(&row_group, "symbol");
            let open_col = Self::get_column(&row_group, "open");
            let high_col = Self::get_column(&row_group, "high");
            let low_col = Self::get_column(&row_group, "low");
            let close_col = Self::get_column(&row_group, "close");
            let volume_col = Self::get_column(&row_group, "volume");
            let timestamp_col = Self::get_column(&row_group, "timestamp");

            let num_rows = symbol_col.as_ref()
                .or(open_col.as_ref())
                .map(|c| c.len())
                .unwrap_or(0);

            for j in 0..num_rows {
                let symbol = symbol_col.as_ref()
                    .map(|c| Self::parse_string(c.get(j)))
                    .unwrap_or_else(|| "UNKNOWN".to_string());

                let open = open_col.as_ref()
                    .map(|c| Self::parse_decimal(c.get(j)))
                    .unwrap_or(Decimal::ZERO);

                let high = high_col.as_ref()
                    .map(|c| Self::parse_decimal(c.get(j)))
                    .unwrap_or(Decimal::ZERO);

                let low = low_col.as_ref()
                    .map(|c| Self::parse_decimal(c.get(j)))
                    .unwrap_or(Decimal::ZERO);

                let close = close_col.as_ref()
                    .map(|c| Self::parse_decimal(c.get(j)))
                    .unwrap_or(Decimal::ZERO);

                let volume = volume_col.as_ref()
                    .map(|c| Self::parse_decimal(c.get(j)))
                    .unwrap_or(Decimal::ZERO);

                let timestamp = timestamp_col.as_ref()
                    .map(|c| Self::parse_timestamp(c.get(j)))
                    .unwrap_or_else(Utc::now);

                klines.push(KLine {
                    symbol: Self::clean_symbol(&symbol),
                    period: Period::Minute(1),
                    open,
                    high,
                    low,
                    close,
                    volume,
                    timestamp,
                });
            }
        }

        // 按时间排序
        klines.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(klines)
    }

    /// 获取列数据
    fn get_column<'a>(
        row_group: &'a parquet::record::RowGroupAccessor,
        name: &str,
    ) -> Option<&'a parquet::record::Field> {
        // 尝试多种可能的列名
        let names = [name, name.to_uppercase(), name.to_lowercase()];
        for n in &names {
            if let Some(col) = row_group.column(n) {
                return Some(col);
            }
        }
        None
    }

    /// 解析字符串值
    fn parse_string(field: parquet::record::Field) -> String {
        match field {
            parquet::record::Field::Str(s) => s.clone(),
            parquet::record::Field::Symbol(s) => s.to_string(),
            parquet::record::Field::ByteArray(ba) => {
                String::from_utf8_lossy(ba.as_bytes()).to_string()
            }
            _ => format!("{:?}", field),
        }
    }

    /// 解析 Decimal 值
    fn parse_decimal(field: parquet::record::Field) -> Decimal {
        match field {
            parquet::record::Field::Double(v) => {
                Decimal::from_f64_retain(v).unwrap_or(Decimal::ZERO)
            }
            parquet::record::Field::Float(v) => {
                Decimal::from_f64_retain(v as f64).unwrap_or(Decimal::ZERO)
            }
            parquet::record::Field::Long(v) => Decimal::from(v),
            parquet::record::Field::Int(v) => Decimal::from(*v),
            parquet::record::Field::UnsignedLong(v) => Decimal::from(v),
            parquet::record::Field::UnsignedInt(v) => Decimal::from(*v),
            parquet::record::Field::DecimalByteArray(ba) => {
                // 简化处理
                Decimal::from_i128_with_scale(
                    i128::from_le_bytes(ba.bytes().chunks(16).next().unwrap_or(&[0; 16])),
                    8,
                )
            }
            _ => Decimal::ZERO,
        }
    }

    /// 解析时间戳
    fn parse_timestamp(field: parquet::record::Field) -> DateTime<Utc> {
        match field {
            parquet::record::Field::Long(v) => {
                // 毫秒时间戳
                DateTime::from_timestamp_millis(*v).unwrap_or_else(Utc::now)
            }
            parquet::record::Field::Int96(v) => {
                DateTime::from_timestamp_millis(v.as_i64()).unwrap_or_else(Utc::now)
            }
            parquet::record::Field::Str(s) => {
                // ISO 格式字符串
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now())
            }
            _ => Utc::now(),
        }
    }

    /// 清理交易对名称
    fn clean_symbol(symbol: &str) -> String {
        symbol
            .trim_matches('"')
            .trim_matches('\'')
            .replace("Symbol(", "")
            .replace(")", "")
            .to_uppercase()
    }

    /// 获取文件信息（不加载数据）
    pub fn info(&self) -> Result<ParquetInfo, String> {
        if !Path::new(&self.path).exists() {
            return Err(format!("文件不存在: {}", self.path));
        }

        let file = std::fs::File::open(&self.path)
            .map_err(|e| format!("打开文件失败: {}", e))?;

        let reader = SerializedFileReader::new(file)
            .map_err(|e| format!("读取 parquet 失败: {}", e))?;

        let metadata = reader.metadata();
        let num_rows: usize = (0..metadata.num_row_groups())
            .map(|i| metadata.row_group(i).map(|rg| rg.num_rows()).unwrap_or(0) as usize)
            .sum();

        let columns: Vec<String> = (0..metadata.schema().get_columns().len())
            .filter_map(|i| metadata.schema().get_columns().get(i).map(|c| c.name().to_string()))
            .collect();

        Ok(ParquetInfo {
            path: self.path.clone(),
            num_rows,
            num_row_groups: metadata.num_row_groups(),
            columns,
        })
    }
}

/// Parquet 文件信息
#[derive(Debug, Clone)]
pub struct ParquetInfo {
    pub path: String,
    pub num_rows: usize,
    pub num_row_groups: usize,
    pub columns: Vec<String>,
}

impl std::fmt::Display for ParquetInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parquet: {} ({} rows, {} groups)\n  Columns: {}",
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
    fn test_loader_creation() {
        let loader = ParquetLoader::new("test.parquet");
        assert_eq!(loader.path, "test.parquet");
    }
}
