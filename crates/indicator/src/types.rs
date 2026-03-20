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

/// 波动率等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilityLevel {
    HIGH,    // 高波动 (15min TR > 13%)
    NORMAL,  // 正常波动
    LOW,     // 低波动
}

impl Default for VolatilityLevel {
    fn default() -> Self {
        VolatilityLevel::NORMAL
    }
}

/// 策略层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyLevel {
    MIN,  // 分钟级策略
    DAY,  // 日线级策略
}

/// 交易动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingAction {
    Long,      // 做多
    Short,     // 做空
    Flat,      // 平仓
    Hedge,     // 对冲
    Wait,      // 等待
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    LONG,
    SHORT,
    NONE,
}

// ==================== min/ 输入输出类型 ====================

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
    pub volatility_level: VolatilityLevel,
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
#[derive(Debug, Clone)]
pub struct MinSignalOutput {
    pub long_entry: bool,
    pub short_entry: bool,
    pub long_exit: bool,
    pub short_exit: bool,
    pub long_hedge: bool,
    pub short_hedge: bool,
    pub exit_high_volatility: bool,
}

// ==================== day/ 输入输出类型 ====================

/// 日线级市场状态输入
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayMarketStatusInput {
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub pine_color: String,
    pub ma5_in_20d_ma5_pos: Decimal,
    pub power_percentile: Decimal,
}

impl DayMarketStatusInput {
    pub fn new() -> Self {
        Self {
            tr_ratio_5d_20d: Decimal::ZERO,
            tr_ratio_20d_60d: Decimal::ZERO,
            pine_color: String::new(),
            ma5_in_20d_ma5_pos: dec!(50),
            power_percentile: Decimal::ZERO,
        }
    }
}

/// 日线级市场状态输出
#[derive(Debug, Clone, Default)]
pub struct DayMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_level: VolatilityLevel,
}

/// 日线级信号输入
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaySignalInput {
    pub pine_color_100_200: String,
    pub pine_color_20_50: String,
    pub pine_color_12_26: String,
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

/// 仓位状态 (简化版 CheckList)
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
