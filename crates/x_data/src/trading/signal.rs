//! 策略层→引擎层 统一信号结构
//!
//! 所有策略（分钟级/日线级）必须通过此模块与引擎层通信。

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
pub use crate::position::PositionSide;

// ============================================================================
// 策略标识
// ============================================================================

/// 策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyType {
    Trend,   // 趋势策略
    Pin,     // Pin因子策略
    Grid,    // 网格策略
}

/// 策略层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyLevel {
    Minute,  // 15分钟级
    Day,     // 日线级
}

/// 策略唯一标识
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyId {
    pub strategy_type: StrategyType,
    pub instance_id: String,
    pub level: StrategyLevel,
}

impl StrategyId {
    /// 创建分钟级趋势策略ID
    pub fn new_trend_minute(instance_id: impl Into<String>) -> Self {
        Self {
            strategy_type: StrategyType::Trend,
            instance_id: instance_id.into(),
            level: StrategyLevel::Minute,
        }
    }

    /// 创建日线级趋势策略ID
    pub fn new_trend_day(instance_id: impl Into<String>) -> Self {
        Self {
            strategy_type: StrategyType::Trend,
            instance_id: instance_id.into(),
            level: StrategyLevel::Day,
        }
    }

    /// 创建分钟级Pin策略ID
    pub fn new_pin_minute(instance_id: impl Into<String>) -> Self {
        Self {
            strategy_type: StrategyType::Pin,
            instance_id: instance_id.into(),
            level: StrategyLevel::Minute,
        }
    }

    /// 创建日线级Pin策略ID
    pub fn new_pin_day(instance_id: impl Into<String>) -> Self {
        Self {
            strategy_type: StrategyType::Pin,
            instance_id: instance_id.into(),
            level: StrategyLevel::Day,
        }
    }
}

// ============================================================================
// 交易指令
// ============================================================================

/// 交易指令类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeCommand {
    Open,           // 开仓
    Add,            // 加仓
    Reduce,         // 减仓（部分平仓）
    FlatAll,        // 全平
    FlatPosition,   // 指定仓位平仓
    HedgeOpen,      // 对冲开仓
    HedgeClose,     // 对冲平仓
}

// ============================================================================
// 仓位引用
// ============================================================================

/// 仓位引用（用于指定平仓）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRef {
    /// 仓位唯一ID
    pub position_id: String,
    /// 关联的策略实例ID
    pub strategy_instance_id: String,
    /// 持仓方向
    pub side: PositionSide,
}

// ============================================================================
// 策略信号（核心输出结构）
// ============================================================================

/// 策略信号（策略层 → 引擎层）
///
/// 这是策略层向引擎层传递信息的唯一结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySignal {
    /// 交易指令
    pub command: TradeCommand,
    /// 交易方向
    pub direction: PositionSide,
    /// 交易数量（策略层计算）
    pub quantity: Decimal,
    /// 目标价格
    pub target_price: Decimal,
    /// 策略标识
    pub strategy_id: StrategyId,
    /// 仓位引用（加仓/平仓时必须）
    pub position_ref: Option<PositionRef>,
    /// 是否全平（true=全部平掉）
    pub full_close: bool,
    /// 止损价格（可选）
    pub stop_loss_price: Option<Decimal>,
    /// 止盈价格（可选）
    pub take_profit_price: Option<Decimal>,
    /// 执行原因
    pub reason: String,
    /// 置信度 0-100
    pub confidence: u8,
    /// 触发时间戳
    pub timestamp: i64,
}

impl StrategySignal {
    /// 创建开仓信号
    pub fn open(
        direction: PositionSide,
        quantity: Decimal,
        target_price: Decimal,
        strategy_id: StrategyId,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::Open,
            direction,
            quantity,
            target_price,
            strategy_id,
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 80,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建加仓信号
    pub fn add(
        direction: PositionSide,
        quantity: Decimal,
        target_price: Decimal,
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::Add,
            direction,
            quantity,
            target_price,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 75,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建全平信号
    pub fn flat_all(
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::FlatAll,
            direction: position_ref.side,
            quantity: Decimal::ZERO,
            target_price: Decimal::ZERO,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: true,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 90,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建指定仓位平仓信号
    pub fn flat_position(
        quantity: Decimal,
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::FlatPosition,
            direction: position_ref.side,
            quantity,
            target_price: Decimal::ZERO,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 85,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建减仓信号
    pub fn reduce(
        quantity: Decimal,
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::Reduce,
            direction: position_ref.side,
            quantity,
            target_price: Decimal::ZERO,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 80,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}
