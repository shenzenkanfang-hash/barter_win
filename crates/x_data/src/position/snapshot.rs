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
