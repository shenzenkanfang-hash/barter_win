//! 状态管理 Trait 定义
//!
//! 提供统一的状态访问接口，支持只读视图和可写管理器。

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use std::sync::Arc;

use crate::position::snapshot::{UnifiedPositionSnapshot, PositionSnapshot};
use crate::account::types::AccountSnapshot;
use crate::trading::order::OrderRecord;
use crate::error::XDataError;

// ============================================================================
// StateViewer Trait (只读接口)
// ============================================================================

/// 状态视图 trait（只读接口）
/// 用于获取系统状态的快照，不提供修改能力
pub trait StateViewer: Send + Sync {
    /// 获取所有持仓快照
    fn get_positions(&self) -> Vec<UnifiedPositionSnapshot>;

    /// 获取账户快照
    fn get_account(&self) -> Option<AccountSnapshot>;

    /// 获取所有未完成订单
    fn get_open_orders(&self) -> Vec<OrderRecord>;
}

// ============================================================================
// StateManager Trait (可写接口)
// ============================================================================

/// 状态管理器 trait（可写接口）
/// 继承自 StateViewer，同时提供修改能力
pub trait StateManager: StateViewer {
    /// 更新持仓
    fn update_position(&self, symbol: &str, pos: PositionSnapshot) -> Result<(), XDataError>;

    /// 移除持仓
    fn remove_position(&self, symbol: &str) -> Result<(), XDataError>;

    /// 锁定持仓列表进行读取
    fn lock_positions_read(&self) -> Vec<UnifiedPositionSnapshot>;
}

// ============================================================================
// UnifiedStateView (统一状态视图)
// ============================================================================

/// 统一状态视图 - 组合多个 StateManager 提供一致的状态快照
///
/// 使用原子操作确保读取一致性。
pub struct UnifiedStateView {
    /// 持仓管理器
    position_manager: Arc<dyn StateManager>,
    /// 账户管理器
    account_pool: Arc<dyn StateManager>,
}

impl UnifiedStateView {
    /// 创建新的统一状态视图
    pub fn new(
        position_manager: Arc<dyn StateManager>,
        account_pool: Arc<dyn StateManager>,
    ) -> Self {
        Self {
            position_manager,
            account_pool,
        }
    }

    /// 获取统一状态快照
    ///
    /// 原子读取所有状态，确保数据一致性
    pub fn snapshot(&self) -> SystemSnapshot {
        let positions = self.position_manager.get_positions();
        let account = self.account_pool.get_account();
        SystemSnapshot {
            positions,
            account,
            timestamp: Utc::now(),
        }
    }

    /// 获取持仓管理器引用
    pub fn position_manager(&self) -> &Arc<dyn StateManager> {
        &self.position_manager
    }

    /// 获取账户管理器引用
    pub fn account_pool(&self) -> &Arc<dyn StateManager> {
        &self.account_pool
    }
}

// ============================================================================
// SystemSnapshot (系统完整快照)
// ============================================================================

/// 系统完整快照
///
/// 包含某一时刻的所有系统状态
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    /// 持仓快照列表
    pub positions: Vec<UnifiedPositionSnapshot>,
    /// 账户快照
    pub account: Option<AccountSnapshot>,
    /// 快照时间戳
    pub timestamp: DateTime<Utc>,
}

impl SystemSnapshot {
    /// 创建新的系统快照
    pub fn new(
        positions: Vec<UnifiedPositionSnapshot>,
        account: Option<AccountSnapshot>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            positions,
            account,
            timestamp,
        }
    }

    /// 检查是否有任何持仓
    pub fn has_positions(&self) -> bool {
        !self.positions.is_empty()
    }

    /// 检查账户是否有效
    pub fn has_account(&self) -> bool {
        self.account.is_some()
    }
}
