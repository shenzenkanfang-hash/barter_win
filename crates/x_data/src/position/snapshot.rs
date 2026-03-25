//! 持仓快照类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// ============================================================================
// PositionSnapshot
// ============================================================================

/// 持仓快照（用于持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    /// 交易品种
    pub symbol: String,
    /// 多头数量
    pub long_qty: Decimal,
    /// 多头平均价格
    pub long_avg_price: Decimal,
    /// 空头数量
    pub short_qty: Decimal,
    /// 空头平均价格
    pub short_avg_price: Decimal,
    /// 更新时间（RFC3339格式）
    pub updated_at: String,
}

// ============================================================================
// Positions
// ============================================================================

/// 持仓列表（统一管理）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Positions {
    /// 持仓列表
    pub positions: Vec<PositionSnapshot>,
    /// 更新时间
    pub updated_at: String,
}

// ============================================================================
// UnifiedPositionSnapshot
// ============================================================================

/// 恢复源优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RecoveryPriority {
    /// 第一优先级：SQLite 本地数据库
    Sqlite = 1,
    /// 第二优先级：内存盘高速备份
    MemoryDisk = 2,
    /// 第三优先级：硬盘持久化备份
    HardDisk = 3,
}

/// 统一持仓快照（跨数据源）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedPositionSnapshot {
    /// 交易品种
    pub symbol: String,
    /// 多头数量
    pub long_qty: Decimal,
    /// 多头平均价格
    pub long_avg_price: Decimal,
    /// 空头数量
    pub short_qty: Decimal,
    /// 空头平均价格
    pub short_avg_price: Decimal,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 数据来源
    pub source: RecoveryPriority,
    /// 校验和
    pub checksum: u64,
}

impl UnifiedPositionSnapshot {
    /// 创建新的统一持仓快照
    pub fn new(
        symbol: String,
        long_qty: Decimal,
        long_avg_price: Decimal,
        short_qty: Decimal,
        short_avg_price: Decimal,
        source: RecoveryPriority,
    ) -> Self {
        Self {
            symbol,
            long_qty,
            long_avg_price,
            short_qty,
            short_avg_price,
            updated_at: Utc::now(),
            source,
            checksum: 0,
        }
    }

    /// 总持仓数量
    pub fn total_qty(&self) -> Decimal {
        self.long_qty + self.short_qty
    }

    /// 是否有多头持仓
    pub fn has_long(&self) -> bool {
        self.long_qty > Decimal::ZERO
    }

    /// 是否有空头持仓
    pub fn has_short(&self) -> bool {
        self.short_qty > Decimal::ZERO
    }

    /// 是否为空仓
    pub fn is_flat(&self) -> bool {
        self.long_qty == Decimal::ZERO && self.short_qty == Decimal::ZERO
    }
}
