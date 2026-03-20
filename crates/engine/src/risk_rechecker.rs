use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::error::EngineError;
use crate::risk::VolatilityMode;

/// 风控复核器 - 锁内复核
///
/// 在获取全局锁之后再次核对，确保并发安全。
/// 这是风控两层的第二层（锁内复核）。
///
/// 设计依据: 设计文档 16.9.2
pub struct RiskReChecker {
    /// 实时波动率阈值: 超过此值认为市场异常
    volatility_threshold: Decimal,
    /// 价格偏离阈值: 超过此值拒绝下单
    price_deviation_threshold: Decimal,
}

impl Default for RiskReChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl RiskReChecker {
    /// 创建风控复核器
    pub fn new() -> Self {
        Self {
            volatility_threshold: dec!(0.05),   // 5% 波动率阈值
            price_deviation_threshold: dec!(0.10), // 10% 价格偏离阈值
        }
    }

    /// 锁内复核检查
    ///
    /// 在全局锁保护下再次核对订单的风控条件。
    /// 如果不通过，锁会被释放，订单被拒绝。
    pub fn re_check(
        &self,
        available_balance: Decimal,
        order_value: Decimal,
        current_price: Decimal,
        reference_price: Decimal,
        volatility_mode: VolatilityMode,
    ) -> Result<(), EngineError> {
        // 1. 再次检查资金（防止并发修改）
        if available_balance < order_value {
            return Err(EngineError::InsufficientFund(format!(
                "锁内复核: 可用资金 {} 不足以支付订单金额 {}",
                available_balance, order_value
            )));
        }

        // 2. 检查价格偏离（防止价格剧烈波动）
        if reference_price > dec!(0) {
            let price_change = (current_price - reference_price).abs() / reference_price;
            if price_change > self.price_deviation_threshold {
                return Err(EngineError::RiskCheckFailed(format!(
                    "锁内复核: 价格偏离 {} 超过阈值 {}",
                    price_change, self.price_deviation_threshold
                )));
            }
        }

        // 3. 极端波动模式检查
        if volatility_mode == VolatilityMode::Extreme {
            return Err(EngineError::RiskCheckFailed(
                "锁内复核: 极端波动模式，禁止所有交易".to_string(),
            ));
        }

        // 4. 高速通道额外检查
        if volatility_mode == VolatilityMode::High {
            // 高波动模式下，订单金额不能超过可用资金的 40%
            let high_vol_ratio = order_value / available_balance;
            if high_vol_ratio > dec!(0.4) {
                return Err(EngineError::PositionLimitExceeded(format!(
                    "锁内复核: 高波动模式订单比例 {} 超过 40%",
                    high_vol_ratio
                )));
            }
        }

        Ok(())
    }

    /// 实时波动率检查
    ///
    /// 检查当前市场波动率是否异常，用于拒绝在高波动时开仓。
    pub fn check_volatility_realtime(
        &self,
        current_price: Decimal,
        open_price: Decimal,
        _high_price: Decimal,
        _low_price: Decimal,
    ) -> Result<(), EngineError> {
        // 计算实时波动率 (当前价格 vs 开盘价)
        if open_price > dec!(0) {
            let realtime_volatility = (current_price - open_price).abs() / open_price;

            if realtime_volatility > self.volatility_threshold {
                return Err(EngineError::RiskCheckFailed(format!(
                    "实时波动率 {} 超过阈值 {}",
                    realtime_volatility, self.volatility_threshold
                )));
            }
        }

        Ok(())
    }

    /// 检查订单金额是否合理
    pub fn check_order_value(
        &self,
        order_value: Decimal,
        available_balance: Decimal,
    ) -> Result<(), EngineError> {
        // 订单金额不能超过可用资金
        if order_value > available_balance {
            return Err(EngineError::InsufficientFund(format!(
                "订单金额 {} 超过可用资金 {}",
                order_value, available_balance
            )));
        }

        // 单笔订单金额不能超过可用资金的 90%
        let ratio = order_value / available_balance;
        if ratio > dec!(0.9) {
            return Err(EngineError::PositionLimitExceeded(format!(
                "单笔订单占比 {} 超过 90%",
                ratio
            )));
        }

        Ok(())
    }

    /// 设置波动率阈值
    pub fn set_volatility_threshold(&mut self, threshold: Decimal) {
        self.volatility_threshold = threshold;
    }

    /// 设置价格偏离阈值
    pub fn set_price_deviation_threshold(&mut self, threshold: Decimal) {
        self.price_deviation_threshold = threshold;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E3.2 RiskReChecker 测试 - 锁内复核检查

    #[test]
    fn test_risk_re_checker_basic() {
        let checker = RiskReChecker::new();
        let result = checker.re_check(
            dec!(10000),    // available_balance
            dec!(1000),     // order_value
            dec!(50000),    // current_price
            dec!(49500),    // reference_price
            VolatilityMode::Normal,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_risk_re_checker_insufficient_fund() {
        let checker = RiskReChecker::new();
        let result = checker.re_check(
            dec!(500),      // available_balance
            dec!(1000),     // order_value
            dec!(50000),    // current_price
            dec!(49500),    // reference_price
            VolatilityMode::Normal,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_risk_re_checker_price_deviation() {
        let checker = RiskReChecker::new();
        // 价格偏离超过 10%
        let result = checker.re_check(
            dec!(10000),
            dec!(1000),
            dec!(55000),    // current_price 偏离 reference_price 11%
            dec!(49500),    // reference_price
            VolatilityMode::Normal,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_volatility_realtime() {
        let checker = RiskReChecker::new();
        let result = checker.check_volatility_realtime(
            dec!(52500),    // current_price
            dec!(50000),    // open_price
            dec!(53000),    // high
            dec!(49000),    // low
        );
        // 波动率 5% 等于阈值，应该通过
        assert!(result.is_ok());
    }

    #[test]
    fn test_risk_re_checker_extreme_volatility_mode() {
        let checker = RiskReChecker::new();
        // 极端波动模式：禁止所有交易
        let result = checker.re_check(
            dec!(10000),
            dec!(1000),
            dec!(50000),
            dec!(49500),
            VolatilityMode::Extreme,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("极端波动模式"));
    }

    #[test]
    fn test_risk_re_checker_high_volatility_mode_rejected() {
        let checker = RiskReChecker::new();
        // 高波动模式：订单比例超过 40%
        let result = checker.re_check(
            dec!(10000),
            dec!(5000),     // order_value / available_balance = 50% > 40%
            dec!(50000),
            dec!(49500),
            VolatilityMode::High,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("高波动模式"));
    }

    #[test]
    fn test_risk_re_checker_high_volatility_mode_pass() {
        let checker = RiskReChecker::new();
        // 高波动模式：订单比例 30% < 40% -> 通过
        let result = checker.re_check(
            dec!(10000),
            dec!(3000),
            dec!(50000),
            dec!(49500),
            VolatilityMode::High,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_volatility_realtime_exceeded() {
        let checker = RiskReChecker::new();
        // 实时波动率 6% > 5% 阈值 -> 拒绝
        let result = checker.check_volatility_realtime(
            dec!(53000),    // current_price
            dec!(50000),    // open_price
            dec!(54000),    // high
            dec!(49000),    // low
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("实时波动率"));
    }

    #[test]
    fn test_volatility_realtime_zero_open_price() {
        let checker = RiskReChecker::new();
        // 开盘价为0时不计算波动率 -> 通过
        let result = checker.check_volatility_realtime(
            dec!(50000),
            dec!(0),        // open_price = 0
            dec!(53000),
            dec!(49000),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_order_value_exceeds_balance() {
        let checker = RiskReChecker::new();
        let result = checker.check_order_value(
            dec!(15000),    // order_value > available
            dec!(10000),    // available_balance
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("超过可用资金"));
    }

    #[test]
    fn test_order_value_exceeds_90_percent() {
        let checker = RiskReChecker::new();
        // 单笔订单 95% > 90% -> 拒绝
        let result = checker.check_order_value(
            dec!(9500),
            dec!(10000),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("超过 90%"));
    }

    #[test]
    fn test_order_value_boundary() {
        let checker = RiskReChecker::new();
        // 正好 90% -> 通过
        let result = checker.check_order_value(
            dec!(9000),
            dec!(10000),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_risk_re_checker_zero_reference_price() {
        let checker = RiskReChecker::new();
        // 参考价为0时不检查价格偏离 -> 通过
        let result = checker.re_check(
            dec!(10000),
            dec!(1000),
            dec!(50000),
            dec!(0),        // reference_price = 0
            VolatilityMode::Normal,
        );
        assert!(result.is_ok());
    }
}
