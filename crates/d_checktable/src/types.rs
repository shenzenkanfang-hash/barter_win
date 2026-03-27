#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

// ==================== 公共枚举 ====================

/// 市场状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    TREND,   // 趋势状态
    RANGE,   // 震荡状态
    PIN,     // 插针状态
    INVALID, // 数据无效
}

impl Default for MarketStatus {
    fn default() -> Self {
        MarketStatus::TREND
    }
}

/// 波动率等级（从 a_common 统一导入）
pub use a_common::models::market_data::VolatilityTier;

/// 策略层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyLevel {
    MIN,  // 分钟级策略
    DAY,  // 日线级策略
}

/// 持仓方向（从 a_common 导入，禁止本地定义）
pub use a_common::models::types::PositionSide;

/// 交易动作（从 a_common 导入，禁止本地定义）
pub use a_common::models::types::TradingAction;

// ==================== min/ 类型 ====================

/// 分钟级市场状态输入
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinMarketStatusInput {
    pub tr_ratio_10min: Decimal,
    pub tr_ratio_15min: Decimal,
    pub price_position: Decimal,
    pub zscore: Decimal,
    pub tr_base_60min: Decimal,
}

/// 分钟级市场状态输出
#[derive(Debug, Clone, Default)]
pub struct MinMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_tier: VolatilityTier,
    pub high_volatility_reason: Option<String>,
}

/// 分钟级信号输入
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinSignalInput {
    pub tr_base_60min: Decimal,
    pub tr_ratio_15min: Decimal,
    pub zscore_14_1m: Decimal,
    pub zscore_1h_1m: Decimal,
    pub tr_ratio_60min_5h: Decimal,
    pub tr_ratio_10min_1h: Decimal,
    pub pos_norm_60: Decimal,
    pub acc_percentile_1h: Decimal,
    pub velocity_percentile_1h: Decimal,
    pub pine_bg_color: String,
    pub pine_bar_color: String,
    pub price_deviation: Decimal,
    pub price_deviation_horizontal_position: Decimal,
}

impl MinSignalInput {
    pub fn new() -> Self {
        Self {
            tr_base_60min: Decimal::ZERO,
            tr_ratio_15min: Decimal::ZERO,
            zscore_14_1m: Decimal::ZERO,
            zscore_1h_1m: Decimal::ZERO,
            tr_ratio_60min_5h: Decimal::ZERO,
            tr_ratio_10min_1h: Decimal::ZERO,
            pos_norm_60: dec!(50),
            acc_percentile_1h: Decimal::ZERO,
            velocity_percentile_1h: Decimal::ZERO,
            pine_bg_color: String::new(),
            pine_bar_color: String::new(),
            price_deviation: Decimal::ZERO,
            price_deviation_horizontal_position: dec!(50),
        }
    }
}

/// 分钟级信号输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinSignalOutput {
    pub long_entry: bool,
    pub short_entry: bool,
    pub long_exit: bool,
    pub short_exit: bool,
    pub long_hedge: bool,
    pub short_hedge: bool,
    pub exit_high_volatility: bool,
}

impl Default for MinSignalOutput {
    fn default() -> Self {
        Self {
            long_entry: false,
            short_entry: false,
            long_exit: false,
            short_exit: false,
            long_hedge: false,
            short_hedge: false,
            exit_high_volatility: false,
        }
    }
}

// ==================== day/ 类型 ====================

/// 日线级市场状态输入
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayMarketStatusInput {
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub pine_color: String,
    pub ma5_in_20d_ma5_pos: Decimal,
    pub power_percentile: Decimal,
}

/// 日线级市场状态输出
#[derive(Debug, Clone, Default)]
pub struct DayMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_tier: VolatilityTier,
}

/// 日线级信号输入
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaySignalInput {
    pub pine_bar_color_12_26: String,
    pub pine_bg_color_12_26: String,
    pub pine_bar_color_20_50: String,
    pub pine_bg_color_20_50: String,
    pub pine_bar_color_100_200: String,
    pub pine_bg_color_100_200: String,
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub ma5_in_20d_ma5_pos: Decimal,
}

/// 日线级信号输出
#[derive(Debug, Clone)]
pub struct DaySignalOutput {
    pub long_entry: bool,
    pub short_entry: bool,
    pub long_exit: bool,
    pub short_exit: bool,
    pub long_hedge: bool,
    pub short_hedge: bool,
}

// ==================== PriceControl 类型 ====================

/// 价格控制输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceControlInput {
    pub position_entry_price: Decimal,
    pub position_side: PositionSide,
    pub position_size: Decimal,
    pub current_price: Decimal,
    pub profit_threshold: Decimal,
    pub loss_threshold: Decimal,
    pub add_threshold: Decimal,
    pub move_stop_threshold: Decimal,
}

/// 价格控制输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceControlOutput {
    pub should_add: bool,
    pub should_stop: bool,
    pub should_take_profit: bool,
    pub should_move_stop: bool,
    pub profit_distance_pct: Decimal,
    pub stop_distance_pct: Decimal,
}

// ==================== TradingTrigger 类型 ====================

/// 仓位状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckList {
    pub long_positions: Vec<PositionRecord>,
    pub short_positions: Vec<PositionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRecord {
    pub entry_price: Decimal,
    pub qty: Decimal,
}

/// 交易触发器输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingTriggerInput {
    pub symbol: String,
    pub current_price: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub min_indicators: MinSignalInput,
    pub day_indicators: DaySignalInput,
    pub check_list: CheckList,
}

/// 交易决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecision {
    pub action: TradingAction,
    pub reason: String,
    pub confidence: u8,
    pub level: StrategyLevel,
}

// ==================== CheckChain 类型 ====================

use x_data::trading::signal::{StrategyId, PositionRef};

/// 检查链上下文（双周期通用）
#[derive(Debug, Clone)]
pub struct CheckChainContext {
    /// 当前持仓数量
    pub current_position_qty: Decimal,
    /// 策略标识
    pub strategy_id: StrategyId,
    /// 仓位引用（加仓/平仓时必须）
    pub position_ref: Option<PositionRef>,
}

/// 检查信号枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSignal {
    Exit,   // 退出信号
    Close,  // 关仓信号
    Hedge,  // 对冲信号
    Add,    // 加仓信号
    Open,   // 开仓信号
}

/// 检查链结果
#[derive(Debug, Clone, Default)]
pub struct CheckChainResult {
    pub signals: Vec<CheckSignal>,
}

impl CheckChainResult {
    pub fn new() -> Self {
        Self { signals: Vec::new() }
    }

    /// 添加信号
    pub fn add_signal(&mut self, signal: CheckSignal) {
        self.signals.push(signal);
    }

    /// 检查是否有特定信号
    pub fn has(&self, signal: CheckSignal) -> bool {
        self.signals.contains(&signal)
    }

    /// 是否有任何信号
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}
