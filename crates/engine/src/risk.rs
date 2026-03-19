use crate::error::EngineError;
use rust_decimal::Decimal;

pub struct RiskPreChecker {
    max_position_ratio: Decimal,
    min_reserve_balance: Decimal,
}

impl RiskPreChecker {
    pub fn new(max_position_ratio: Decimal, min_reserve_balance: Decimal) -> Self {
        Self {
            max_position_ratio,
            min_reserve_balance,
        }
    }

    pub fn pre_check(
        &self,
        available_balance: Decimal,
        order_value: Decimal,
        total_equity: Decimal,
    ) -> Result<(), EngineError> {
        if available_balance < self.min_reserve_balance {
            return Err(EngineError::InsufficientFund(format!(
                "可用资金 {} 小于最低保留 {}",
                available_balance, self.min_reserve_balance
            )));
        }

        let position_ratio = order_value / total_equity;
        if position_ratio > self.max_position_ratio {
            return Err(EngineError::PositionLimitExceeded(format!(
                "订单金额/总权益 {} 超过最大比例 {}",
                position_ratio, self.max_position_ratio
            )));
        }

        Ok(())
    }
}
