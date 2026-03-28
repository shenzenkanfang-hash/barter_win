//! HistoryDataManager - 历史数据管理层
//!
//! 复制自 b_data_source::history::manager
//! 使用简化内存存储

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;

use super::provider::HistoryDataProvider;
use super::types::{DataIssue, HistoryError, HistoryResponse, KLine};

/// 最大内存K线缓存条数
pub const MAX_KLINE_ENTRIES: usize = 1000;

/// 历史数据管理器
pub struct HistoryDataManager {
    /// 内存缓存: symbol -> period -> Vec<KLine>
    klines: RwLock<HashMap<String, HashMap<String, Vec<KLine>>>>,
}

impl HistoryDataManager {
    /// 创建新实例
    pub fn new() -> Self {
        Self {
            klines: RwLock::new(HashMap::new()),
        }
    }

    /// 更新实时K线
    pub async fn update_realtime_kline(
        &self,
        symbol: &str,
        period: &str,
        kline: KLine,
        is_closed: bool,
    ) -> Result<(), HistoryError> {
        let mut klines = self.klines.write();

        let symbol_map = klines.entry(symbol.to_string()).or_insert_with(HashMap::new);
        let period_vec = symbol_map.entry(period.to_string()).or_insert_with(Vec::new);

        if is_closed {
            period_vec.push(kline);
            while period_vec.len() > MAX_KLINE_ENTRIES {
                period_vec.remove(0);
            }
        } else {
            if let Some(last) = period_vec.last_mut() {
                *last = kline;
            } else {
                period_vec.push(kline);
            }
        }

        Ok(())
    }

    /// 查询历史数据
    pub async fn query_history(
        &self,
        symbol: &str,
        period: &str,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError> {
        let klines = self.klines.read();

        let klines_vec = klines
            .get(symbol)
            .and_then(|p| p.get(period))
            .cloned()
            .unwrap_or_default();

        let filtered: Vec<KLine> = klines_vec
            .into_iter()
            .filter(|k| k.timestamp <= end_time)
            .rev()
            .take(limit as usize)
            .collect();

        Ok(HistoryResponse {
            symbol: symbol.to_string(),
            period: period.to_string(),
            klines: filtered,
            current: None,
            has_more: false,
            source: super::types::DataSource::Memory,
        })
    }

    /// 获取数据状态
    pub async fn get_data_status(&self) -> Vec<DataIssue> {
        Vec::new()
    }
}

impl Default for HistoryDataManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HistoryDataProvider for HistoryDataManager {
    async fn update_realtime_kline(
        &self,
        symbol: &str,
        period: &str,
        kline: KLine,
        is_closed: bool,
    ) -> Result<(), HistoryError> {
        HistoryDataManager::update_realtime_kline(self, symbol, period, kline, is_closed).await
    }

    async fn query_history(
        &self,
        symbol: &str,
        period: &str,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError> {
        HistoryDataManager::query_history(self, symbol, period, end_time, limit).await
    }

    async fn get_data_status(&self) -> Vec<DataIssue> {
        HistoryDataManager::get_data_status(self).await
    }
}
