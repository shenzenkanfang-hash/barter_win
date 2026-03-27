//! 风控接口（最小化）
//!
//! 仅导出沙盒测试需要的 trait 和类型。

use a_common::exchange::ExchangeAccount;
pub use crate::types::{OrderRequest, RiskCheckResult};

/// 从 a_common 导出风控相关类型
pub use a_common::models::dto::{
    ExecutedOrder, PositionInfo, RiskThresholds, RiskWarning,
};

/// 风控检查器 trait（sandbox 用）
pub trait RiskChecker: Send + Sync {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult;
    fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult;
    fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>;
    fn thresholds(&self) -> RiskThresholds;
}
