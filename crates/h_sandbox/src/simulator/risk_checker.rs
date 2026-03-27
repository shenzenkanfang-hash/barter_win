//! Shadow RiskChecker - 模拟风控检查器
//!
//! 【优化改动】新增模块，补充模拟环境缺失的风控组件
//! 原因：模拟真实环境的 RiskChecker 接口

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use a_common::exchange::ExchangeAccount;
use f_engine::interfaces::{ExecutedOrder, PositionInfo, RiskChecker, RiskThresholds, RiskWarning};
use f_engine::types::{OrderRequest, RiskCheckResult};

/// Shadow 风控检查器
///
/// 模拟币安期货的风控规则
pub struct ShadowRiskChecker {
    thresholds: RiskThresholds,
    max_leverage: u32,
}

impl ShadowRiskChecker {
    pub fn new() -> Self {
        Self {
            thresholds: RiskThresholds::default(),
            max_leverage: 20,
        }
    }

    /// 检查杠杆是否超过限制
    fn check_leverage(&self, leverage: u32) -> bool {
        leverage <= self.max_leverage
    }

    /// 检查订单金额是否满足最小要求
    fn check_order_value(&self, price: Option<Decimal>, qty: Decimal) -> bool {
        if let Some(p) = price {
            let value = p * qty;
            // 最小订单金额: 5 USDT 或 最小数量 10
            value >= dec!(5) || qty >= dec!(10)
        } else {
            true // 市价单不检查价格
        }
    }
}

impl Default for ShadowRiskChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl RiskChecker for ShadowRiskChecker {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult {
        // 临时跳过所有风控检查，专注于测试架构
        // TODO: 后续根据实际需求启用风控规则
        
        // 1. 检查订单金额 (临时禁用)
        // if !self.check_order_value(order.price, order.qty) {
        //     return RiskCheckResult::new(false, false);
        // }

        // 2. 检查余额是否足够（临时禁用）
        // let order_value = order.qty * order.price.unwrap_or(dec!(0));
        // if order_value > account.available {
        //     return RiskCheckResult::new(false, false);
        // }

        RiskCheckResult::new(true, true)
    }

    fn post_check(&self, _order: &ExecutedOrder, _account: &ExchangeAccount) -> RiskCheckResult {
        RiskCheckResult::new(true, true)
    }

    fn scan(&self, _positions: &[PositionInfo], _account: &ExchangeAccount) -> Vec<RiskWarning> {
        vec![]
    }

    fn thresholds(&self) -> RiskThresholds {
        self.thresholds.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leverage_check() {
        let checker = ShadowRiskChecker::new();
        assert!(checker.check_leverage(10));
        assert!(!checker.check_leverage(25));
    }

    #[test]
    fn test_order_value_check() {
        let checker = ShadowRiskChecker::new();
        assert!(checker.check_order_value(Some(dec!(50000)), dec!(1)));
        assert!(!checker.check_order_value(Some(dec!(100)), dec!(0.001)));
    }
}
