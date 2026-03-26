//! Strategy 模块
//!
//! 提供策略接口定义和调度器。

pub mod executor;
pub mod trader_manager;

pub use executor::{SignalAggregator, StrategyExecutor};
pub use trader_manager::{TraderManager, StrategyType, TraderError};

use a_common::models::market_data::VolatilityTier;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

/// 内部同步状态（使用 RwLock）
struct InternalState {
    enabled: RwLock<bool>,
    market_status: RwLock<Option<MarketStatus>>,
    volatility: RwLock<Option<f64>>,
}

impl InternalState {
    fn new() -> Self {
        Self {
            enabled: RwLock::new(true),
            market_status: RwLock::new(None),
            volatility: RwLock::new(None),
        }
    }
}

/// 策略状态（线程安全）
///
/// 设计说明：
/// - 内部使用 Arc<InternalState> 提供线程安全的可变状态
/// - Clone 只复制元数据，共享内部状态
/// - 可通过 Arc 共享给多个线程
#[derive(Clone)]
pub struct StrategyState {
    pub id: String,
    pub enabled: bool,
    pub position_direction: Direction,
    pub position_qty: Decimal,
    pub status: StrategyStatus,
    _internal: Arc<InternalState>,
}

impl StrategyState {
    pub fn new(id: String) -> Self {
        Self {
            id: id.clone(),
            enabled: true,
            position_direction: Direction::Flat,
            position_qty: Decimal::ZERO,
            status: StrategyStatus::Idle,
            _internal: Arc::new(InternalState::new()),
        }
    }

    /// 设置启用状态（线程安全）
    pub fn set_enabled(&self, enabled: bool) {
        *self._internal.enabled.write() = enabled;
    }

    /// 获取启用状态
    pub fn is_enabled(&self) -> bool {
        *self._internal.enabled.read()
    }

    /// 更新市场状态（线程安全）
    pub fn update_market_status(&self, status: MarketStatus) {
        *self._internal.market_status.write() = Some(status);
    }

    /// 获取市场状态
    pub fn market_status(&self) -> Option<MarketStatus> {
        self._internal.market_status.read().clone()
    }

    /// 更新波动率（线程安全）
    pub fn update_volatility(&self, volatility: f64) {
        *self._internal.volatility.write() = Some(volatility);
    }

    /// 获取波动率
    pub fn volatility(&self) -> Option<f64> {
        *self._internal.volatility.read()
    }
}

impl Debug for StrategyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrategyState")
            .field("id", &self.id)
            .field("enabled", &self.enabled)
            .field("position_direction", &self.position_direction)
            .field("position_qty", &self.position_qty)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum StrategyStatus {
    Idle,
    Running,
    Waiting,
    Error,
}

/// 交易方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Direction {
    Long,
    Short,
    Flat,
}

impl Default for Direction {
    fn default() -> Self {
        Self::Flat
    }
}

/// 信号类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSignal {
    pub symbol: String,
    pub direction: Direction,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub signal_type: SignalType,
    pub strategy_id: String,
    pub priority: u8,
    pub timestamp: DateTime<Utc>,
}

impl TradingSignal {
    pub fn new(symbol: String, direction: Direction, quantity: Decimal, strategy_id: String) -> Self {
        Self {
            symbol,
            direction,
            quantity,
            price: None,
            stop_loss: None,
            take_profit: None,
            signal_type: SignalType::Open,
            strategy_id,
            priority: 50,
            timestamp: Utc::now(),
        }
    }

    /// Builder 模式支持链式调用
    pub fn with_price(mut self, price: Decimal) -> Self {
        self.price = Some(price);
        self
    }

    pub fn with_stop_loss(mut self, stop_loss: Decimal) -> Self {
        self.stop_loss = Some(stop_loss);
        self
    }

    pub fn with_take_profit(mut self, take_profit: Decimal) -> Self {
        self.take_profit = Some(take_profit);
        self
    }

    pub fn with_signal_type(mut self, signal_type: SignalType) -> Self {
        self.signal_type = signal_type;
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn is_open(&self) -> bool {
        matches!(self.signal_type, SignalType::Open)
    }

    pub fn is_close(&self) -> bool {
        matches!(self.signal_type, SignalType::Close)
    }

    pub fn is_valid(&self) -> bool {
        self.quantity > Decimal::ZERO && self.direction != Direction::Flat
    }
}

/// K 线数据（策略输入）
#[derive(Debug, Clone)]
pub struct StrategyKLine {
    pub symbol: String,
    pub period: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
}

impl From<b_data_source::KLine> for StrategyKLine {
    fn from(k: b_data_source::KLine) -> Self {
        // 优化：使用静态字符串切片避免动态分配
        // 修复：对于非标准周期仍使用 format! 分配
        let period = match k.period {
            b_data_source::Period::Minute(m) => {
                match m {
                    1 => "1m".to_string(),
                    5 => "5m".to_string(),
                    15 => "15m".to_string(),
                    30 => "30m".to_string(),
                    60 => "1h".to_string(),
                    240 => "4h".to_string(),
                    _ => format!("{}m", m),  // 非标准周期需要动态分配
                }
            }
            b_data_source::Period::Day => "1d".to_string(),
        };
        Self {
            symbol: k.symbol,
            period,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            timestamp: k.timestamp,
        }
    }
}

/// 市场状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStatus {
    pub status: MarketStatusType,
    pub volatility: VolatilityTier,
    pub volatility_value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketStatusType {
    Pin,
    Trend,
    Range,
}

/// 策略接口
///
/// 注意：
/// 1. 所有方法使用 `&self` 保证线程安全
/// 2. 状态变更通过 StrategyState 内部的 RwLock 实现
/// 3. 策略实现者应持有 StrategyState 的 Arc 引用
pub trait Strategy: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    
    /// 策略关注的交易品种列表
    fn symbols(&self) -> Vec<String>;
    
    /// 检查策略是否启用
    fn is_enabled(&self) -> bool {
        self.state().is_enabled()
    }
    
    /// 核心方法：处理 K 线并返回交易信号
    fn on_bar(&self, bar: &StrategyKLine) -> Option<TradingSignal>;
    
    /// 获取策略状态引用
    fn state(&self) -> &StrategyState;
    
    /// 处理市场状态变化（使用内部状态，无需 &mut self）
    fn on_market_status(&self, status: &MarketStatus) {
        self.state().update_market_status(status.clone());
    }
    
    /// 处理波动率变化（使用内部状态，无需 &mut self）
    fn on_volatility(&self, volatility: f64) {
        self.state().update_volatility(volatility);
    }
}

/// 策略工厂 trait
pub trait StrategyFactory: Send + Sync {
    fn create(&self) -> Box<dyn Strategy>;
    fn clone_box(&self) -> Box<dyn StrategyFactory>;
}

impl Clone for Box<dyn StrategyFactory> {
    fn clone(&self) -> Box<dyn StrategyFactory> {
        self.clone_box()
    }
}
