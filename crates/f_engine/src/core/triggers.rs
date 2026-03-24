//! 触发器模块
//!
//! 负责主动检查市场状态，发现交易机会。
//!
//! # 触发器类型
//! - 分钟级触发器：高波动排名监听
//! - 日线级触发器：趋势转折检测

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::sync::Arc;
use parking_lot::RwLock;

use crate::core::{
    EngineStateHandle, SymbolState, VolatilityTier, RiskState,
    StrategyQuery, StrategyResponse, ChannelType,
};

/// 触发器配置
#[derive(Debug, Clone)]
pub struct TriggerConfig {
    /// 分钟级波动率阈值（百分比）
    pub minute_volatility_threshold: Decimal,
    /// 日线入场低位阈值（百分比）
    pub daily_entry_low_threshold: Decimal,
    /// 日线入场高位阈值（百分比）
    pub daily_entry_high_threshold: Decimal,
    /// 最大同时运行品种数
    pub max_running_symbols: usize,
    /// 分钟级指标历史数据要求（天数）
    pub minute_data_days: u32,
    /// 日线级指标历史数据要求（天数）
    pub daily_data_days: u32,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            minute_volatility_threshold: Decimal::from(13),
            daily_entry_low_threshold: Decimal::from(30),
            daily_entry_high_threshold: Decimal::from(70),
            max_running_symbols: 10,
            minute_data_days: 1,
            daily_data_days: 250,
        }
    }
}

impl TriggerConfig {
    pub fn production() -> Self {
        Self::default()
    }

    pub fn backtest() -> Self {
        Self {
            minute_volatility_threshold: Decimal::from(10),
            daily_entry_low_threshold: Decimal::from(25),
            daily_entry_high_threshold: Decimal::from(75),
            max_running_symbols: 50,
            minute_data_days: 1,
            daily_data_days: 250,
        }
    }
}

/// 触发结果
#[derive(Debug, Clone)]
pub struct TriggerResult {
    /// 品种
    pub symbol: String,
    /// 触发类型
    pub trigger_type: TriggerType,
    /// 是否通过预检
    pub precheck_passed: bool,
    /// 拒绝原因（如果有）
    pub reject_reason: Option<String>,
}

impl TriggerResult {
    pub fn passed(symbol: String, trigger_type: TriggerType) -> Self {
        Self {
            symbol,
            trigger_type,
            precheck_passed: true,
            reject_reason: None,
        }
    }

    pub fn rejected(symbol: String, trigger_type: TriggerType, reason: String) -> Self {
        Self {
            symbol,
            trigger_type,
            precheck_passed: false,
            reject_reason: Some(reason),
        }
    }
}

/// 触发类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerType {
    /// 分钟级高波动
    MinuteHighVolatility,
    /// 日线级趋势反转
    DailyTrendReversal,
}

// ============================================================================
// 分钟级触发器
// ============================================================================

/// 分钟级触发器
///
/// 检查高波动品种，发现交易机会。
pub struct MinuteTrigger {
    config: TriggerConfig,
}

impl MinuteTrigger {
    pub fn new(config: TriggerConfig) -> Self {
        Self { config }
    }

    /// 检查品种是否满足分钟级触发条件
    ///
    /// # 参数
    /// - symbol: 品种符号
    /// - volatility_ratio: 波动率百分比
    /// - engine_state: 引擎状态句柄
    pub fn check(
        &self,
        symbol: &str,
        volatility_ratio: Decimal,
        engine_state: &EngineStateHandle,
    ) -> TriggerResult {
        let state = engine_state.read();
        let trigger_type = TriggerType::MinuteHighVolatility;

        // 1. 检查波动率是否超过阈值
        if volatility_ratio < self.config.minute_volatility_threshold {
            return TriggerResult::rejected(
                symbol.to_string(),
                trigger_type,
                "波动率未超过阈值".to_string(),
            );
        }

        // 2. 检查品种是否已被其他策略绑定（品种互斥）
        if let Some(symbol_state) = state.get_symbol(symbol) {
            if symbol_state.is_bound() {
                return TriggerResult::rejected(
                    symbol.to_string(),
                    trigger_type,
                    format!("品种已被策略 {} 绑定", symbol_state.bound_strategy().unwrap_or("unknown")),
                );
            }
        }

        // 3. 检查是否超过最大同时运行品种数
        if state.symbol_count() >= self.config.max_running_symbols {
            return TriggerResult::rejected(
                symbol.to_string(),
                trigger_type,
                "已达到最大同时运行品种数".to_string(),
            );
        }

        TriggerResult::passed(symbol.to_string(), trigger_type)
    }
}

// ============================================================================
// 日线级触发器
// ============================================================================

/// 日线级触发器
///
/// 检查趋势转折，发现交易机会。
pub struct DailyTrigger {
    config: TriggerConfig,
}

impl DailyTrigger {
    pub fn new(config: TriggerConfig) -> Self {
        Self { config }
    }

    /// 检查品种是否满足日线级触发条件
    ///
    /// # 参数
    /// - symbol: 品种符号
    /// - price_position: 价格位置（0-100）
    /// - pine_color: Pine 颜色（转绿/转红）
    /// - engine_state: 引擎状态句柄
    pub fn check(
        &self,
        symbol: &str,
        price_position: Decimal,
        is_green: bool,
        is_red: bool,
        engine_state: &EngineStateHandle,
    ) -> TriggerResult {
        let state = engine_state.read();
        let trigger_type = TriggerType::DailyTrendReversal;

        // 1. 检查是否在低位且转绿（做多）
        let long_condition = price_position <= self.config.daily_entry_low_threshold && is_green;

        // 2. 检查是否在高位且转红（做空）
        let short_condition = price_position >= self.config.daily_entry_high_threshold && is_red;

        // 满足任一条件才算触发
        if !long_condition && !short_condition {
            return TriggerResult::rejected(
                symbol.to_string(),
                trigger_type,
                "不满足趋势反转条件".to_string(),
            );
        }

        // 3. 检查品种是否已被其他策略绑定
        if let Some(symbol_state) = state.get_symbol(symbol) {
            if symbol_state.is_bound() {
                return TriggerResult::rejected(
                    symbol.to_string(),
                    trigger_type,
                    format!("品种已被策略 {} 绑定", symbol_state.bound_strategy().unwrap_or("unknown")),
                );
            }
        }

        // 4. 检查是否超过最大同时运行品种数
        if state.symbol_count() >= self.config.max_running_symbols {
            return TriggerResult::rejected(
                symbol.to_string(),
                trigger_type,
                "已达到最大同时运行品种数".to_string(),
            );
        }

        TriggerResult::passed(symbol.to_string(), trigger_type)
    }
}

// ============================================================================
// 并行触发器管理器
// ============================================================================

/// 并行触发器管理器
pub struct TriggerManager {
    minute_trigger: MinuteTrigger,
    daily_trigger: DailyTrigger,
}

impl TriggerManager {
    pub fn new(config: TriggerConfig) -> Self {
        Self {
            minute_trigger: MinuteTrigger::new(config.clone()),
            daily_trigger: DailyTrigger::new(config),
        }
    }

    pub fn minute_trigger(&self) -> &MinuteTrigger {
        &self.minute_trigger
    }

    pub fn daily_trigger(&self) -> &DailyTrigger {
        &self.daily_trigger
    }
}

impl Default for TriggerManager {
    fn default() -> Self {
        Self::new(TriggerConfig::default())
    }
}
