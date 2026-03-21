use a_common::EngineError;
use rust_decimal::Decimal;
use std::collections::HashSet;

/// 风控预检器
///
/// 检查项目:
/// 1. 资金是否足够 (最低保留金额)
/// 2. 持仓比例是否超限
/// 3. 品种是否已注册
/// 4. 波动率模式是否允许交易
#[derive(Debug, Clone)]
pub struct RiskPreChecker {
    max_position_ratio: Decimal,
    min_reserve_balance: Decimal,
    registered_symbols: HashSet<String>,
    volatility_mode: VolatilityMode,
}

/// 波动率模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolatilityMode {
    /// 正常模式 - 允许交易
    Normal,
    /// 高波动模式 - 减少仓位或禁止开仓
    High,
    /// 极端波动 - 禁止所有交易
    Extreme,
}

impl Default for VolatilityMode {
    fn default() -> Self {
        VolatilityMode::Normal
    }
}

impl RiskPreChecker {
    /// 创建风控预检器
    pub fn new(max_position_ratio: Decimal, min_reserve_balance: Decimal) -> Self {
        Self {
            max_position_ratio,
            min_reserve_balance,
            registered_symbols: HashSet::new(),
            volatility_mode: VolatilityMode::Normal,
        }
    }

    /// 注册可交易的品种
    pub fn register_symbol(&mut self, symbol: String) {
        self.registered_symbols.insert(symbol);
    }

    /// 设置波动率模式
    pub fn set_volatility_mode(&mut self, mode: VolatilityMode) {
        self.volatility_mode = mode;
    }

    /// 预检订单
    ///
    /// 检查顺序:
    /// 1. 品种是否注册
    /// 2. 波动率模式是否允许交易
    /// 3. 资金是否足够
    /// 4. 持仓比例是否超限
    pub fn pre_check(
        &self,
        symbol: &str,
        available_balance: Decimal,
        order_value: Decimal,
        total_equity: Decimal,
    ) -> Result<(), EngineError> {
        // 1. 检查品种是否注册
        if !self.registered_symbols.is_empty() && !self.registered_symbols.contains(symbol) {
            return Err(EngineError::RiskCheckFailed(format!(
                "品种 {} 未注册",
                symbol
            )));
        }

        // 2. 检查波动率模式
        match self.volatility_mode {
            VolatilityMode::Extreme => {
                return Err(EngineError::RiskCheckFailed(
                    "极端波动模式，禁止所有交易".to_string(),
                ));
            }
            VolatilityMode::High => {
                // 高波动模式下，仓位减半
                let adjusted_ratio = self.max_position_ratio / Decimal::try_from(2.0).unwrap();
                let position_ratio = order_value / total_equity;
                if position_ratio > adjusted_ratio {
                    return Err(EngineError::PositionLimitExceeded(format!(
                        "高波动模式: 订单金额/总权益 {} 超过调整后的最大比例 {}",
                        position_ratio, adjusted_ratio
                    )));
                }
            }
            VolatilityMode::Normal => {
                // 3. 检查资金
                if available_balance < self.min_reserve_balance {
                    return Err(EngineError::InsufficientFund(format!(
                        "可用资金 {} 小于最低保留 {}",
                        available_balance, self.min_reserve_balance
                    )));
                }

                // 4. 检查持仓比例
                let position_ratio = order_value / total_equity;
                if position_ratio > self.max_position_ratio {
                    return Err(EngineError::PositionLimitExceeded(format!(
                        "订单金额/总权益 {} 超过最大比例 {}",
                        position_ratio, self.max_position_ratio
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// E3.1 RiskPreChecker 测试 - 订单请求风控预检

    #[test]
    fn test_pre_check_normal_mode_pass() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));
        checker.register_symbol("BTCUSDT".to_string());

        // 正常模式：品种已注册、资金充足、持仓比例未超限 -> 通过
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(50000),
            dec!(5000),   // 5% 持仓比例
            dec!(100000),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_pre_check_unregistered_symbol() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));
        checker.register_symbol("BTCUSDT".to_string());

        // 品种未注册 -> 拒绝
        let result = checker.pre_check(
            "ETHUSDT",
            dec!(50000),
            dec!(5000),
            dec!(100000),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("未注册"));
    }

    #[test]
    fn test_pre_check_insufficient_balance() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));

        // 资金不足 -> 拒绝
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(500),    // 可用资金小于最低保留
            dec!(5000),
            dec!(100000),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("可用资金"));
    }

    #[test]
    fn test_pre_check_position_ratio_exceeded() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));

        // 持仓比例超限 (20% > 10%) -> 拒绝
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(50000),
            dec!(20000),
            dec!(100000),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("超过最大比例"));
    }

    #[test]
    fn test_pre_check_high_volatility_mode_pass() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));
        checker.set_volatility_mode(VolatilityMode::High);
        checker.register_symbol("BTCUSDT".to_string());

        // 高波动模式：仓位减半后通过 (5% < 10%/2 = 5%)
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(50000),
            dec!(5000),
            dec!(100000),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_pre_check_high_volatility_mode_rejected() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));
        checker.set_volatility_mode(VolatilityMode::High);
        checker.register_symbol("BTCUSDT".to_string());

        // 高波动模式：仓位减半后仍超限 (10% > 10%/2 = 5%) -> 拒绝
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(50000),
            dec!(10000),
            dec!(100000),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("高波动模式"));
    }

    #[test]
    fn test_pre_check_extreme_volatility_mode_rejected() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));
        checker.set_volatility_mode(VolatilityMode::Extreme);
        checker.register_symbol("BTCUSDT".to_string());

        // 极端波动模式：任何交易都拒绝
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(50000),
            dec!(1000),   // 很小的订单
            dec!(100000),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("极端波动模式"));
    }

    #[test]
    fn test_pre_check_empty_registration_allows_all() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));
        // 未注册任何品种（注册集合为空），允许所有品种交易

        let result = checker.pre_check(
            "ANYCOIN",
            dec!(50000),
            dec!(5000),
            dec!(100000),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_pre_check_boundary_position_ratio() {
        let mut checker = RiskPreChecker::new(dec!(0.1), dec!(1000));

        // 边界情况：正好 10% 持仓比例 -> 通过
        let result = checker.pre_check(
            "BTCUSDT",
            dec!(50000),
            dec!(10000),
            dec!(100000),
        );
        assert!(result.is_ok());
    }
}
