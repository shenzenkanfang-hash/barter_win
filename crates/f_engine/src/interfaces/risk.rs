//! 风控接口
//!
//! 定义风控检查的统一接口。

use a_common::ExchangeAccount;
use crate::types::OrderRequest;

// Re-export DTO from a_common
pub use a_common::models::dto::{
    RiskLevel, PositionDirection, PositionInfo, ExecutedOrder,
    RiskWarning, RiskThresholds,
};

use crate::core::RiskCheckResult;

/// 风控检查器接口
///
/// 封装所有风控检查逻辑。
pub trait RiskChecker: Send + Sync {
    /// 预下单检查
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult;

    /// 订单成交后检查
    fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult;

    /// 定期风险扫描
    fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>;

    /// 获取风控阈值
    fn thresholds(&self) -> RiskThresholds;
}
