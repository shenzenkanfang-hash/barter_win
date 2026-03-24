//! Mock Exchange Gateway - 模拟交易所网关
//!
//! 用于测试环境，模拟真实的交易所行为

#![forbid(unsafe_code)]

use a_common::{EngineError, ExchangeAccount, ExchangePosition, OrderResult, OrderStatus};
use f_engine::order::gateway::ExchangeGateway;
use f_engine::types::{OrderRequest, Side};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Mock 交易所网关 - 用于测试
pub struct MockExchangeGateway {
    /// 账户余额
    account: RwLock<ExchangeAccount>,
    /// 持仓列表
    positions: RwLock<HashMap<String, ExchangePosition>>,
    /// 订单历史
    orders: RwLock<Vec<OrderResult>>,
    /// 拒绝订单的模拟开关
    should_reject: RwLock<bool>,
    /// 拒绝原因
    reject_reason: RwLock<Option<String>>,
}

impl MockExchangeGateway {
    pub fn new(initial_balance: Decimal) -> Self {
        Self {
            account: RwLock::new(ExchangeAccount::new("test_account".to_string(), initial_balance)),
            positions: RwLock::new(HashMap::new()),
            orders: RwLock::new(Vec::new()),
            should_reject: RwLock::new(false),
            reject_reason: RwLock::new(None),
        }
    }

    /// 创建默认测试网关 (10000 USDT)
    pub fn default_test() -> Self {
        Self::new(dec!(10000))
    }

    /// 模拟拒绝订单
    pub fn set_reject(&self, reason: Option<String>) {
        *self.should_reject.write() = reason.is_some();
        *self.reject_reason.write() = reason;
    }

    /// 获取账户信息
    pub fn get_account_info(&self) -> ExchangeAccount {
        self.account.read().clone()
    }

    /// 获取持仓
    pub fn get_position_info(&self, symbol: &str) -> Option<ExchangePosition> {
        self.positions.read().get(symbol).cloned()
    }

    /// 获取所有持仓
    pub fn get_all_positions(&self) -> Vec<ExchangePosition> {
        self.positions.read().values().cloned().collect()
    }

    /// 获取订单数量
    pub fn order_count(&self) -> usize {
        self.orders.read().len()
    }

    /// 模拟账户扣款
    pub fn deduct_balance(&self, amount: Decimal) -> bool {
        let mut account = self.account.write();
        if account.available >= amount {
            account.available -= amount;
            account.frozen_margin += amount;
            true
        } else {
            false
        }
    }

    /// 模拟持仓更新 (开多)
    pub fn add_long_position(&self, symbol: &str, qty: Decimal, price: Decimal) {
        let mut positions = self.positions.write();
        let pos = positions.entry(symbol.to_string()).or_insert_with(|| ExchangePosition::new(symbol.to_string()));
        let total_cost = pos.long_qty * pos.long_avg_price + qty * price;
        pos.long_qty += qty;
        pos.long_avg_price = if pos.long_qty > Decimal::ZERO {
            total_cost / pos.long_qty
        } else {
            Decimal::ZERO
        };
    }

    /// 模拟持仓更新 (开空)
    pub fn add_short_position(&self, symbol: &str, qty: Decimal, price: Decimal) {
        let mut positions = self.positions.write();
        let pos = positions.entry(symbol.to_string()).or_insert_with(|| ExchangePosition::new(symbol.to_string()));
        let total_cost = pos.short_qty * pos.short_avg_price + qty * price;
        pos.short_qty += qty;
        pos.short_avg_price = if pos.short_qty > Decimal::ZERO {
            total_cost / pos.short_qty
        } else {
            Decimal::ZERO
        };
    }

    /// 模拟平多仓
    pub fn reduce_long_position(&self, symbol: &str, qty: Decimal) {
        let mut positions = self.positions.write();
        if let Some(pos) = positions.get_mut(symbol) {
            pos.long_qty = (pos.long_qty - qty).max(Decimal::ZERO);
        }
    }

    /// 模拟平空仓
    pub fn reduce_short_position(&self, symbol: &str, qty: Decimal) {
        let mut positions = self.positions.write();
        if let Some(pos) = positions.get_mut(symbol) {
            pos.short_qty = (pos.short_qty - qty).max(Decimal::ZERO);
        }
    }

    /// 重置状态
    pub fn reset(&self) {
        *self.account.write() = ExchangeAccount::new("test_account".to_string(), dec!(10000));
        self.positions.write().clear();
        self.orders.write().clear();
        *self.should_reject.write() = false;
        *self.reject_reason.write() = None;
    }
}

impl ExchangeGateway for MockExchangeGateway {
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> {
        // 检查是否模拟拒绝
        if *self.should_reject.read() {
            return Ok(OrderResult {
                order_id: format!("mock_rejected_{}", self.orders.read().len()),
                status: OrderStatus::Rejected,
                filled_qty: Decimal::ZERO,
                filled_price: Decimal::ZERO,
                commission: Decimal::ZERO,
                reject_reason: None,
                message: self.reject_reason.read().clone().unwrap_or_else(|| "Mock rejection".to_string()),
            });
        }

        let order_value = req.qty * req.price.unwrap_or(dec!(0));
        let account = self.account.read();

        // 简单风控检查
        if order_value > account.available {
            return Ok(OrderResult {
                order_id: format!("mock_{}", self.orders.read().len()),
                status: OrderStatus::Rejected,
                filled_qty: Decimal::ZERO,
                filled_price: Decimal::ZERO,
                commission: Decimal::ZERO,
                reject_reason: None,
                message: "Insufficient balance".to_string(),
            });
        }
        drop(account);

        // 执行订单
        let order_id = format!("mock_order_{}", self.orders.read().len());
        let price = req.price.unwrap_or(dec!(0));
        let filled_price = price; // 模拟成交价为请求价

        // 更新持仓 - 需要在同一个事务中处理开仓和平仓
        {
            let mut positions = self.positions.write();
            let pos = positions.entry(req.symbol.clone()).or_insert_with(|| ExchangePosition::new(req.symbol.clone()));
            
            match req.side {
                Side::Buy => {
                    // 如果有空仓，先平空仓
                    if pos.short_qty > Decimal::ZERO {
                        let close_qty = pos.short_qty.min(req.qty);
                        pos.short_qty = (pos.short_qty - close_qty).max(Decimal::ZERO);
                        let remaining_qty = req.qty - close_qty;
                        if remaining_qty > Decimal::ZERO {
                            // 开多仓
                            let total_cost = pos.long_qty * pos.long_avg_price + remaining_qty * filled_price;
                            pos.long_qty += remaining_qty;
                            pos.long_avg_price = if pos.long_qty > Decimal::ZERO {
                                total_cost / pos.long_qty
                            } else {
                                Decimal::ZERO
                            };
                        }
                    } else {
                        // 开多仓
                        let total_cost = pos.long_qty * pos.long_avg_price + req.qty * filled_price;
                        pos.long_qty += req.qty;
                        pos.long_avg_price = if pos.long_qty > Decimal::ZERO {
                            total_cost / pos.long_qty
                        } else {
                            Decimal::ZERO
                        };
                    }
                }
                Side::Sell => {
                    // 如果有多仓，先平多仓
                    if pos.long_qty > Decimal::ZERO {
                        let close_qty = pos.long_qty.min(req.qty);
                        pos.long_qty = (pos.long_qty - close_qty).max(Decimal::ZERO);
                        let remaining_qty = req.qty - close_qty;
                        if remaining_qty > Decimal::ZERO {
                            // 开空仓
                            let total_cost = pos.short_qty * pos.short_avg_price + remaining_qty * filled_price;
                            pos.short_qty += remaining_qty;
                            pos.short_avg_price = if pos.short_qty > Decimal::ZERO {
                                total_cost / pos.short_qty
                            } else {
                                Decimal::ZERO
                            };
                        }
                    } else {
                        // 开空仓
                        let total_cost = pos.short_qty * pos.short_avg_price + req.qty * filled_price;
                        pos.short_qty += req.qty;
                        pos.short_avg_price = if pos.short_qty > Decimal::ZERO {
                            total_cost / pos.short_qty
                        } else {
                            Decimal::ZERO
                        };
                    }
                }
            }
        }

        // 扣款
        self.deduct_balance(order_value);

        let result = OrderResult {
            order_id: order_id.clone(),
            status: OrderStatus::Filled,
            filled_qty: req.qty,
            filled_price,
            commission: order_value * dec!(0.0004), // 0.04% 手续费
            reject_reason: None,
            message: "Order filled".to_string(),
        };

        self.orders.write().push(result.clone());
        Ok(result)
    }

    fn get_account(&self) -> Result<ExchangeAccount, EngineError> {
        Ok(self.account.read().clone())
    }

    fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError> {
        Ok(self.positions.read().get(symbol).cloned())
    }
}
