//! 接口层 DTO (Data Transfer Object)
//!
//! 定义跨模块交互的数据传输对象。
//!
//! 这些类型是接口契约的一部分，用于在模块间传递数据。

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use super::types::Side;

// ============================================================================
// CheckTable DTO
// ============================================================================

/// CheckTable 检查结果
#[derive(Debug, Clone)]
pub struct CheckTableResult {
    /// 检查是否通过
    pub passed: bool,
    /// 拒绝原因（如果未通过）
    pub reject_reason: Option<String>,
    /// 检查执行时间（毫秒）
    pub execution_time_ms: u64,
}

/// CheckTable 配置
#[derive(Debug, Clone)]
pub struct CheckTableConfig {
    /// 检查表标识
    pub id: String,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
}

// ============================================================================
// Strategy DTO
// ============================================================================

/// 交易信号方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDirection {
    Long,
    Short,
    Flat,
}

impl Default for SignalDirection {
    fn default() -> Self {
        Self::Flat
    }
}

/// 交易信号类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    Open,
    Add,
    Reduce,
    Close,
}

impl Default for SignalType {
    fn default() -> Self {
        Self::Close
    }
}

/// 交易信号
#[derive(Debug, Clone)]
pub struct TradingSignal {
    pub id: String,
    pub symbol: String,
    pub direction: SignalDirection,
    pub signal_type: SignalType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub priority: u8,
    pub confidence: u8,
    pub timestamp: DateTime<Utc>,
}

/// 策略状态
#[derive(Debug, Clone)]
pub struct StrategyState {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub position_direction: SignalDirection,
    pub position_qty: Decimal,
    pub status: StrategyStatus,
    pub last_signal_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyStatus {
    Idle,
    Running,
    Waiting,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketStatusType {
    Pin,
    Trend,
    Range,
}

// ============================================================================
// Risk DTO
// ============================================================================

/// 风控等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionDirection {
    Long,
    Short,
    NetLong,
    NetShort,
    Flat,
}

/// 持仓信息
#[derive(Debug, Clone)]
pub struct PositionInfo {
    pub symbol: String,
    pub direction: PositionDirection,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

/// 已执行订单信息
#[derive(Debug, Clone)]
pub struct ExecutedOrder {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub quantity: Decimal,
    pub price: Decimal,
    pub commission: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 风险警告
#[derive(Debug, Clone)]
pub struct RiskWarning {
    pub code: String,
    pub message: String,
    pub severity: RiskLevel,
    pub affected_symbol: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// 风控阈值
#[derive(Debug, Clone)]
pub struct RiskThresholds {
    pub max_exposure_ratio: Decimal,
    pub max_order_value: Decimal,
    pub max_position_ratio: Decimal,
    pub max_leverage: u8,
    pub min_order_value: Decimal,
    pub stop_loss_ratio: Decimal,
}

impl Default for RiskThresholds {
    fn default() -> Self {
        Self {
            max_exposure_ratio: Decimal::from(95) / Decimal::from(100),
            max_order_value: Decimal::from(1000),
            max_position_ratio: Decimal::from(20) / Decimal::from(100),
            max_leverage: 20,
            min_order_value: Decimal::from(10),
            stop_loss_ratio: Decimal::from(2) / Decimal::from(100),
        }
    }
}

impl RiskThresholds {
    pub fn production() -> Self {
        Self::default()
    }

    pub fn backtest() -> Self {
        Self {
            max_exposure_ratio: Decimal::from(80) / Decimal::from(100),
            max_order_value: Decimal::from(500),
            max_position_ratio: Decimal::from(15) / Decimal::from(100),
            max_leverage: 10,
            min_order_value: Decimal::from(5),
            stop_loss_ratio: Decimal::from(3) / Decimal::from(100),
        }
    }
}

// ============================================================================
// Execution DTO
// ============================================================================

/// 执行错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExecutionError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),

    #[error("Position limit exceeded: {0}")]
    PositionLimitExceeded(String),

    #[error("Order rejected: {0}")]
    OrderRejected(String),

    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Gateway error: {0}")]
    Gateway(String),
}
