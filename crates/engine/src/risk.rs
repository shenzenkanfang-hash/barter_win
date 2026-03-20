use crate::error::EngineError;
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
