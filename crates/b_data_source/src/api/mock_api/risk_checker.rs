//! MockRiskChecker - 模拟风控检查器
//!
//! 模拟真实环境的风险控制规则

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use a_common::exchange::ExchangeAccount;

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

/// 风控检查结果
#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    /// 是否通过风控
    pub passed: bool,
    /// 是否阻止下单
    pub blocked: bool,
    pub reason: Option<String>,
}

impl RiskCheckResult {
    pub fn new(passed: bool, blocked: bool) -> Self {
        Self { passed, blocked, reason: None }
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        Self { passed: false, blocked: true, reason: Some(reason.into()) }
    }
}

/// Mock 风控检查器
///
/// 模拟币安期货的风控规则
#[allow(dead_code)]
pub struct MockRiskChecker {
    max_leverage: u32,
    mode: RiskMode,
}

impl MockRiskChecker {
    /// 创建风控检查器
    pub fn new() -> Self {
        Self {
            max_leverage: 20,
            mode: RiskMode::default(),
        }
    }

    /// 创建带模式的风控检查器
    pub fn with_mode(mode: RiskMode) -> Self {
        Self {
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
    pub fn check_leverage(&self, leverage: u32) -> bool {
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

    /// 预风控检查
    pub fn pre_check(&self, symbol: &str, qty: Decimal, price: Option<Decimal>, account: &ExchangeAccount) -> RiskCheckResult {
        match self.mode {
            RiskMode::Bypass => {
                tracing::debug!("[Risk] {} 模式: 跳过风控检查", symbol);
                RiskCheckResult::new(true, false)
            }
            RiskMode::Audit => {
                // 审计模式：执行检查但记录日志，不阻止交易
                let order_value = qty * price.unwrap_or(Decimal::ZERO);
                let order_value_ok = self.check_order_value(price, qty);
                let balance_ok = order_value <= account.available;

                tracing::info!(
                    "[Risk] {} Audit模式: order_value_ok={}, balance_ok={}, available={}",
                    symbol,
                    order_value_ok,
                    balance_ok,
                    account.available
                );

                // 审计模式不阻止，只记录
                RiskCheckResult::new(true, false)
            }
            RiskMode::Strict => {
                // 严格模式：执行完整风控检查

                // 1. 检查订单金额
                if !self.check_order_value(price, qty) {
                    tracing::warn!("[Risk] {} 订单金额不满足要求", symbol);
                    return RiskCheckResult::rejected("订单金额不满足要求");
                }

                // 2. 检查余额是否足够
                let order_value = qty * price.unwrap_or(Decimal::ZERO);
                if order_value > account.available {
                    tracing::warn!(
                        "[Risk] {} 余额不足: 订单={}, 可用={}",
                        symbol,
                        order_value,
                        account.available
                    );
                    return RiskCheckResult::rejected("余额不足");
                }

                RiskCheckResult::new(true, false)
            }
        }
    }
}

impl Default for MockRiskChecker {
    fn default() -> Self {
        Self::new()
    }
}
