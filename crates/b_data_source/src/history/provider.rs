//! HistoryDataProvider Trait - 历史数据提供者接口
//!
//! 定义统一的历史数据访问接口，供指标层和其他模块使用

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::types::{DataIssue, HistoryError, HistoryResponse, KLine};

/// 历史数据提供者接口
///
/// 封装所有历史数据访问，其他模块不能直接访问内部实现。
///
/// 实现者可以是：
/// - HistoryDataManager (实时管理)
/// - MockHistoryProvider (测试)
/// - ReplayHistoryProvider (回放)
#[async_trait]
pub trait HistoryDataProvider: Send + Sync {
    /// 更新实时K线（WS调用）
    ///
    /// # Arguments
    /// * `symbol` - 交易对符号
    /// * `period` - 周期，如 "1m", "1d"
    /// * `kline` - K线数据
    /// * `is_closed` - 是否已闭合
    async fn update_realtime_kline(
        &self,
        symbol: &str,
        period: &str,
        kline: KLine,
        is_closed: bool,
    ) -> Result<(), HistoryError>;

    /// 查询历史数据（返回闭合K线 + current）
    ///
    /// # Arguments
    /// * `symbol` - 交易对符号
    /// * `period` - 周期
    /// * `end_time` - 截止时间
    /// * `limit` - 需要条数
    async fn query_history(
        &self,
        symbol: &str,
        period: &str,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError>;

    /// 获取历史数据用于指标初始化
    ///
    /// 专门供指标层初始化时调用，返回完整的历史数据
    async fn get_history_for_indicator(
        &self,
        symbol: &str,
        period: &str,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError>;

    /// 报告数据异常（触发自愈）
    ///
    /// 指标层发现数据问题后调用此方法
    async fn report_issue(
        &self,
        symbol: &str,
        period: &str,
        issue: DataIssue,
    ) -> Result<(), HistoryError>;

    /// 获取指定品种的当前未闭合K线
    async fn get_current_kline(
        &self,
        symbol: &str,
        period: &str,
    ) -> Result<Option<KLine>, HistoryError>;

    /// 检查数据完整性
    async fn check_integrity(
        &self,
        symbol: &str,
        period: &str,
    ) -> Result<bool, HistoryError>;
}
