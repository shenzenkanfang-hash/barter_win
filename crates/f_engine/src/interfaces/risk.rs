//! 风控接口
//!
//! 定义风控检查的统一接口。
//! 注意：RiskCheckResult 已移至 core::business_types::RiskCheckResult

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use a_common::models::types::{Side, OrderType as CommonOrderType};
use crate::core::RiskCheckResult;

/// 风控等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
}

/// 订单请求（风控接口契约）
///
/// 注意：这是风控层的接口契约，不是内部实现。
#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub order_type: ExtendedOrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
}

/// 扩展订单类型（包含 a_common 的基础类型）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendedOrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
}

impl From<ExtendedOrderType> for CommonOrderType {
    fn from(ext: ExtendedOrderType) -> Self {
        match ext {
            ExtendedOrderType::Market => CommonOrderType::Market,
            ExtendedOrderType::Limit => CommonOrderType::Limit,
            ExtendedOrderType::StopLoss => CommonOrderType::Limit, // 映射为 Limit
            ExtendedOrderType::TakeProfit => CommonOrderType::Limit, // 映射为 Limit
        }
    }
}

/// 风控检查器接口
///
/// 封装所有风控检查逻辑。
///
/// # 封装理由
/// 1. 风控是核心业务逻辑，必须独立封装
/// 2. 引擎下单前必须通过风控检查
/// 3. 不能直接在引擎中硬编码风控规则
///
/// # 检查类型
/// - 仓位限制检查
/// - 风险敞口检查
/// - 订单价值检查
/// - 余额检查
pub trait RiskChecker: Send + Sync {
    /// 预下单检查
    fn pre_check(&self, order: &OrderRequest, account: &AccountInfo) -> RiskCheckResult;

    /// 订单成交后检查
    fn post_check(&self, order: &ExecutedOrder, account: &AccountInfo) -> RiskCheckResult;

    /// 定期风险扫描
    fn scan(&self, positions: &[PositionInfo], account: &AccountInfo) -> Vec<RiskWarning>;

    /// 获取风控阈值
    fn thresholds(&self) -> RiskThresholds;
}

/// 账户信息（接口契约）
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub account_id: String,
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
}

/// 持仓信息（接口契约）
#[derive(Debug, Clone)]
pub struct PositionInfo {
    pub symbol: String,
    pub direction: PositionDirection,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionDirection {
    Long,
    Short,
    NetLong,
    NetShort,
    Flat,
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
    /// 最大账户风险敞口比例 (0.0 ~ 1.0)
    pub max_exposure_ratio: Decimal,
    /// 单笔订单最大价值
    pub max_order_value: Decimal,
    /// 单币种最大持仓比例
    pub max_position_ratio: Decimal,
    /// 最大杠杆倍数
    pub max_leverage: u8,
    /// 最小订单价值
    pub min_order_value: Decimal,
    /// 止损比例
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
