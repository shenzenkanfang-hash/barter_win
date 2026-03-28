//! MockApiGateway - 模拟API网关
//!
//! 替代真实Binance API，账户/持仓/下单走模拟账户

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use a_common::exchange::{ExchangeAccount, ExchangePosition, OrderResult, RejectReason};
use a_common::EngineError;
use a_common::models::types::Side;

use crate::api::mock_account::Account;
use crate::api::mock_config::MockConfig;

/// 订单请求
#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Decimal,
    pub leverage: Decimal,
}

/// 订单引擎
pub struct OrderEngine {
    account: Account,
    config: MockConfig,
}

impl OrderEngine {
    pub fn new(initial_balance: Decimal, config: &MockConfig) -> Self {
        Self {
            account: Account::new(initial_balance, config),
            config: config.clone(),
        }
    }

    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.account.update_price(symbol, price);
    }

    pub fn get_current_price(&self, symbol: &str) -> Option<Decimal> {
        let price = self.account.get_price(symbol);
        if price.is_zero() {
            None
        } else {
            Some(price)
        }
    }

    pub fn get_account(&self) -> ExchangeAccount {
        self.account.to_exchange_account()
    }

    pub fn get_position(&self, symbol: &str) -> Option<ExchangePosition> {
        self.account.to_exchange_position(symbol)
    }

    pub fn execute(&mut self, req: OrderRequest) -> OrderResult {
        // 前置检查
        if let Err(reason) = self.account.pre_check(&req.symbol, req.qty, req.price, req.leverage) {
            return OrderResult {
                order_id: self.account.next_order_id(),
                status: a_common::models::types::OrderStatus::Rejected,
                filled_qty: Decimal::ZERO,
                filled_price: Decimal::ZERO,
                commission: Decimal::ZERO,
                reject_reason: Some(reason),
                message: String::new(),
            };
        }

        // 计算手续费
        let commission = req.price * req.qty * self.config.fee_rate;
        self.account.deduct_fee(commission);

        // 执行开仓/平仓
        match req.side {
            Side::Buy => {
                self.account.apply_open(&req.symbol, Side::Buy, req.qty, req.price, req.leverage);
            }
            Side::Sell => {
                self.account.apply_close(&req.symbol, Side::Sell, req.qty, req.price);
            }
        }

        OrderResult {
            order_id: self.account.next_order_id(),
            status: a_common::models::types::OrderStatus::Filled,
            filled_qty: req.qty,
            filled_price: req.price,
            commission,
            reject_reason: None,
            message: String::new(),
        }
    }

    pub fn check_liquidation(&self) -> bool {
        self.account.check_liquidation()
    }
}

/// MockApiGateway - 模拟API网关
pub struct MockApiGateway {
    engine: Arc<RwLock<OrderEngine>>,
    config: MockConfig,
}

impl MockApiGateway {
    pub fn new(initial_balance: Decimal, config: MockConfig) -> Self {
        Self {
            engine: Arc::new(RwLock::new(OrderEngine::new(initial_balance, &config))),
            config,
        }
    }

    pub fn with_default_config(initial_balance: Decimal) -> Self {
        Self::new(initial_balance, MockConfig::default())
    }

    pub fn update_price(&self, symbol: &str, price: Decimal) {
        self.engine.write().update_price(symbol, price);
    }

    pub fn get_current_price(&self, symbol: &str) -> Decimal {
        self.engine.read().get_current_price(symbol).unwrap_or(Decimal::ZERO)
    }

    pub fn get_account(&self) -> Result<ExchangeAccount, EngineError> {
        Ok(self.engine.read().get_account())
    }

    pub fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError> {
        Ok(self.engine.read().get_position(symbol))
    }

    /// 下单
    pub fn place_order(&self, symbol: &str, side: Side, qty: Decimal, price: Option<Decimal>) -> Result<OrderResult, EngineError> {
        let price = price.unwrap_or_else(|| {
            self.engine.read().get_current_price(symbol).unwrap_or(Decimal::ZERO)
        });

        let req = OrderRequest {
            symbol: symbol.to_string(),
            side,
            qty,
            price,
            leverage: dec!(1),
        };

        let result = self.engine.write().execute(req);
        Ok(result)
    }

    pub fn check_liquidation(&self) -> bool {
        self.engine.read().check_liquidation()
    }

    pub fn engine(&self) -> Arc<RwLock<OrderEngine>> {
        Arc::clone(&self.engine)
    }
}

impl Clone for MockApiGateway {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            config: self.config.clone(),
        }
    }
}
