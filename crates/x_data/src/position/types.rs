//! 持仓类型定义

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// ============================================================================
// PositionDirection
// ============================================================================

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionDirection {
    /// 多头
    Long,
    /// 空头
    Short,
    /// 净多
    NetLong,
    /// 净空
    NetShort,
    /// 无持仓（平仓）
    Flat,
}

impl PositionDirection {
    pub fn is_long(&self) -> bool {
        matches!(self, PositionDirection::Long | PositionDirection::NetLong)
    }

    pub fn is_short(&self) -> bool {
        matches!(self, PositionDirection::Short | PositionDirection::NetShort)
    }

    pub fn is_flat(&self) -> bool {
        matches!(self, PositionDirection::Flat)
    }
}

// ============================================================================
// PositionSide
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
// LocalPosition
// ============================================================================

/// 本地持仓（运行时持仓信息）
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
    /// 开仓时间戳
    pub open_time: i64,
    /// 持仓费用（开仓手续费 + 资金费率）
    pub position_cost: Decimal,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 仓位唯一ID（用于指定平仓）
    pub position_id: String,
    /// 关联的策略实例ID
    pub strategy_instance_id: String,
}

impl LocalPosition {
    pub fn new(
        symbol: String,
        direction: PositionDirection,
        qty: Decimal,
        avg_price: Decimal,
        strategy_instance_id: String,
    ) -> Self {
        let position_id = Self::generate_position_id(&symbol, &direction, &strategy_instance_id);
        Self {
            symbol,
            direction,
            qty,
            avg_price,
            open_time: Utc::now().timestamp(),
            position_cost: Decimal::ZERO,
            updated_at: Utc::now(),
            position_id,
            strategy_instance_id,
        }
    }

    /// 生成唯一仓位ID
    fn generate_position_id(symbol: &str, direction: &PositionDirection, strategy_instance_id: &str) -> String {
        let direction_str = match direction {
            PositionDirection::Long => "long",
            PositionDirection::Short => "short",
            PositionDirection::NetLong => "netlong",
            PositionDirection::NetShort => "netshort",
            PositionDirection::Flat => "flat",
        };
        format!(
            "{}_{}_{}_{}",
            symbol.to_lowercase(),
            direction_str,
            strategy_instance_id,
            Utc::now().timestamp()
        )
    }

    /// 计算未实现盈亏
    pub fn unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        match self.direction {
            PositionDirection::Long => (current_price - self.avg_price) * self.qty,
            PositionDirection::Short => (self.avg_price - current_price) * self.qty,
            _ => Decimal::ZERO,
        }
    }

    /// 名义价值
    pub fn notional_value(&self, price: Decimal) -> Decimal {
        self.qty * price
    }
}
