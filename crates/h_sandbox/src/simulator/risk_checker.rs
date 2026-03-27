//! Shadow RiskChecker - 模拟风控检查器
//!
//! 【优化改动】新增模块，补充模拟环境缺失的风控组件
//! 原因：模拟真实环境的 RiskChecker 接口

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use a_common::exchange::ExchangeAccount;
use f_engine::interfaces::{ExecutedOrder, PositionInfo, RiskChecker, RiskThresholds, RiskWarning};
use f_engine::types::{OrderRequest, RiskCheckResult};

/// 风控模式
///
/// - Strict: 严格模式，所有风控规则强制执行
/// - Audit: 审计模式，记录所有风控决策但不阻止交易
/// - Bypass: 旁路模式，跳过风控检查
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskMode {
    Strict,
    Audit,
    Bypass,
}

impl Default for RiskMode {
    fn default() -> Self {
        RiskMode::Strict
    }
}

/// Shadow 风控检查器
///
/// 模拟币安期货的风控规则
#[allow(dead_code)]
pub struct ShadowRiskChecker {
    thresholds: RiskThresholds,
    max_leverage: u32,
    mode: RiskMode,
}

impl ShadowRiskChecker {
    pub fn new() -> Self {
        Self {
            thresholds: RiskThresholds::default(),
            max_leverage: 20,
            mode: RiskMode::default(),
        }
    }

    /// 创建带模式的风控检查器
    pub fn with_mode(mode: RiskMode) -> Self {
        Self {
            thresholds: RiskThresholds::default(),
            max_leverage: 20,
            mode,
        }
    }

    /// 设置风控模式
    pub fn set_mode(&mut self, mode: RiskMode) {
        self.mode = mode;
    }

    /// 获取当前风控模式
    pub fn get_mode(&self) -> RiskMode {
        self.mode
    }

    /// 检查杠杆是否超过限制
    #[allow(dead_code)]
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
        match self.mode {
            RiskMode::Bypass => {
                // 旁路模式：跳过所有风控检查，直接通过
                tracing::debug!("[Risk] {} 模式: 跳过风控检查", order.symbol);
                RiskCheckResult::new(true, true)
            }
            RiskMode::Audit => {
                // 审计模式：执行检查但记录日志，不阻止交易
                let order_value = order.qty * order.price.unwrap_or(dec!(0));
                
                let order_value_ok = self.check_order_value(order.price, order.qty);
                let balance_ok = order_value <= account.available;
                
                tracing::info!(
                    "[Risk] {} Audit模式: symbol={}, order_value_ok={}, balance_ok={}, available={}",
                    order.symbol,
                    order.symbol,
                    order_value_ok,
                    balance_ok,
                    account.available
                );
                
                // 审计模式不阻止，只记录
                RiskCheckResult::new(true, true)
            }
            RiskMode::Strict => {
                // 严格模式：执行完整风控检查
                
                // 1. 检查订单金额
                if !self.check_order_value(order.price, order.qty) {
                    tracing::warn!("[Risk] {} 订单金额不满足要求", order.symbol);
                    return RiskCheckResult::new(false, false);
                }

                // 2. 检查余额是否足够
                let order_value = order.qty * order.price.unwrap_or(dec!(0));
                if order_value > account.available {
                    tracing::warn!(
                        "[Risk] {} 余额不足: 订单={}, 可用={}",
                        order.symbol,
                        order_value,
                        account.available
                    );
                    return RiskCheckResult::new(false, false);
                }

                RiskCheckResult::new(true, true)
            }
        }
    }

    fn post_check(&self, _order: &ExecutedOrder, _account: &ExchangeAccount) -> RiskCheckResult {
        match self.mode {
            RiskMode::Bypass => RiskCheckResult::new(true, true),
            RiskMode::Audit | RiskMode::Strict => {
                // 后续实现持仓检查...
                RiskCheckResult::new(true, true)
            }
        }
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
