//! 账户类型定义

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// FundPool
// ============================================================================

/// 资金池
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundPool {
    /// 池名称
    pub name: String,
    /// 总分配资金
    pub allocated: Decimal,
    /// 已用资金
    pub used: Decimal,
    /// 冻结资金（等待成交）
    pub frozen: Decimal,
}

impl FundPool {
    pub fn new(name: &str, allocated: Decimal) -> Self {
        Self {
            name: name.to_string(),
            allocated,
            used: Decimal::ZERO,
            frozen: Decimal::ZERO,
        }
    }

    /// 可用资金
    pub fn available(&self) -> Decimal {
        self.allocated - self.used - self.frozen
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.available() <= Decimal::ZERO
    }
}

// ============================================================================
// AccountSnapshot
// ============================================================================

/// 账户快照（用于持久化和恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    /// 账户ID
    pub account_id: String,
    /// 账户Equity（净值）
    pub equity: Decimal,
    /// 可用资金
    pub available: Decimal,
    /// 冻结保证金
    pub frozen: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 更新时间（RFC3339格式）
    pub updated_at: String,
}
