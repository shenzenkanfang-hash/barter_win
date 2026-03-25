//! 持仓类型定义

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// ============================================================================
// 持仓方向
// ============================================================================

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionDirection {
    /// 多头
    Long,
    /// 空头
    Short,
    /// 无持仓（平仓）
    Flat,
}

impl PositionDirection {
    pub fn is_long(&self) -> bool {
        matches!(self, PositionDirection::Long)
    }

    pub fn is_short(&self) -> bool {
        matches!(self, PositionDirection::Short)
    }

    pub fn is_flat(&self) -> bool {
        matches!(self, PositionDirection::Flat)
    }
}

// ============================================================================
// 持仓边
// ============================================================================

/// 持仓边（用于区分同一仓位的多空方向）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    /// 多头边
    Long,
    /// 空头边
    Short,
    /// 两边都有
    Both,
    /// 无持仓
    None,
}

impl PositionSide {
    pub fn is_long(&self) -> bool {
        matches!(self, PositionSide::Long | PositionSide::Both)
    }

    pub fn is_short(&self) -> bool {
        matches!(self, PositionSide::Short | PositionSide::Both)
    }

    pub fn is_flat(&self) -> bool {
        matches!(self, PositionSide::None)
    }
}

// ============================================================================
// 本地持仓
// ============================================================================

/// 本地持仓（运行时单方向持仓）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPosition {
    /// 交易品种
    pub symbol: String,
    /// 持仓方向
    pub direction: PositionDirection,
    /// 持仓数量
    pub qty: Decimal,
    /// 平均价格
    pub avg_price: Decimal,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl LocalPosition {
    pub fn new(symbol: String, direction: PositionDirection, qty: Decimal, avg_price: Decimal) -> Self {
        Self {
            symbol,
            direction,
            qty,
            avg_price,
            updated_at: Utc::now(),
        }
    }
}
