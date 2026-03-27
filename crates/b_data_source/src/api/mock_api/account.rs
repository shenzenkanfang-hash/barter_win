//! Simulator Account - 账户状态机
//!
//! 纯状态管理：持仓、余额、盈亏计算、强平检测
//! 不含下单逻辑

use fnv::FnvHashMap;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use a_common::exchange::{ExchangeAccount, ExchangePosition, RejectReason};
use super::config::MockConfig;

/// 持仓方向（从 a_common 导入并重导出）
pub use a_common::models::types::Side;

/// 模拟持仓
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 交易对
    pub symbol: String,
    /// 多头数量
    pub long_qty: Decimal,
    /// 多头入场价
    pub long_entry_price: Decimal,
    /// 多头保证金
    pub long_margin: Decimal,
    /// 空头数量
    pub short_qty: Decimal,
    /// 空头入场价
    pub short_entry_price: Decimal,
    /// 空头保证金
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

    /// 多头数量 + 空头数量
    pub fn total_qty(&self) -> Decimal {
        self.long_qty + self.short_qty
    }

    /// 是否为空仓
    pub fn is_empty(&self) -> bool {
        self.long_qty == Decimal::ZERO && self.short_qty == Decimal::ZERO
    }

    /// 多头持仓均价
    pub fn long_avg_price(&self) -> Decimal {
        if self.long_qty.is_zero() {
            return Decimal::ZERO;
        }
        self.long_entry_price
    }

    /// 空头持仓均价
    pub fn short_avg_price(&self) -> Decimal {
        if self.short_qty.is_zero() {
            return Decimal::ZERO;
        }
        self.short_entry_price
    }

    /// 多头未实现盈亏
    pub fn long_unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        if self.long_qty.is_zero() {
            return Decimal::ZERO;
        }
        (current_price - self.long_entry_price) * self.long_qty
    }

    /// 空头未实现盈亏
    pub fn short_unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        if self.short_qty.is_zero() {
            return Decimal::ZERO;
        }
        (self.short_entry_price - current_price) * self.short_qty
    }

    /// 总未实现盈亏
    pub fn total_unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        self.long_unrealized_pnl(current_price) + self.short_unrealized_pnl(current_price)
    }

    /// 总保证金
    pub fn total_margin(&self) -> Decimal {
        self.long_margin + self.short_margin
    }
}

/// 账户状态机
///
/// 纯状态管理，不包含下单逻辑
#[derive(Debug, Clone)]
pub struct Account {
    /// 初始余额
    initial_balance: Decimal,
    /// 可用余额
    available: Decimal,
    /// 冻结保证金
    frozen_margin: Decimal,
    /// 当前价格（用于计算未实现盈亏）
    prices: FnvHashMap<String, Decimal>,
    /// 持仓
    positions: FnvHashMap<String, Position>,
    /// 配置
    config: MockConfig,
    /// 订单号计数器
    order_id_counter: u64,
}

impl Account {
    /// 创建新账户
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

    /// 更新价格
    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.prices.insert(symbol.to_string(), price);
    }

    /// 获取当前价格
    pub fn get_price(&self, symbol: &str) -> Decimal {
        self.prices.get(symbol).copied().unwrap_or(Decimal::ZERO)
    }

    /// 获取持仓引用
    pub fn get_position(&self, symbol: &str) -> Option<&Position> {
        self.positions.get(symbol)
    }

    /// 获取持仓（可变）
    pub fn get_position_mut(&mut self, symbol: &str) -> Option<&mut Position> {
        self.positions.get_mut(symbol)
    }

    /// 获取或创建持仓
    pub fn get_or_create_position(&mut self, symbol: &str) -> &mut Position {
        self.positions.entry(symbol.to_string()).or_insert_with(|| Position::new(symbol.to_string()))
    }

    /// 总权益 = 可用 + 冻结 + 未实现盈亏
    pub fn total_equity(&self) -> Decimal {
        let unrealized = self.total_unrealized_pnl();
        self.available + self.frozen_margin + unrealized
    }

    /// 总未实现盈亏
    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.positions.values()
            .map(|p| p.total_unrealized_pnl(self.get_price(&p.symbol)))
            .sum()
    }

    /// 冻结保证金
    pub fn frozen_margin(&self) -> Decimal {
        self.frozen_margin
    }

    /// 可用余额
    pub fn available(&self) -> Decimal {
        self.available
    }

    /// 初始余额
    pub fn initial_balance(&self) -> Decimal {
        self.initial_balance
    }

    /// 下单前置检查
    pub fn pre_check(&self, symbol: &str, qty: Decimal, price: Decimal, leverage: Decimal) -> Result<(), RejectReason> {
        // 检查余额
        let required_margin = price * qty / leverage;
        if self.available < required_margin {
            return Err(RejectReason::InsufficientBalance);
        }

        // 检查最大持仓比例
        let current_position_value = self.current_position_value(symbol);
        let total_equity = self.total_equity();
        let max_position_value = total_equity * self.config.max_position_ratio;
        
        if current_position_value + (price * qty) > max_position_value {
            return Err(RejectReason::PositionLimitExceeded);
        }

        Ok(())
    }

    /// 当前持仓价值
    fn current_position_value(&self, symbol: &str) -> Decimal {
        self.positions.get(symbol)
            .map(|p| {
                let price = self.get_price(symbol);
                p.long_qty * price + p.short_qty * price
            })
            .unwrap_or(Decimal::ZERO)
    }

    /// 开仓 - 账户层面更新状态
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

    /// 平仓 - 账户层面更新状态
    pub fn apply_close(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal) -> Decimal {
        // 先计算并释放借用
        let (realized_pnl, released_margin, should_remove) = {
            let position = self.get_or_create_position(symbol);
            let mut realized_pnl = Decimal::ZERO;
            let mut released_margin = Decimal::ZERO;

            match side {
                Side::Buy => {
                    // 平空仓
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
                    // 平多仓
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

        // 更新账户余额
        self.available += released_margin + realized_pnl;
        self.frozen_margin -= released_margin;
        
        // 清理空持仓
        if should_remove {
            self.positions.remove(symbol);
        }

        realized_pnl
    }

    /// 扣除手续费
    pub fn deduct_fee(&mut self, fee: Decimal) {
        self.available -= fee;
    }

    /// 生成订单号
    pub fn next_order_id(&mut self) -> String {
        self.order_id_counter += 1;
        format!("SIM_{}_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(), self.order_id_counter)
    }

    /// 强平检测
    pub fn check_liquidation(&self) -> bool {
        if self.frozen_margin.is_zero() {
            return false;
        }
        let margin_ratio = self.frozen_margin / self.total_equity();
        margin_ratio >= self.config.maintenance_margin_rate
    }

    /// 转换为 ExchangeAccount
    pub fn to_exchange_account(&self) -> ExchangeAccount {
        ExchangeAccount {
            account_id: "SHADOW_001".to_string(),
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

    /// 转换为 ExchangePosition
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_account_creation() {
        let config = MockConfig::default();
        let account = Account::new(dec!(100000.0), &config);
        
        assert_eq!(account.available, dec!(100000.0));
        assert_eq!(account.frozen_margin, Decimal::ZERO);
    }

    #[test]
    fn test_update_price() {
        let config = MockConfig::default();
        let mut account = Account::new(dec!(100000.0), &config);
        
        account.update_price("BTCUSDT", dec!(50000.0));
        assert_eq!(account.get_price("BTCUSDT"), dec!(50000.0));
    }

    #[test]
    fn test_unrealized_pnl() {
        let config = MockConfig::default();
        let mut account = Account::new(dec!(100000.0), &config);
        
        // 开多仓
        account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(10));
        account.update_price("BTCUSDT", dec!(51000.0));
        
        let position = account.get_position("BTCUSDT").unwrap();
        assert_eq!(position.long_unrealized_pnl(dec!(51000.0)), dec!(100.0));
    }

    #[test]
    fn test_liquidation_check() {
        let config = MockConfig::default();
        let account = Account::new(dec!(1000.0), &config);
        
        // 无持仓，不应强平
        assert!(!account.check_liquidation());
    }
}
