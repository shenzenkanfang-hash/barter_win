//! 策略接口
//!
//! 定义策略执行和信号生成的统一接口。
//! 确保策略模块与其他模块通过接口交互。

use crate::interfaces::market_data::{MarketKLine, MarketTick, VolatilityInfo};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::sync::Arc;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDirection {
    Long,
    Short,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    Open,
    Add,
    Reduce,
    Close,
}

impl Default for SignalDirection {
    fn default() -> Self {
        Self::Flat
    }
}

impl Default for SignalType {
    fn default() -> Self {
        Self::Close
    }
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

/// 策略实例接口
///
/// 所有策略必须实现此接口。
///
/// # 封装理由
/// 1. 策略是独立模块，不能直接访问引擎内部
/// 2. 策略通过接口获取市场数据
/// 3. 策略生成信号后通过接口提交
///
/// # 设计约束
/// - 所有方法使用 &self 保证线程安全
/// - 状态变更通过内部状态机管理
pub trait StrategyInstance: Send + Sync {
    /// 获取策略 ID
    fn id(&self) -> &str;

    /// 获取策略名称
    fn name(&self) -> &str;

    /// 获取关注的品种列表
    fn symbols(&self) -> Vec<String>;

    /// 检查策略是否启用
    fn is_enabled(&self) -> bool;

    /// 获取策略当前状态
    fn state(&self) -> StrategyState;

    /// 处理 K 线数据
    ///
    /// 注意：使用接口契约类型，不依赖具体实现
    fn on_bar(&self, bar: &MarketKLine) -> Option<TradingSignal>;

    /// 处理 Tick 数据（可选）
    fn on_tick(&self, tick: &MarketTick) -> Option<TradingSignal> {
        let _ = tick;
        None
    }

    /// 处理波动率变化
    fn on_volatility_change(&self, volatility: &VolatilityInfo);

    /// 设置启用状态
    fn set_enabled(&self, enabled: bool);

    /// 更新市场状态
    fn update_market_status(&self, status: MarketStatusType);

    /// 获取市场状态
    fn market_status(&self) -> Option<MarketStatusType>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketStatusType {
    Pin,
    Trend,
    Range,
}

/// 策略工厂接口
///
/// 用于动态创建策略实例。
pub trait StrategyFactory: Send + Sync {
    /// 创建策略实例
    fn create(&self) -> Arc<dyn StrategyInstance>;

    /// 克隆工厂
    fn clone_box(&self) -> Box<dyn StrategyFactory>;
}

impl Clone for Box<dyn StrategyFactory> {
    fn clone(&self) -> Box<dyn StrategyFactory> {
        self.clone_box()
    }
}

/// 策略执行器接口
///
/// 封装策略调度、信号聚合等逻辑。
///
/// # 封装理由
/// 1. 引擎不能直接操作策略内部状态
/// 2. 信号聚合逻辑封装在执行器内部
/// 3. 策略注册/注销通过接口完成
pub trait StrategyExecutor: Send + Sync {
    /// 注册策略
    fn register(&self, strategy: Arc<dyn StrategyInstance>);

    /// 注销策略
    fn unregister(&self, strategy_id: &str);

    /// 分发 K 线到对应策略
    fn dispatch(&self, bar: &MarketKLine) -> Vec<TradingSignal>;

    /// 获取指定品种的最高优先级信号
    fn get_signal(&self, symbol: &str) -> Option<TradingSignal>;

    /// 获取策略状态
    fn get_strategy_state(&self, strategy_id: &str) -> Option<StrategyState>;

    /// 设置策略启用状态
    fn set_enabled(&self, strategy_id: &str, enabled: bool);

    /// 获取所有策略状态
    fn get_all_states(&self) -> Vec<StrategyState>;

    /// 策略数量
    fn count(&self) -> usize;
}

/// 信号聚合器接口
///
/// 封装信号去重、优先级排序等逻辑。
pub trait SignalAggregator: Send + Sync {
    /// 聚合多个信号
    fn aggregate(&self, signals: Vec<TradingSignal>) -> Vec<TradingSignal>;

    /// 获取最大信号数量
    fn max_signals(&self) -> usize;
}
