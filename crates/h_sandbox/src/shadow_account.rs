//! ShadowAccount - 模拟账户核心逻辑
//!
//! 本地维护账户余额、持仓、盈亏计算，强平检测等
//! 线程安全，通过 Arc<RwLock<>> 保护

use fnv::FnvHashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use a_common::exchange::{ExchangeAccount, ExchangePosition, RejectReason};
use crate::shadow_config::ShadowConfig;

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,  // 多头
    Sell, // 空头
}

/// 模拟持仓
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowPosition {
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

impl ShadowPosition {
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

    /// 计算未实现盈亏（需要外部注入当前价格）
    pub fn unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        let long_pnl = if self.long_qty > Decimal::ZERO {
            (current_price - self.long_entry_price) * self.long_qty
        } else {
            Decimal::ZERO
        };

        let short_pnl = if self.short_qty > Decimal::ZERO {
            (self.short_entry_price - current_price) * self.short_qty
        } else {
            Decimal::ZERO
        };

        long_pnl + short_pnl
    }

    /// 总保证金
    pub fn total_margin(&self) -> Decimal {
        self.long_margin + self.short_margin
    }
}

impl From<&ShadowPosition> for ExchangePosition {
    fn from(pos: &ShadowPosition) -> Self {
        ExchangePosition {
            symbol: pos.symbol.clone(),
            long_qty: pos.long_qty,
            long_avg_price: pos.long_entry_price,
            short_qty: pos.short_qty,
            short_avg_price: pos.short_entry_price,
            unrealized_pnl: Decimal::ZERO, // 需要外部价格计算
            margin_used: pos.total_margin(),
        }
    }
}

/// 模拟账户
#[derive(Debug, Clone)]
pub struct ShadowAccount {
    /// 钱包余额（包含已实现盈亏）
    wallet_balance: Decimal,
    /// 初始余额
    initial_balance: Decimal,
    /// 手续费率
    fee_rate: Decimal,
    /// 维持保证金率
    maintenance_margin_rate: Decimal,
    /// 持仓（Hedge 模式）
    positions: FnvHashMap<String, ShadowPosition>,
    /// 当前价格映射（外部注入）
    price_map: FnvHashMap<String, Decimal>,
    /// 下一个订单ID
    next_order_id: u64,
}

impl ShadowAccount {
    pub fn new(initial_balance: Decimal, config: &ShadowConfig) -> Self {
        Self {
            wallet_balance: initial_balance,
            initial_balance,
            fee_rate: config.fee_rate,
            maintenance_margin_rate: config.maintenance_margin_rate,
            positions: FnvHashMap::default(),
            price_map: FnvHashMap::default(),
            next_order_id: 1,
        }
    }

    /// 生成订单ID
    pub fn next_order_id(&mut self) -> String {
        let id = format!("SH{}", self.next_order_id);
        self.next_order_id += 1;
        id
    }

    /// 更新价格（计算未实现盈亏）
    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.price_map.insert(symbol.to_string(), price);
    }

    /// 获取当前价格
    pub fn get_price(&self, symbol: &str) -> Option<Decimal> {
        self.price_map.get(symbol).copied()
    }

    /// 总权益 = 钱包余额 + 未实现盈亏
    pub fn total_equity(&self) -> Decimal {
        self.wallet_balance + self.total_unrealized_pnl()
    }

    /// 未实现盈亏总额
    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.positions
            .iter()
            .map(|(symbol, pos)| {
                self.price_map
                    .get(symbol)
                    .map(|p| pos.unrealized_pnl(*p))
                    .unwrap_or(Decimal::ZERO)
            })
            .sum()
    }

    /// 冻结保证金
    pub fn frozen_margin(&self) -> Decimal {
        self.positions.values().map(|p| p.total_margin()).sum()
    }

    /// 可用余额 = 总权益 - 冻结保证金
    pub fn available_balance(&self) -> Decimal {
        (self.total_equity() - self.frozen_margin()).max(Decimal::ZERO)
    }

    /// 维持保证金
    pub fn total_maint_margin(&self) -> Decimal {
        self.positions
            .iter()
            .map(|(symbol, pos)| {
                let price = self.price_map.get(symbol).unwrap_or(&Decimal::ZERO);
                pos.total_qty() * *price * self.maintenance_margin_rate
            })
            .sum()
    }

    /// 获取持仓
    pub fn get_position(&self, symbol: &str) -> Option<&ShadowPosition> {
        self.positions.get(symbol)
    }

    /// 获取所有有持仓的符号
    pub fn active_symbols(&self) -> Vec<String> {
        self.positions
            .iter()
            .filter(|(_, pos)| !pos.is_empty())
            .map(|(sym, _)| sym.clone())
            .collect()
    }

    /// 开仓
    ///
    /// 返回: (order_id, filled_price, filled_qty, commission) 或 Err
    pub fn open(
        &mut self,
        symbol: &str,
        side: Side,
        qty: Decimal,
        price: Decimal,
        leverage: i32,
    ) -> Result<(String, Decimal, Decimal, Decimal), RejectReason> {
        let notional = qty * price;
        let margin = notional / Decimal::from(leverage); // 初始保证金
        let fee = notional * self.fee_rate;
        let order_id = self.next_order_id();

        // 检查余额
        if self.available_balance() < margin + fee {
            return Err(RejectReason::InsufficientBalance);
        }

        // 扣除手续费
        self.wallet_balance -= fee;

        // 更新持仓
        let position = self.positions.entry(symbol.to_string()).or_insert_with(|| {
            ShadowPosition::new(symbol.to_string())
        });

        match side {
            Side::Buy => {
                // 多头：计算新的加权平均价
                let total_cost = position.long_qty * position.long_entry_price + qty * price;
                let new_qty = position.long_qty + qty;
                position.long_entry_price = if new_qty > Decimal::ZERO {
                    total_cost / new_qty
                } else {
                    Decimal::ZERO
                };
                position.long_qty = new_qty;
                position.long_margin += margin;
            }
            Side::Sell => {
                // 空头：计算新的加权平均价
                let total_cost = position.short_qty * position.short_entry_price + qty * price;
                let new_qty = position.short_qty + qty;
                position.short_entry_price = if new_qty > Decimal::ZERO {
                    total_cost / new_qty
                } else {
                    Decimal::ZERO
                };
                position.short_qty = new_qty;
                position.short_margin += margin;
            }
        }

        Ok((order_id, price, qty, fee))
    }

    /// 平仓
    ///
    /// 返回: (order_id, realized_pnl, released_margin, commission) 或 Err
    pub fn close(
        &mut self,
        symbol: &str,
        side: Side,
        qty: Decimal,
        price: Decimal,
    ) -> Result<(String, Decimal, Decimal, Decimal), RejectReason> {
        let position = match self.positions.get_mut(symbol) {
            Some(p) => p,
            None => return Err(RejectReason::SymbolNotTradable),
        };

        let order_id = self.next_order_id();
        let fee = qty * price * self.fee_rate;

        match side {
            Side::Buy => {
                // 平空头
                if qty > position.short_qty {
                    return Err(RejectReason::InsufficientBalance);
                }

                // 已实现盈亏 = (入场价 - 平仓价) * 数量
                let realized_pnl = (position.short_entry_price - price) * qty;
                let released_margin = position.short_margin * qty / position.short_qty;

                position.short_qty -= qty;
                if position.short_qty == Decimal::ZERO {
                    position.short_entry_price = Decimal::ZERO;
                }
                position.short_margin -= released_margin;

                // 结算盈亏 + 释放保证金 + 扣除手续费
                self.wallet_balance += realized_pnl + released_margin - fee;

                Ok((order_id, realized_pnl, released_margin, fee))
            }
            Side::Sell => {
                // 平多头
                if qty > position.long_qty {
                    return Err(RejectReason::InsufficientBalance);
                }

                // 已实现盈亏 = (平仓价 - 入场价) * 数量
                let realized_pnl = (price - position.long_entry_price) * qty;
                let released_margin = position.long_margin * qty / position.long_qty;

                position.long_qty -= qty;
                if position.long_qty == Decimal::ZERO {
                    position.long_entry_price = Decimal::ZERO;
                }
                position.long_margin -= released_margin;

                // 结算盈亏 + 释放保证金 + 扣除手续费
                self.wallet_balance += realized_pnl + released_margin - fee;

                Ok((order_id, realized_pnl, released_margin, fee))
            }
        }
    }

    /// 爆仓检测
    ///
    /// Cross Margin 强平规则: Margin Balance < Maintenance Margin
    pub fn check_liquidation(&self) -> bool {
        self.total_equity() < self.total_maint_margin()
    }

    /// 获取账户摘要
    pub fn account_summary(&self) -> ExchangeAccount {
        ExchangeAccount {
            account_id: "shadow_account".to_string(),
            total_equity: self.total_equity(),
            available: self.available_balance(),
            frozen_margin: self.frozen_margin(),
            unrealized_pnl: self.total_unrealized_pnl(),
            update_ts: chrono::Utc::now().timestamp(),
        }
    }

    /// 获取持仓详情（带当前价格计算的未实现盈亏）
    pub fn get_position_detail(&self, symbol: &str) -> Option<ExchangePosition> {
        let pos = self.positions.get(symbol)?;
        let current_price = self.price_map.get(symbol).copied().unwrap_or(Decimal::ZERO);

        Some(ExchangePosition {
            symbol: pos.symbol.clone(),
            long_qty: pos.long_qty,
            long_avg_price: pos.long_entry_price,
            short_qty: pos.short_qty,
            short_avg_price: pos.short_entry_price,
            unrealized_pnl: pos.unrealized_pnl(current_price),
            margin_used: pos.total_margin(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ShadowConfig {
        ShadowConfig::default()
    }

    #[test]
    fn test_account_creation() {
        let config = create_test_config();
        let account = ShadowAccount::new(dec!(100000.0), &config);

        assert_eq!(account.wallet_balance, dec!(100000.0));
        assert_eq!(account.total_equity(), dec!(100000.0));
        assert_eq!(account.available_balance(), dec!(100000.0));
    }

    #[test]
    fn test_open_long() {
        let config = create_test_config();
        let mut account = ShadowAccount::new(dec!(100000.0), &config);

        let result = account.open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), 10);
        assert!(result.is_ok());

        let (_, price, qty, fee) = result.unwrap();
        assert_eq!(price, dec!(50000.0));
        assert_eq!(qty, dec!(0.1));
        assert!(fee > Decimal::ZERO);

        // 余额应该减少（手续费）
        assert!(account.wallet_balance < dec!(100000.0));
    }

    #[test]
    fn test_open_and_update_price() {
        let config = create_test_config();
        let mut account = ShadowAccount::new(dec!(100000.0), &config);

        // 开多仓
        account.open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), 10).unwrap();

        // 更新价格到 51000
        account.update_price("BTCUSDT", dec!(51000.0));

        // 未实现盈亏 = (51000 - 50000) * 0.1 = 100
        assert_eq!(account.total_unrealized_pnl(), dec!(100.0));
    }

    #[test]
    fn test_close_long() {
        let config = create_test_config();
        let mut account = ShadowAccount::new(dec!(100000.0), &config);

        // 开多仓
        account.open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), 10).unwrap();

        // 平多仓
        let result = account.close("BTCUSDT", Side::Sell, dec!(0.1), dec!(51000.0));
        assert!(result.is_ok());

        let (_, realized_pnl, _, _) = result.unwrap();
        // 已实现盈亏 = (51000 - 50000) * 0.1 = 100
        assert_eq!(realized_pnl, dec!(100.0));
    }

    #[test]
    fn test_insufficient_balance() {
        let config = create_test_config();
        let mut account = ShadowAccount::new(dec!(1000.0), &config);

        // 尝试开一个超过余额的仓位
        let result = account.open("BTCUSDT", Side::Buy, dec!(1.0), dec!(50000.0), 10);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RejectReason::InsufficientBalance);
    }
}
