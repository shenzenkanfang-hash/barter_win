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
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
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

    /// 净持仓（多头 - 空头）
    pub fn net_qty(&self) -> Decimal {
        self.long_qty - self.short_qty
    }

    /// 是否有持仓
    pub fn has_position(&self) -> bool {
        self.long_qty > Decimal::ZERO || self.short_qty > Decimal::ZERO
    }

    /// 平多仓
    pub fn close_long(&mut self, qty: Decimal) {
        if qty >= self.long_qty {
            self.long_qty = Decimal::ZERO;
            self.long_avg_price = Decimal::ZERO;
        } else {
            self.long_qty -= qty;
        }
    }

    /// 平空仓
    pub fn close_short(&mut self, qty: Decimal) {
        if qty >= self.short_qty {
            self.short_qty = Decimal::ZERO;
            self.short_avg_price = Decimal::ZERO;
        } else {
            self.short_qty -= qty;
        }
    }
}

/// Mock 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockGatewayConfig {
    /// 初始账户余额
    pub initial_balance: Decimal,
    /// 手续费率 (Taker)
    pub commission_rate: Decimal,
    /// 滑点率
    pub slippage_rate: Decimal,
    /// 成交延迟（毫秒）
    pub fill_delay_ms: u64,
    /// 是否模拟成交
    pub simulate_fill: bool,
}

impl Default for MockGatewayConfig {
    fn default() -> Self {
        Self {
            initial_balance: dec!(100000.0),
            commission_rate: dec!(0.0004),
            slippage_rate: dec!(0.0001),
            fill_delay_ms: 0,
            simulate_fill: true,
        }
    }
}

/// MockBinanceGateway - 模拟币安网关
pub struct MockBinanceGateway {
    config: MockGatewayConfig,
    account: RwLock<MockAccount>,
    positions: RwLock<FnvHashMap<String, MockPosition>>,
    next_order_id: RwLock<u64>,
    orders: RwLock<FnvHashMap<String, OrderRecord>>,
}

#[derive(Debug, Clone)]
pub struct OrderRecord {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Decimal,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub filled_price: Decimal,
}

impl MockBinanceGateway {
    /// 创建新的 MockBinanceGateway（使用默认配置）
    pub fn new() -> Self {
        Self::with_config(MockGatewayConfig::default())
    }

    /// 使用配置创建 MockBinanceGateway
    pub fn with_config(config: MockGatewayConfig) -> Self {
        Self {
            config: config.clone(),
            account: RwLock::new(MockAccount::new(
                "mock_account_001".to_string(),
                config.initial_balance,
            )),
            positions: RwLock::new(FnvHashMap::default()),
            next_order_id: RwLock::new(1),
            orders: RwLock::new(FnvHashMap::default()),
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

    /// 获取所有持仓
    pub fn get_all_positions(&self) -> Vec<MockPosition> {
        self.positions.read().values().cloned().collect()
    }

    /// 获取订单记录
    pub fn get_order(&self, order_id: &str) -> Option<OrderRecord> {
        self.orders.read().get(order_id).cloned()
    }

    /// 获取订单总数
    pub fn order_count(&self) -> usize {
        self.orders.read().len()
    }

    /// 更新持仓盈亏（根据最新市场价格）
    pub fn update_pnl(&self, symbol: &str, current_price: Decimal) {
        let mut positions = self.positions.write();
        if let Some(pos) = positions.get_mut(symbol) {
            // 多头盈亏
            if pos.long_qty > Decimal::ZERO {
                let long_pnl = (current_price - pos.long_avg_price) * pos.long_qty;
                pos.unrealized_pnl = long_pnl;
            }
            // 空头盈亏
            if pos.short_qty > Decimal::ZERO {
                let short_pnl = (pos.short_avg_price - current_price) * pos.short_qty;
                pos.unrealized_pnl += short_pnl;
            }
        }
    }

    /// 更新所有持仓盈亏
    pub fn update_all_pnl(&self, prices: &FnvHashMap<String, Decimal>) {
        for (symbol, price) in prices.iter() {
            self.update_pnl(symbol, *price);
        }
    }

    /// 重置账户
    pub fn reset(&self) {
        let mut account = self.account.write();
        *account = MockAccount::new(
            "mock_account_001".to_string(),
            self.config.initial_balance,
        );
        self.positions.write().clear();
        self.orders.write().clear();
        *self.next_order_id.write() = 1;
    }
}

impl Default for MockBinanceGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl ExchangeGateway for MockBinanceGateway {
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> {
        let order_id = self.generate_order_id();

        // 如果不模拟成交，返回 Pending 状态
        if !self.config.simulate_fill {
            let record = OrderRecord {
                order_id: order_id.clone(),
                symbol: req.symbol.clone(),
                side: req.side,
                qty: req.qty,
                price: req.price.unwrap_or(Decimal::ZERO),
                status: OrderStatus::Pending,
                filled_qty: Decimal::ZERO,
                filled_price: Decimal::ZERO,
            };
            self.orders.write().insert(order_id.clone(), record);

            return Ok(OrderResult {
                order_id,
                status: OrderStatus::Pending,
                filled_qty: Decimal::ZERO,
                filled_price: Decimal::ZERO,
                commission: Decimal::ZERO,
                reject_reason: None,
                message: "Mock: Order pending".to_string(),
            });
        }

        let base_price = req.price.unwrap_or(Decimal::ZERO);
        
        // 应用滑点
        let slippage = base_price * self.config.slippage_rate;
        let filled_price = match req.side {
            Side::Buy => base_price + slippage,
            Side::Sell => base_price - slippage,
        };

        let filled_qty = req.qty;

        // 计算手续费
        let commission = filled_qty * filled_price * self.config.commission_rate;

        // 更新账户
        {
            let mut account = self.account.write();
            let order_value = filled_qty * filled_price;
            
            match req.side {
                Side::Buy => {
                    account.available -= order_value + commission;
                    account.frozen_margin += order_value;
                }
                Side::Sell => {
                    account.available += order_value - commission;
                }
            }
            
            // 更新总权益
            account.total_equity = account.available + account.frozen_margin;
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

        // 记录订单
        let record = OrderRecord {
            order_id: order_id.clone(),
            symbol: req.symbol.clone(),
            side: req.side,
            qty: req.qty,
            price: filled_price,
            status: OrderStatus::Filled,
            filled_qty,
            filled_price,
        };
        self.orders.write().insert(order_id.clone(), record);

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
                unrealized_pnl: pos.unrealized_pnl,
                margin_used: dec!(0),
            }))
        } else {
            Ok(None)
        }
    }
}
