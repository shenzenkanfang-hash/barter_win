#![forbid(unsafe_code)]

//! MockBinanceGateway - 模拟币安网关实现
//!
//! 用于测试和回测环境，提供简化的订单执行模拟。

use crate::gateway::ExchangeGateway;
use crate::types::{OrderRequest, Side, OrderType};
use a_common::{EngineError, ExchangeAccount, ExchangePosition, OrderResult, OrderStatus};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::{info, warn};

/// Mock 账户
#[derive(Debug, Clone)]
pub struct MockAccount {
    pub account_id: String,
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
}

impl MockAccount {
    pub fn new(account_id: String, initial_balance: Decimal) -> Self {
        Self {
            account_id,
            total_equity: initial_balance,
            available: initial_balance,
            frozen_margin: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
        }
    }
}

/// Mock 持仓
#[derive(Debug, Clone)]
pub struct MockPosition {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
}

impl MockPosition {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            long_qty: Decimal::ZERO,
            long_avg_price: Decimal::ZERO,
            short_qty: Decimal::ZERO,
            short_avg_price: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
        }
    }
}

/// MockBinanceGateway - 模拟币安网关
pub struct MockBinanceGateway {
    account: RwLock<MockAccount>,
    positions: RwLock<FnvHashMap<String, MockPosition>>,
    next_order_id: RwLock<u64>,
}

impl MockBinanceGateway {
    /// 创建新的 MockBinanceGateway
    pub fn new(initial_balance: Decimal) -> Self {
        Self {
            account: RwLock::new(MockAccount::new("mock_account_001".to_string(), initial_balance)),
            positions: RwLock::new(FnvHashMap::default()),
            next_order_id: RwLock::new(1),
        }
    }

    /// 生成订单ID
    fn generate_order_id(&self) -> String {
        let mut counter = self.next_order_id.write();
        let id = format!("M{:06}", *counter);
        *counter += 1;
        id
    }

    /// 获取账户信息
    pub fn get_account(&self) -> MockAccount {
        self.account.read().clone()
    }

    /// 获取持仓
    pub fn get_position(&self, symbol: &str) -> Option<MockPosition> {
        self.positions.read().get(symbol).cloned()
    }
}

impl ExchangeGateway for MockBinanceGateway {
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> {
        let order_id = self.generate_order_id();
        let price = req.price.unwrap_or(Decimal::ZERO);
        let filled_qty = req.qty;
        let filled_price = price;

        // 计算手续费 (Taker 0.04%)
        let commission = filled_qty * filled_price * dec!(0.0004);

        // 更新账户
        {
            let mut account = self.account.write();
            let order_value = filled_qty * filled_price;
            account.available -= order_value;
            account.frozen_margin += order_value;
        }

        // 更新持仓
        {
            let mut positions = self.positions.write();
            let position = positions.entry(req.symbol.clone()).or_insert_with(|| {
                MockPosition::new(req.symbol.clone())
            });

            match req.side {
                Side::Buy => {
                    let total_cost = position.long_qty * position.long_avg_price + filled_qty * filled_price;
                    let total_qty = position.long_qty + filled_qty;
                    position.long_avg_price = if total_qty > Decimal::ZERO {
                        total_cost / total_qty
                    } else {
                        Decimal::ZERO
                    };
                    position.long_qty = total_qty;
                }
                Side::Sell => {
                    let total_cost = position.short_qty * position.short_avg_price + filled_qty * filled_price;
                    let total_qty = position.short_qty + filled_qty;
                    position.short_avg_price = if total_qty > Decimal::ZERO {
                        total_cost / total_qty
                    } else {
                        Decimal::ZERO
                    };
                    position.short_qty = total_qty;
                }
            }
        }

        info!(
            "Mock订单成交: {} {:?} {}@{} 手续费:{}",
            order_id, req.side, filled_qty, filled_price, commission
        );

        Ok(OrderResult {
            order_id,
            status: OrderStatus::Filled,
            filled_qty,
            filled_price,
            commission,
            reject_reason: None,
            message: "Mock成交成功".to_string(),
        })
    }

    fn get_account(&self) -> Result<ExchangeAccount, EngineError> {
        let mock = self.account.read().clone();
        Ok(ExchangeAccount {
            account_id: mock.account_id,
            total_equity: mock.total_equity,
            available: mock.available,
            frozen_margin: mock.frozen_margin,
            unrealized_pnl: mock.unrealized_pnl,
        })
    }

    fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError> {
        let positions = self.positions.read();
        if let Some(pos) = positions.get(symbol) {
            Ok(Some(ExchangePosition {
                symbol: pos.symbol.clone(),
                long_qty: pos.long_qty,
                long_avg_price: pos.long_avg_price,
                short_qty: pos.short_qty,
                short_avg_price: pos.short_avg_price,
            }))
        } else {
            Ok(None)
        }
    }
}
