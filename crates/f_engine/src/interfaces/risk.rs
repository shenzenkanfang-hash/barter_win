//! 风控接口
//!
//! 定义风控检查的统一接口。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use a_common::models::types::Side;
use a_common::ExchangeAccount;
use crate::core::RiskCheckResult;
use crate::types::OrderRequest;

/// 风控等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionDirection {
    Long,
    Short,
    NetLong,
    NetShort,
    Flat,
}

/// 持仓信息
#[derive(Debug, Clone)]
pub struct PositionInfo {
    pub symbol: String,
    pub direction: PositionDirection,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

/// 已执行订单信息
#[derive(Debug, Clone)]
pub struct ExecutedOrder {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub quantity: Decimal,
    pub price: Decimal,
    pub commission: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 风险警告
#[derive(Debug, Clone)]
pub struct RiskWarning {
    pub code: String,
    pub message: String,
    pub severity: RiskLevel,
    pub affected_symbol: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// 风控阈值
#[derive(Debug, Clone)]
pub struct RiskThresholds {
    pub max_exposure_ratio: Decimal,
    pub max_order_value: Decimal,
    pub max_position_ratio: Decimal,
    pub max_leverage: u8,
    pub min_order_value: Decimal,
    pub stop_loss_ratio: Decimal,
}

impl Default for RiskThresholds {
    fn default() -> Self {
        Self {
            max_exposure_ratio: Decimal::from(95) / Decimal::from(100),
            max_order_value: Decimal::from(1000),
            max_position_ratio: Decimal::from(20) / Decimal::from(100),
            max_leverage: 20,
            min_order_value: Decimal::from(10),
            stop_loss_ratio: Decimal::from(2) / Decimal::from(100),
        }
    }
}

impl RiskThresholds {
    pub fn production() -> Self {
        Self::default()
    }

    pub fn backtest() -> Self {
        Self {
            max_exposure_ratio: Decimal::from(80) / Decimal::from(100),
            max_order_value: Decimal::from(500),
            max_position_ratio: Decimal::from(15) / Decimal::from(100),
            max_leverage: 10,
            min_order_value: Decimal::from(5),
            stop_loss_ratio: Decimal::from(3) / Decimal::from(100),
        }
    }
}

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
