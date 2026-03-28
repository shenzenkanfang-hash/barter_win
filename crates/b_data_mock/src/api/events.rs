//! 账户事件（用于 AuditTick 广播）

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 账户事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountEvent {
    /// 余额更新
    BalanceUpdate {
        asset: String,
        balance_total: Decimal,
        balance_free: Decimal,
        balance_frozen: Decimal,
    },
    /// 订单成交
    OrderFilled {
        order_id: String,
        symbol: String,
        side: String,       // "Buy" 或 "Sell"
        qty: Decimal,
        price: Decimal,
        fee: Decimal,
        pnl: Decimal,      // 实现盈亏（如果平仓）
        time: DateTime<Utc>,
    },
    /// 持仓更新
    PositionUpdate {
        symbol: String,
        qty: Decimal,
        unrealized_pnl: Decimal,
    },
}

impl AccountEvent {
    pub fn time(&self) -> DateTime<Utc> {
        match self {
            Self::OrderFilled { time, .. } => *time,
            Self::BalanceUpdate { .. } => Utc::now(),
            Self::PositionUpdate { .. } => Utc::now(),
        }
    }
}
