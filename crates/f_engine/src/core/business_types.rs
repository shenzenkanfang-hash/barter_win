//! f_engine 业务数据类型
//!
//! 本模块定义引擎与策略、风控之间的接口数据类型。
//!
//! # 设计原则
//! - 层内直接调用，使用具体结构体
//! - 层间通过 pub 函数传递数据
//! - 字段私有化，对外暴露方法

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// 持仓方向
// ============================================================================

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    /// 无持仓
    NONE,
    /// 多头
    LONG,
    /// 空头
    SHORT,
}

impl Default for PositionSide {
    fn default() -> Self {
        PositionSide::NONE
    }
}

impl PositionSide {
    pub fn is_long(&self) -> bool {
        matches!(self, PositionSide::LONG)
    }

    pub fn is_short(&self) -> bool {
        matches!(self, PositionSide::SHORT)
    }

    pub fn is_flat(&self) -> bool {
        matches!(self, PositionSide::NONE)
    }
}

// ============================================================================
// 波动率等级
// ============================================================================

/// 波动率等级（业务层定义）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilityTier {
    /// 低波动
    Low,
    /// 中波动
    Medium,
    /// 高波动
    High,
    /// 极端波动
    Extreme,
}

impl Default for VolatilityTier {
    fn default() -> Self {
        VolatilityTier::Low
    }
}

impl VolatilityTier {
    pub fn from_ratio(ratio: Decimal) -> Self {
        if ratio < Decimal::from(5) {
            VolatilityTier::Low
        } else if ratio < Decimal::from(10) {
            VolatilityTier::Medium
        } else if ratio < Decimal::from(20) {
            VolatilityTier::High
        } else {
            VolatilityTier::Extreme
        }
    }
}

// ============================================================================
// 账户风险状态
// ============================================================================

/// 账户风险状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskState {
    /// 正常
    Normal,
    /// 警告
    Warning,
    /// 风险
    Risky,
    /// 禁止交易
    Forbidden,
}

impl Default for RiskState {
    fn default() -> Self {
        RiskState::Normal
    }
}

impl RiskState {
    pub fn can_trade(&self) -> bool {
        matches!(self, RiskState::Normal | RiskState::Warning)
    }
}

// ============================================================================
// 通道类型
// ============================================================================

/// 执行通道类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelType {
    /// 高速通道（分钟级专用）
    HighSpeed,
    /// 低速通道（分钟级 + 日线级）
    LowSpeed,
}

impl Default for ChannelType {
    fn default() -> Self {
        ChannelType::LowSpeed
    }
}

impl ChannelType {
    pub fn is_high_speed(&self) -> bool {
        matches!(self, ChannelType::HighSpeed)
    }

    pub fn is_low_speed(&self) -> bool {
        matches!(self, ChannelType::LowSpeed)
    }
}

// ============================================================================
// 引擎 → 策略：查询请求
// ============================================================================

/// 引擎传递给策略的查询请求
///
/// # 用途
/// 引擎在每个交易周期开始时，向策略查询当前市场状态下的交易决策。
///
/// # 包含信息
/// - 公共信息（账户、持仓、市场状态）
/// - 策略不需要知道的信息由引擎内部处理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyQuery {
    /// 当前时间戳
    pub timestamp: i64,
    /// 账户可用资金
    pub account_available: Decimal,
    /// 账户风险状态
    pub account_risk_state: RiskState,
    /// 当前价格
    pub current_price: Decimal,
    /// 波动率等级
    pub volatility_tier: VolatilityTier,
    /// 是否有持仓
    pub position_exists: bool,
    /// 持仓方向
    pub position_direction: PositionSide,
    /// 持仓数量
    pub position_qty: Decimal,
    /// 持仓均价
    pub position_entry_price: Decimal,
}

impl StrategyQuery {
    pub fn new(
        timestamp: i64,
        account_available: Decimal,
        account_risk_state: RiskState,
        current_price: Decimal,
        volatility_tier: VolatilityTier,
        position_exists: bool,
        position_direction: PositionSide,
        position_qty: Decimal,
        position_entry_price: Decimal,
    ) -> Self {
        Self {
            timestamp,
            account_available,
            account_risk_state,
            current_price,
            volatility_tier,
            position_exists,
            position_direction,
            position_qty,
            position_entry_price,
        }
    }

    /// 账户是否允许交易
    pub fn can_trade(&self) -> bool {
        self.account_risk_state.can_trade()
    }

    /// 是否有有效持仓
    pub fn has_valid_position(&self) -> bool {
        self.position_exists && self.position_qty > Decimal::ZERO
    }
}

// ============================================================================
// 策略 → 引擎：执行响应
// ============================================================================

/// 策略返回给引擎的执行响应
///
/// # 用途
/// 策略根据 Query 信息，生成交易决策返回给引擎。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyResponse {
    /// 是否执行
    pub should_execute: bool,
    /// 交易动作
    pub action: TradingAction,
    /// 仓位数量
    pub quantity: Decimal,
    /// 目标价格
    pub target_price: Decimal,
    /// 通道类型
    pub channel_type: ChannelType,
    /// 执行原因
    pub reason: String,
}

impl StrategyResponse {
    pub fn no_action(reason: impl Into<String>) -> Self {
        Self {
            should_execute: false,
            action: TradingAction::Flat,
            quantity: Decimal::ZERO,
            target_price: Decimal::ZERO,
            channel_type: ChannelType::LowSpeed,
            reason: reason.into(),
        }
    }

    pub fn execute(
        action: TradingAction,
        quantity: Decimal,
        target_price: Decimal,
        channel_type: ChannelType,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            should_execute: true,
            action,
            quantity,
            target_price,
            channel_type,
            reason: reason.into(),
        }
    }
}

// ============================================================================
// 风控检查结果（V1.4 定义）
// ============================================================================

/// 风控检查结果
///
/// # 设计
/// 引擎只关心两个 bool，不关心具体风控细节。
#[derive(Debug, Clone, Default)]
pub struct RiskCheckResult {
    /// 一次检查（锁外）是否通过
    pub pre_check_passed: bool,
    /// 二次检查（加锁后）是否通过
    pub lock_check_passed: bool,
}

impl RiskCheckResult {
    pub fn new(pre_passed: bool, lock_passed: bool) -> Self {
        Self {
            pre_check_passed: pre_passed,
            lock_check_passed: lock_passed,
        }
    }

    pub fn both_passed(&self) -> bool {
        self.pre_check_passed && self.lock_check_passed
    }

    pub fn pre_failed(&self) -> bool {
        !self.pre_check_passed
    }

    pub fn lock_failed(&self) -> bool {
        self.pre_check_passed && !self.lock_check_passed
    }
}

// ============================================================================
// 交易动作（从 c_data_process 导入）
// ============================================================================

pub use c_data_process::types::TradingAction;

// ============================================================================
// 价格控制输出（日线级）
// ============================================================================

/// 日线级价格控制输出
///
/// # 用途
/// 日线级触发器生成止盈/止损/加仓/移动止损指令。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PriceControlOutput {
    /// 是否加仓
    pub should_add: bool,
    /// 是否止损
    pub should_stop: bool,
    /// 是否止盈
    pub should_take_profit: bool,
    /// 是否移动止损
    pub should_move_stop: bool,
    /// 盈利距离百分比
    pub profit_distance_pct: Decimal,
    /// 止损距离百分比
    pub stop_distance_pct: Decimal,
}

impl PriceControlOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn any_action(&self) -> bool {
        self.should_add
            || self.should_stop
            || self.should_take_profit
            || self.should_move_stop
    }
}

// ============================================================================
// 订单生命周期
// ============================================================================

/// 订单生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderLifecycle {
    /// 订单已创建
    Created,
    /// 订单已发送
    Sent,
    /// 部分成交
    PartialFilled,
    /// 完全成交
    Filled,
    /// 已取消
    Cancelled,
    /// 失败
    Failed,
}

impl Default for OrderLifecycle {
    fn default() -> Self {
        OrderLifecycle::Created
    }
}

impl OrderLifecycle {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderLifecycle::Filled | OrderLifecycle::Cancelled | OrderLifecycle::Failed
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            OrderLifecycle::Created | OrderLifecycle::Sent | OrderLifecycle::PartialFilled
        )
    }

    pub fn next(&self) -> Self {
        match self {
            OrderLifecycle::Created => OrderLifecycle::Sent,
            OrderLifecycle::Sent => OrderLifecycle::PartialFilled,
            OrderLifecycle::PartialFilled => OrderLifecycle::Filled,
            OrderLifecycle::Filled => OrderLifecycle::Filled,
            OrderLifecycle::Cancelled => OrderLifecycle::Cancelled,
            OrderLifecycle::Failed => OrderLifecycle::Failed,
        }
    }
}

// ============================================================================
// 订单信息
// ============================================================================

/// 订单信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    /// 订单ID（全局唯一）
    pub order_id: String,
    /// 品种
    pub symbol: String,
    /// 动作
    pub action: TradingAction,
    /// 数量
    pub quantity: Decimal,
    /// 目标价格
    pub target_price: Decimal,
    /// 通道类型
    pub channel_type: ChannelType,
    /// 生命周期状态
    pub lifecycle: OrderLifecycle,
    /// 创建时间戳
    pub created_at: i64,
    /// 最后更新时间戳
    pub updated_at: i64,
    /// 重试次数
    pub retry_count: u8,
}

impl OrderInfo {
    pub fn new(
        order_id: String,
        symbol: String,
        action: TradingAction,
        quantity: Decimal,
        target_price: Decimal,
        channel_type: ChannelType,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            order_id,
            symbol,
            action,
            quantity,
            target_price,
            channel_type,
            lifecycle: OrderLifecycle::Created,
            created_at: now,
            updated_at: now,
            retry_count: 0,
        }
    }

    pub fn transition(&mut self, new_state: OrderLifecycle) {
        self.lifecycle = new_state;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

// ============================================================================
// 资金池
// ============================================================================

/// 资金池
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundPool {
    /// 池名称
    pub name: String,
    /// 分配额度
    pub allocated: Decimal,
    /// 已使用额度
    pub used: Decimal,
    /// 冻结额度
    pub frozen: Decimal,
}

impl FundPool {
    pub fn new(name: impl Into<String>, allocated: Decimal) -> Self {
        Self {
            name: name.into(),
            allocated,
            used: Decimal::ZERO,
            frozen: Decimal::ZERO,
        }
    }

    /// 可用额度
    pub fn available(&self) -> Decimal {
        self.allocated - self.used - self.frozen
    }

    /// 使用率
    pub fn usage_rate(&self) -> Decimal {
        if self.allocated.is_zero() {
            Decimal::ZERO
        } else {
            (self.used + self.frozen) / self.allocated
        }
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.available() <= Decimal::ZERO
    }

    /// 冻结额度
    pub fn freeze(&mut self, amount: Decimal) -> bool {
        if amount <= self.available() {
            self.frozen += amount;
            true
        } else {
            false
        }
    }

    /// 确认使用（从冻结转为已使用）
    pub fn confirm_usage(&mut self, amount: Decimal) {
        if amount <= self.frozen {
            self.frozen -= amount;
            self.used += amount;
        }
    }

    /// 释放冻结
    pub fn release_frozen(&mut self, amount: Decimal) {
        if amount <= self.frozen {
            self.frozen -= amount;
        }
    }

    /// 回滚（释放冻结并恢复）
    pub fn rollback(&mut self, amount: Decimal) {
        self.release_frozen(amount);
    }
}

// ============================================================================
// 错误码
// ============================================================================

/// 引擎错误码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineErrorCode {
    /// 品种已存在/互斥
    SymbolExists = 1001,
    /// 资金不足
    InsufficientFunds = 2001,
    /// 风控拒绝
    RiskRejected = 3001,
    /// 超时
    Timeout = 4001,
    /// 状态不一致
    StateInconsistent = 5001,
}

impl EngineErrorCode {
    pub fn code(&self) -> u16 {
        *self as u16
    }

    pub fn message(&self) -> &'static str {
        match self {
            EngineErrorCode::SymbolExists => "品种已存在/互斥",
            EngineErrorCode::InsufficientFunds => "资金不足",
            EngineErrorCode::RiskRejected => "风控拒绝",
            EngineErrorCode::Timeout => "超时",
            EngineErrorCode::StateInconsistent => "状态不一致",
        }
    }
}
