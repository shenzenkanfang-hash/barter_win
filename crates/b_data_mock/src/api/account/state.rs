//! 账户状态管理

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 精细化余额
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// 总余额
    pub total: Decimal,
    /// 可用余额
    pub free: Decimal,
    /// 冻结余额（挂单中）
    pub frozen: Decimal,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            total: Decimal::ZERO,
            free: Decimal::ZERO,
            frozen: Decimal::ZERO,
        }
    }
}

impl Balance {
    pub fn new(amount: Decimal) -> Self {
        Self {
            total: amount,
            free: amount,
            frozen: Decimal::ZERO,
        }
    }

    /// 冻结金额（挂单时调用）
    pub fn freeze(&mut self, amount: Decimal) -> Result<(), MockAccountError> {
        if self.free < amount {
            return Err(MockAccountError::InsufficientBalance {
                required: amount,
                available: self.free,
            });
        }
        self.free -= amount;
        self.frozen += amount;
        Ok(())
    }

    /// 解冻金额（取消订单时调用）
    pub fn unfreeze(&mut self, amount: Decimal) {
        self.frozen -= amount;
        self.free += amount;
    }

    /// 扣除金额（成交时调用）
    pub fn deduct(&mut self, amount: Decimal) {
        self.total -= amount;
        self.frozen -= amount;
    }

    /// 增加余额（充值或提现）
    pub fn add(&mut self, amount: Decimal) {
        self.total += amount;
        self.free += amount;
    }
}

/// 账户错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum MockAccountError {
    #[error("余额不足: 需要 {required}, 可用 {available}")]
    InsufficientBalance { required: Decimal, available: Decimal },

    #[error("持仓不足: 需要 {required}, 持有 {held}")]
    InsufficientPosition { required: Decimal, held: Decimal },
}

/// 持仓
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Position {
    /// 交易对
    pub symbol: String,
    /// 数量（正=多仓，负=空仓）
    pub qty: Decimal,
    /// 多仓平均入场价
    pub long_avg_price: Decimal,
    /// 空仓平均入场价
    pub short_avg_price: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
}

impl Position {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            qty: Decimal::ZERO,
            long_avg_price: Decimal::ZERO,
            short_avg_price: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
        }
    }

    /// 更新未实现盈亏
    pub fn update_pnl(&mut self, current_price: Decimal) {
        self.unrealized_pnl = self.qty * (current_price - self.long_avg_price)
            + (-self.qty) * (self.short_avg_price - current_price);
    }

    /// 是否有持仓
    pub fn has_position(&self) -> bool {
        !self.qty.is_zero()
    }
}

/// 账户状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountState {
    /// 余额映射（asset -> Balance）
    pub balances: HashMap<String, Balance>,
    /// 持仓映射（symbol -> Position）
    pub positions: HashMap<String, Position>,
}

impl AccountState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 获取或创建余额
    pub fn get_or_create_balance(&mut self, asset: &str) -> &mut Balance {
        self.balances.entry(asset.to_string()).or_insert_with(|| Balance::default())
    }

    /// 获取或创建持仓
    pub fn get_or_create_position(&mut self, symbol: &str) -> &mut Position {
        self.positions.entry(symbol.to_string()).or_insert_with(|| Position::new(symbol.to_string()))
    }

    /// 总权益（余额 + 未实现盈亏）
    pub fn total_equity(&self) -> Decimal {
        let balance_sum: Decimal = self.balances.values().map(|b| b.total).sum();
        let unrealized_sum: Decimal = self.positions.values().map(|p| p.unrealized_pnl).sum();
        balance_sum + unrealized_sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_balance_freeze() {
        let mut balance = Balance::new(dec!(1000));
        assert_eq!(balance.free, dec!(1000));

        balance.freeze(dec!(100)).unwrap();
        assert_eq!(balance.free, dec!(900));
        assert_eq!(balance.frozen, dec!(100));

        // 冻结超过可用应该失败
        assert!(balance.freeze(dec!(1000)).is_err());
    }

    #[test]
    fn test_position_pnl() {
        let mut pos = Position::new("BTCUSDT".to_string());
        pos.qty = dec!(0.5);
        pos.long_avg_price = dec!(50000);

        pos.update_pnl(dec!(51000));
        // 0.5 * (51000 - 50000) = 500
        assert_eq!(pos.unrealized_pnl, dec!(500));
    }
}
