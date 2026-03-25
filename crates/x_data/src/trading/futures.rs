//! 合约数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// FuturesPosition
// ============================================================================

/// 合约持仓
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesPosition {
    /// 交易品种
    pub symbol: String,
    /// 持仓方向
    pub side: String,
    /// 持仓数量
    pub qty: Decimal,
    /// 开仓价格
    pub entry_price: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 保证金
    pub margin: Decimal,
}

// ============================================================================
// FuturesAccount
// ============================================================================

/// 合约账户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesAccount {
    /// 账户ID
    pub account_id: String,
    /// 总资产
    pub total_assets: Decimal,
    /// 可用资金
    pub available: Decimal,
    /// 保证金余额
    pub margin_balance: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 账户等级
    pub accountTier: String,
}

/// 合约账户数据（带原始字段名）
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesAccountData {
    pub accountId: String,
    pub totalMarginBalance: String,
    pub availableBalance: String,
    pub marginBalance: String,
    pub unrealizedProfit: String,
}
