//! Simulator Account - 账户状态机
//!
//! 纯状态管理：持仓、余额、盈亏计算，强平检测

use fnv::FnvHashMap;
use rust_decimal::Decimal;

use a_common::exchange::{ExchangeAccount, ExchangePosition, RejectReason};
use crate::api::mock_config::MockConfig;

// Side 需要重新导出供外部使用
pub use a_common::models::types::Side;

/// 模拟持仓
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_entry_price: Decimal,
    pub long_margin: Decimal,
    pub short_qty: Decimal,
    pub short_entry_price: Decimal,
    pub short_margin: Decimal,
}

impl Position {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            long_qty: Decimal::ZERO,
            long_entry_price: Decimal::ZERO,
            long_margin: Decimal::ZERO,
            short_qty: Decimal::ZERO,
            short_entry_price: Decimal::ZERO,
            short_margin: Decimal::ZERO,
        }
    }

    pub fn total_qty(&self) -> Decimal {
        self.long_qty + self.short_qty
    }

    pub fn is_empty(&self) -> bool {
        self.long_qty == Decimal::ZERO && self.short_qty == Decimal::ZERO
    }

    pub fn long_avg_price(&self) -> Decimal {
        if self.long_qty.is_zero() {
            Decimal::ZERO
        } else {
            self.long_entry_price
        }
    }

    pub fn short_avg_price(&self) -> Decimal {
        if self.short_qty.is_zero() {
            Decimal::ZERO
        } else {
            self.short_entry_price
        }
    }

    pub fn long_unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        if self.long_qty.is_zero() {
            Decimal::ZERO
        } else {
            (current_price - self.long_entry_price) * self.long_qty
        }
    }

    pub fn short_unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        if self.short_qty.is_zero() {
            Decimal::ZERO
        } else {
            (self.short_entry_price - current_price) * self.short_qty
        }
    }

    pub fn total_unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        self.long_unrealized_pnl(current_price) + self.short_unrealized_pnl(current_price)
    }

    pub fn total_margin(&self) -> Decimal {
        self.long_margin + self.short_margin
    }
}

/// 账户状态机
#[derive(Debug, Clone)]
pub struct Account {
    initial_balance: Decimal,
    available: Decimal,
    frozen_margin: Decimal,
    prices: FnvHashMap<String, Decimal>,
    positions: FnvHashMap<String, Position>,
    config: MockConfig,
    order_id_counter: u64,
}

impl Account {
    pub fn new(initial_balance: Decimal, config: &MockConfig) -> Self {
        Self {
            initial_balance,
            available: initial_balance,
            frozen_margin: Decimal::ZERO,
            prices: FnvHashMap::default(),
            positions: FnvHashMap::default(),
            config: config.clone(),
            order_id_counter: 0,
        }
    }

    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.prices.insert(symbol.to_string(), price);
    }

    pub fn get_price(&self, symbol: &str) -> Decimal {
        self.prices.get(symbol).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn get_position(&self, symbol: &str) -> Option<&Position> {
        self.positions.get(symbol)
    }

    pub fn get_or_create_position(&mut self, symbol: &str) -> &mut Position {
        self.positions.entry(symbol.to_string())
            .or_insert_with(|| Position::new(symbol.to_string()))
    }

    pub fn total_equity(&self) -> Decimal {
        let unrealized = self.total_unrealized_pnl();
        self.available + self.frozen_margin + unrealized
    }

    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.positions.values()
            .map(|p| p.total_unrealized_pnl(self.get_price(&p.symbol)))
            .sum()
    }

    pub fn frozen_margin(&self) -> Decimal {
        self.frozen_margin
    }

    pub fn available(&self) -> Decimal {
        self.available
    }

    pub fn initial_balance(&self) -> Decimal {
        self.initial_balance
    }

    pub fn pre_check(&self, symbol: &str, qty: Decimal, price: Decimal, leverage: Decimal, side: Side) -> Result<(), RejectReason> {
        // 开仓：验证保证金充足
        // 平仓：验证账户余额不会因手续费变负（保证金由 apply_close 释放）
        match side {
            Side::Buy => {
                let required_margin = price * qty / leverage;
                if self.available < required_margin {
                    return Err(RejectReason::InsufficientBalance);
                }

                // 开仓时，验证持仓价值不超限
                // 注意：这里用 current_position_value + price*qty，close 时由 caller 传 0
                let current_position_value = self.current_position_value(symbol);
                let total_equity = self.total_equity();
                let max_position_value = total_equity * self.config.max_position_ratio;

                if current_position_value + (price * qty) > max_position_value {
                    return Err(RejectReason::PositionLimitExceeded);
                }

                Ok(())
            }
            Side::Sell => {
                // 平仓时，不需要验证保证金，只需要验证有足够的余额支付手续费
                // 保证金和已实现盈亏由 apply_close 释放回 available
                // 但 pre_check 调用在 execute 之前，apply_close 还没执行
                // 所以这里只检查：持仓是否存在 + 余额不为负（极端情况）
                let position_value = self.positions.get(symbol)
                    .map(|p| (p.long_qty + p.short_qty) * price)
                    .unwrap_or(Decimal::ZERO);
                if position_value.is_zero() {
                    return Err(RejectReason::PositionLimitExceeded);
                }
                // 余额检查：当前余额应能覆盖手续费（apply_close 会释放保证金）
                if self.available < price * qty * self.config.fee_rate {
                    return Err(RejectReason::InsufficientBalance);
                }
                Ok(())
            }
        }
    }

    fn current_position_value(&self, symbol: &str) -> Decimal {
        self.positions.get(symbol)
            .map(|p| {
                let price = self.get_price(symbol);
                p.long_qty * price + p.short_qty * price
            })
            .unwrap_or(Decimal::ZERO)
    }

    pub fn apply_open(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal, leverage: Decimal) {
        let margin = price * qty / leverage;
        let position = self.get_or_create_position(symbol);

        match side {
            Side::Buy => {
                position.long_qty += qty;
                position.long_entry_price = price;
                position.long_margin += margin;
            }
            Side::Sell => {
                position.short_qty += qty;
                position.short_entry_price = price;
                position.short_margin += margin;
            }
        }

        self.available -= margin;
        self.frozen_margin += margin;
    }

    pub fn apply_close(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal) -> Decimal {
        let (realized_pnl, released_margin, should_remove) = {
            let position = self.get_or_create_position(symbol);
            let mut realized_pnl = Decimal::ZERO;
            let mut released_margin = Decimal::ZERO;

            match side {
                Side::Buy => {
                    let close_qty = qty.min(position.short_qty);
                    if close_qty > Decimal::ZERO {
                        realized_pnl = (price - position.short_entry_price) * close_qty;
                        released_margin = position.short_margin * close_qty / position.short_qty;
                        position.short_qty -= close_qty;
                        if position.short_qty.is_zero() {
                            position.short_entry_price = Decimal::ZERO;
                        }
                        position.short_margin -= released_margin;
                    }
                }
                Side::Sell => {
                    let close_qty = qty.min(position.long_qty);
                    if close_qty > Decimal::ZERO {
                        realized_pnl = (position.long_entry_price - price) * close_qty;
                        released_margin = position.long_margin * close_qty / position.long_qty;
                        position.long_qty -= close_qty;
                        if position.long_qty.is_zero() {
                            position.long_entry_price = Decimal::ZERO;
                        }
                        position.long_margin -= released_margin;
                    }
                }
            }

            (realized_pnl, released_margin, position.is_empty())
        };

        self.available += released_margin + realized_pnl;
        self.frozen_margin -= released_margin;

        if should_remove {
            self.positions.remove(symbol);
        }

        realized_pnl
    }

    pub fn deduct_fee(&mut self, fee: Decimal) {
        self.available -= fee;
    }

    pub fn next_order_id(&mut self) -> String {
        self.order_id_counter += 1;
        format!("MOCK_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            self.order_id_counter)
    }

    /// 强平检测
    pub fn check_liquidation(&self) -> bool {
        if self.frozen_margin.is_zero() {
            return false;
        }

        let total_position_value: Decimal = self.positions.values()
            .map(|p| {
                let price = self.get_price(&p.symbol);
                (p.long_qty + p.short_qty) * price
            })
            .sum();

        if total_position_value.is_zero() {
            return false;
        }

        let margin_ratio = self.frozen_margin / total_position_value;
        margin_ratio <= self.config.maintenance_margin_rate
    }

    pub fn to_exchange_account(&self) -> ExchangeAccount {
        ExchangeAccount {
            account_id: "MOCK_001".to_string(),
            total_equity: self.total_equity(),
            available: self.available,
            frozen_margin: self.frozen_margin,
            unrealized_pnl: self.total_unrealized_pnl(),
            update_ts: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
    }

    pub fn to_exchange_position(&self, symbol: &str) -> Option<ExchangePosition> {
        self.positions.get(symbol).map(|p| ExchangePosition {
            symbol: p.symbol.clone(),
            long_qty: p.long_qty,
            long_avg_price: p.long_avg_price(),
            short_qty: p.short_qty,
            short_avg_price: p.short_avg_price(),
            unrealized_pnl: p.total_unrealized_pnl(self.get_price(symbol)),
            margin_used: p.total_margin(),
        })
    }

    /// 获取所有持仓（用于数据同步）
    pub fn get_all_positions(&self) -> Vec<(String, Position)> {
        self.positions.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
