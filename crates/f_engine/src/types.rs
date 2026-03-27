//! f_engine 核心类型定义
//!
//! # 精简后保留
//! - `StrategyId` - 策略标识符
//! - `TradingDecision` - 交易决策
//! - `OrderRequest` - 订单请求
//! - `TaskState` / `RunningStatus` - sandbox_main 任务状态
//! - `RiskCheckResult` - mock_api 风控结果
//! - `Side`, `OrderType`, `TradingAction` - 来自 a_common 的类型重导出

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// 从 a_common 导入（权威类型位置）
// ============================================================================

/// TradingAction 交易动作
pub use a_common::models::types::TradingAction;

// ============================================================================
// 策略标识符
// ============================================================================

/// 策略 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(pub String);

impl StrategyId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Default for StrategyId {
    fn default() -> Self {
        Self("main".to_string())
    }
}

impl std::fmt::Display for StrategyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// 交易决策
// ============================================================================

/// 交易决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecision {
    pub action: TradingAction,
    pub reason: String,
    pub confidence: u8,
    pub symbol: String,
    pub qty: Decimal,
    pub price: Decimal,
    /// 信号生成时间戳 (秒)
    pub timestamp: i64,
}

impl TradingDecision {
    pub fn new(
        action: TradingAction,
        reason: impl Into<String>,
        confidence: u8,
        symbol: String,
        qty: Decimal,
        price: Decimal,
        timestamp: i64,
    ) -> Self {
        Self {
            action,
            reason: reason.into(),
            confidence,
            symbol,
            qty,
            price,
            timestamp,
        }
    }

    pub fn is_exit(&self) -> bool {
        matches!(self.action, TradingAction::Flat)
    }

    pub fn is_entry(&self) -> bool {
        matches!(self.action, TradingAction::Long | TradingAction::Short)
    }
}

// ============================================================================
// 订单相关类型 (来自 a_common)
// ============================================================================

/// Side 用于订单方向
pub use a_common::models::types::Side;

/// OrderType 订单类型
pub use a_common::models::types::OrderType;

/// OrderRequest 订单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
}

impl OrderRequest {
    pub fn new_market(symbol: String, side: Side, qty: Decimal) -> Self {
        Self {
            symbol,
            side,
            order_type: OrderType::Market,
            qty,
            price: None,
        }
    }

    pub fn new_limit(symbol: String, side: Side, qty: Decimal, price: Decimal) -> Self {
        Self {
            symbol,
            side,
            order_type: OrderType::Limit,
            qty,
            price: Some(price),
        }
    }
}

// ============================================================================
// 沙箱任务状态（sandbox_main 用）
// ============================================================================

/// 任务运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningStatus {
    Running,
    Stopped,
    Ended,
}

impl Default for RunningStatus {
    fn default() -> Self {
        RunningStatus::Stopped
    }
}

/// 任务状态（sandbox TradeManager 用）
///
/// 精简版：心跳 + 禁止状态 + 结束状态
#[derive(Debug, Clone)]
pub struct TaskState {
    /// 品种
    pub symbol: String,
    /// 运行状态
    pub status: RunningStatus,
    /// 最后心跳时间（Unix 秒）
    pub last_beat: i64,
    /// 禁止交易截止时间（Unix 秒）
    pub forbid_until: Option<i64>,
    /// 禁止原因
    pub forbid_reason: Option<String>,
    /// 结束原因
    pub done_reason: Option<String>,
}

impl TaskState {
    pub fn new(symbol: String, _interval_ms: u64) -> Self {
        Self {
            symbol,
            status: RunningStatus::Running,
            last_beat: chrono::Utc::now().timestamp(),
            forbid_until: None,
            forbid_reason: None,
            done_reason: None,
        }
    }

    /// 是否被禁止
    pub fn is_forbidden(&self) -> bool {
        if let Some(ts) = self.forbid_until {
            chrono::Utc::now().timestamp() < ts
        } else {
            false
        }
    }

    /// 更新心跳
    pub fn heartbeat(&mut self) {
        self.last_beat = chrono::Utc::now().timestamp();
    }

    /// 结束任务
    pub fn end(&mut self, reason: String) {
        self.status = RunningStatus::Ended;
        self.done_reason = Some(reason);
    }
}

// ============================================================================
// 风控结果（mock_api 用）
// ============================================================================

/// 风控检查结果
#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    /// 是否通过
    pub passed: bool,
    /// 是否通过二次检查
    pub secondary_passed: bool,
}

impl RiskCheckResult {
    pub fn new(passed: bool, secondary_passed: bool) -> Self {
        Self { passed, secondary_passed }
    }

    pub fn pre_failed(&self) -> bool {
        !self.passed
    }
}
