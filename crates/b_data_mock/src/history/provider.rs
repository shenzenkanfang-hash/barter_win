//! HistoryDataProvider Trait - 历史数据提供者接口
//!
//! 复制自 b_data_source::history::provider

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::types::{DataIssue, HistoryError, HistoryResponse, KLine};

/// 历史数据提供者接口
///
/// 封装所有历史数据访问，其他模块不能直接访问内部实现。
#[async_trait]
pub trait HistoryDataProvider: Send + Sync {
    /// 更新实时K线
    async fn update_realtime_kline(
        &self,
        symbol: &str,
        period: &str,
        kline: KLine,
        is_closed: bool,
    ) -> Result<(), HistoryError>;

    /// 查询历史数据
    async fn query_history(
        &self,
        symbol: &str,
        period: &str,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError>;

    /// 获取当前数据状态
    async fn get_data_status(&self) -> Vec<DataIssue>;
}
