//! 保证金池配置模块
//!
//! 对应 Python 的 config.py 中的风控配置常量
//!
//! 配置来源: D:\量化策略开发\tradingW\backup_old_code\d_risk_monitor\config.py

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 策略级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrategyLevel {
    Minute,
    Hour,
}

impl StrategyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            StrategyLevel::Minute => "MINUTE",
            StrategyLevel::Hour => "HOUR",
        }
    }
}

impl std::fmt::Display for StrategyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 全局保证金池配置
#[derive(Debug, Clone)]
pub struct GlobalMarginConfig {
    /// 最大使用比例 (默认 80%)
    pub max_usage_ratio: Decimal,
    /// 保留比例 (默认 20%)
    pub reserve_ratio: Decimal,
}

impl Default for GlobalMarginConfig {
    fn default() -> Self {
        Self {
            max_usage_ratio: dec!(0.8),
            reserve_ratio: dec!(0.2),
        }
    }
}

/// 单个策略级别的保证金配置
#[derive(Debug, Clone)]
pub struct StrategyMarginConfig {
    /// 分配比例
    pub allocation_ratio: Decimal,
    /// 新开仓比例
    pub new_open_ratio: Decimal,
    /// 翻倍仓比例
    pub double_open_ratio: Decimal,
}

/// 分钟级动态开仓配置
#[derive(Debug, Clone)]
pub struct MinuteOpenConfig {
    /// 每品种最小名义价值 (默认 5 USDT)
    pub min_notional_per_symbol: Decimal,
    /// 目标品种数量 (默认 50)
    pub target_symbol_count: u32,
    /// 阈值金额 (默认 250 USDT)
    pub threshold_amount: Decimal,
    /// 新开仓比例 (默认 15%)
    pub new_open_ratio: Decimal,
}

impl Default for MinuteOpenConfig {
    fn default() -> Self {
        Self {
            min_notional_per_symbol: dec!(5.0),
            target_symbol_count: 50,
            threshold_amount: dec!(250.0),
            new_open_ratio: dec!(0.15),
        }
    }
}

/// 小时级开仓配置
#[derive(Debug, Clone)]
pub struct HourOpenConfig {
    /// 每品种最小名义价值 (默认 5 USDT)
    pub min_notional_per_symbol: Decimal,
    /// 目标品种数量 (默认 10)
    pub target_symbol_count: u32,
    /// 加仓次数 (默认 10)
    pub add_times: u32,
    /// 名义价值阈值 (默认 500 USDT)
    pub threshold_notional: Decimal,
}

impl Default for HourOpenConfig {
    fn default() -> Self {
        Self {
            min_notional_per_symbol: dec!(5.0),
            target_symbol_count: 10,
            add_times: 10,
            threshold_notional: dec!(500.0),
        }
    }
}

/// 保证金池完整配置
#[derive(Debug, Clone)]
pub struct MarginPoolConfig {
    /// 全局配置
    pub global: GlobalMarginConfig,
    /// 分钟级策略配置
    pub minute: StrategyMarginConfig,
    /// 小时级策略配置
    pub hour: StrategyMarginConfig,
    /// 分钟级开仓配置
    pub minute_open: MinuteOpenConfig,
    /// 小时级开仓配置
    pub hour_open: HourOpenConfig,
}

impl Default for MarginPoolConfig {
    fn default() -> Self {
        Self {
            global: GlobalMarginConfig::default(),
            minute: StrategyMarginConfig {
                allocation_ratio: dec!(0.4),
                new_open_ratio: dec!(0.15),
                double_open_ratio: dec!(0.5),
            },
            hour: StrategyMarginConfig {
                allocation_ratio: dec!(0.4),
                new_open_ratio: dec!(0.3),
                double_open_ratio: dec!(0.5),
            },
            minute_open: MinuteOpenConfig::default(),
            hour_open: HourOpenConfig::default(),
        }
    }
}

impl MarginPoolConfig {
    /// 创建带自定义配置的保证金池
    pub fn with_config(
        max_usage_ratio: Decimal,
        reserve_ratio: Decimal,
        minute_allocation: Decimal,
        hour_allocation: Decimal,
    ) -> Self {
        Self {
            global: GlobalMarginConfig {
                max_usage_ratio,
                reserve_ratio,
            },
            minute: StrategyMarginConfig {
                allocation_ratio: minute_allocation,
                new_open_ratio: dec!(0.15),
                double_open_ratio: dec!(0.5),
            },
            hour: StrategyMarginConfig {
                allocation_ratio: hour_allocation,
                new_open_ratio: dec!(0.3),
                double_open_ratio: dec!(0.5),
            },
            minute_open: MinuteOpenConfig::default(),
            hour_open: HourOpenConfig::default(),
        }
    }

    /// 获取策略配置
    pub fn strategy_config(&self, level: StrategyLevel) -> &StrategyMarginConfig {
        match level {
            StrategyLevel::Minute => &self.minute,
            StrategyLevel::Hour => &self.hour,
        }
    }
}

/// 单品种最大名义价值限制 (默认 5000 USDT)
pub const MAX_SINGLE_NOTIONAL: Decimal = dec!(5000.0);

/// 账户兜底保证金 (默认 1000 USDT)
pub const FALLBACK_TOTAL_MARGIN: Decimal = dec!(1000.0);

/// 最低有效保证金 (默认 0.01 USDT)
pub const MIN_EFFECTIVE_MARGIN: Decimal = dec!(0.01);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MarginPoolConfig::default();
        assert_eq!(config.global.max_usage_ratio, dec!(0.8));
        assert_eq!(config.global.reserve_ratio, dec!(0.2));
        assert_eq!(config.minute.allocation_ratio, dec!(0.4));
        assert_eq!(config.minute_open.min_notional_per_symbol, dec!(5.0));
    }

    #[test]
    fn test_strategy_config() {
        let config = MarginPoolConfig::default();
        assert_eq!(config.strategy_config(StrategyLevel::Minute).allocation_ratio, dec!(0.4));
        assert_eq!(config.strategy_config(StrategyLevel::Hour).allocation_ratio, dec!(0.4));
    }
}
