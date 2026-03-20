use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 阈值常量模块
///
/// 集中管理所有策略阈值常量，避免硬编码。
///
/// 设计依据: 设计文档 17.3.2
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThresholdConstants {
    // ========== 盈亏相关 ==========
    /// 盈利平仓阈值 (1%)
    pub profit_threshold: Decimal,
    /// 止损阈值 (5%)
    pub stop_loss_threshold: Decimal,

    // ========== 价格相关 ==========
    /// 下跌阈值 (2%) - 对冲/加仓触发
    pub price_down_threshold: Decimal,
    /// 上涨阈值 (2%) - 对冲/加仓触发
    pub price_up_threshold: Decimal,
    /// 下跌硬阈值 (10%)
    pub price_down_hard_threshold: Decimal,
    /// 上涨硬阈值 (10%)
    pub price_up_hard_threshold: Decimal,

    // ========== 马丁格尔相关 ==========
    /// 多头加仓价格阈值 (2%)
    pub long_add_threshold: Decimal,
    /// 空头加仓价格阈值 (2%)
    pub short_add_threshold: Decimal,

    // ========== 波动率相关 ==========
    /// 1分钟波动率阈值 (3%) - 进入高速通道
    pub volatility_threshold_1m: Decimal,
    /// 15分钟波动率阈值 (13%) - 进入高速通道
    pub volatility_threshold_15m: Decimal,

    // ========== 保证金相关 ==========
    /// 最低保留余额
    pub min_reserve_balance: Decimal,
    /// 最大持仓比例
    pub max_position_ratio: Decimal,
    /// 高波动模式调整因子 (0.5 = 减半)
    pub high_volatility_adjust_factor: Decimal,

    // ========== 数据相关 ==========
    /// 数据超时时间 (秒)
    pub data_timeout_seconds: i64,
    /// 下单间隔 (秒)
    pub order_interval_seconds: Decimal,
}

impl Default for ThresholdConstants {
    fn default() -> Self {
        Self::production()
    }
}

impl ThresholdConstants {
    /// 创建生产环境阈值
    pub fn production() -> Self {
        Self {
            // 盈亏
            profit_threshold: dec!(0.01),       // 1%
            stop_loss_threshold: dec!(0.05),   // 5%

            // 价格
            price_down_threshold: dec!(0.98),   // -2%
            price_up_threshold: dec!(1.02),    // +2%
            price_down_hard_threshold: dec!(0.90),  // -10%
            price_up_hard_threshold: dec!(1.10),   // +10%

            // 马丁格尔
            long_add_threshold: dec!(1.02),    // +2%
            short_add_threshold: dec!(0.98),   // -2%

            // 波动率
            volatility_threshold_1m: dec!(0.03),   // 3%
            volatility_threshold_15m: dec!(0.13),   // 13%

            // 保证金
            min_reserve_balance: dec!(1000.0),  // 最低保留 1000
            max_position_ratio: dec!(0.95),     // 95%
            high_volatility_adjust_factor: dec!(0.5), // 50%

            // 数据
            data_timeout_seconds: 180,          // 180 秒
            order_interval_seconds: dec!(0.1),  // 0.1 秒
        }
    }

    /// 创建测试环境阈值
    pub fn testing() -> Self {
        Self {
            profit_threshold: dec!(0.005),     // 0.5%
            stop_loss_threshold: dec!(0.03),    // 3%
            price_down_threshold: dec!(0.99),
            price_up_threshold: dec!(1.01),
            price_down_hard_threshold: dec!(0.95),
            price_up_hard_threshold: dec!(1.05),
            long_add_threshold: dec!(1.01),
            short_add_threshold: dec!(0.99),
            volatility_threshold_1m: dec!(0.02),
            volatility_threshold_15m: dec!(0.10),
            min_reserve_balance: dec!(100.0),
            max_position_ratio: dec!(0.80),
            high_volatility_adjust_factor: dec!(0.5),
            data_timeout_seconds: 60,
            order_interval_seconds: dec!(0.5),
        }
    }

    /// 创建高风险环境阈值
    pub fn high_risk() -> Self {
        Self {
            profit_threshold: dec!(0.02),       // 2%
            stop_loss_threshold: dec!(0.02),   // 2%
            price_down_threshold: dec!(0.99),
            price_up_threshold: dec!(1.01),
            price_down_hard_threshold: dec!(0.95),
            price_up_hard_threshold: dec!(1.05),
            long_add_threshold: dec!(1.005),
            short_add_threshold: dec!(0.995),
            volatility_threshold_1m: dec!(0.01),
            volatility_threshold_15m: dec!(0.05),
            min_reserve_balance: dec!(5000.0),
            max_position_ratio: dec!(0.50),
            high_volatility_adjust_factor: dec!(0.25),
            data_timeout_seconds: 60,
            order_interval_seconds: dec!(1.0),
        }
    }

    // ========== 辅助方法 ==========

    /// 判断是否应该保本平仓
    pub fn should_breakeven(&self, total_pnl: Decimal, entry_value: Decimal) -> bool {
        if entry_value <= dec!(0) {
            return false;
        }
        total_pnl >= dec!(0)
    }

    /// 判断是否应该止损
    pub fn should_stop_loss(&self, pnl_ratio: Decimal) -> bool {
        pnl_ratio <= -self.stop_loss_threshold
    }

    /// 判断是否达到盈利目标
    pub fn is_profit_target_reached(&self, pnl_ratio: Decimal) -> bool {
        pnl_ratio >= self.profit_threshold
    }

    /// 判断是否触发加仓
    pub fn should_add_position(&self, price_change_ratio: Decimal, is_long: bool) -> bool {
        if is_long {
            price_change_ratio <= -self.price_down_threshold + dec!(1)
        } else {
            price_change_ratio >= self.price_up_threshold - dec!(1)
        }
    }

    /// 判断是否触发对冲
    pub fn should_hedge(&self, price_change_ratio: Decimal) -> bool {
        price_change_ratio < -self.price_down_threshold + dec!(1)
            || price_change_ratio > self.price_up_threshold - dec!(1)
    }

    /// 判断是否触发硬止损
    pub fn is_hard_stop_loss(&self, price_change_ratio: Decimal) -> bool {
        price_change_ratio < -self.price_down_hard_threshold + dec!(1)
            || price_change_ratio > self.price_up_hard_threshold - dec!(1)
    }

    /// 获取调整后的最大持仓比例
    pub fn adjusted_max_ratio(&self, is_high_volatility: bool) -> Decimal {
        if is_high_volatility {
            self.max_position_ratio * self.high_volatility_adjust_factor
        } else {
            self.max_position_ratio
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_thresholds() {
        let t = ThresholdConstants::production();
        assert_eq!(t.profit_threshold, dec!(0.01));
        assert_eq!(t.stop_loss_threshold, dec!(0.05));
        assert_eq!(t.volatility_threshold_1m, dec!(0.03));
    }

    #[test]
    fn test_should_breakeven() {
        let t = ThresholdConstants::production();
        assert!(t.should_breakeven(dec!(100), dec!(10000)));
        assert!(!t.should_breakeven(dec!(-100), dec!(10000)));
    }

    #[test]
    fn test_should_stop_loss() {
        let t = ThresholdConstants::production();
        // -6% 亏损
        assert!(t.should_stop_loss(dec!(-0.06)));
        // +3% 盈利
        assert!(!t.should_stop_loss(dec!(0.03)));
    }

    #[test]
    fn test_is_profit_target_reached() {
        let t = ThresholdConstants::production();
        assert!(t.is_profit_target_reached(dec!(0.015)));
        assert!(!t.is_profit_target_reached(dec!(0.005)));
    }

    #[test]
    fn test_adjusted_max_ratio() {
        let t = ThresholdConstants::production();
        // 正常模式
        assert_eq!(t.adjusted_max_ratio(false), dec!(0.95));
        // 高波动模式 (减半)
        assert_eq!(t.adjusted_max_ratio(true), dec!(0.475));
    }

    #[test]
    fn test_should_add_position_long() {
        let t = ThresholdConstants::production();
        // 下跌 2.5% - 应该加仓
        assert!(t.should_add_position(dec!(-0.025), true));
        // 下跌 2% - 刚好等于阈值，应该加仓
        assert!(t.should_add_position(dec!(-0.02), true));
    }

    #[test]
    fn test_should_add_position_short() {
        let t = ThresholdConstants::production();
        // 上涨 2.5% - 应该加仓 (空头)
        assert!(t.should_add_position(dec!(0.025), false));
        // 上涨 1% - 不加仓
        assert!(!t.should_add_position(dec!(0.01), false));
    }
}
