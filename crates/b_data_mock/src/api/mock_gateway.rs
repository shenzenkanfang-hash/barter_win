//! MockApiGateway - 模拟API网关
//!
//! 替代真实Binance API，账户/持仓/下单走模拟账户

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use a_common::exchange::{ExchangeAccount, ExchangePosition, OrderResult};
use a_common::EngineError;
use a_common::models::types::Side;

use crate::api::mock_account::Account;
use crate::api::mock_config::{MockConfig, MockExecutionConfig};
use crate::api::account::state::{Balance, Position, AccountState};

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
    /// 新增：执行配置（支持细粒度控制）
    execution_config: MockExecutionConfig,
    /// 新增：账户状态（精细化余额管理）
    account_state: Arc<RwLock<AccountState>>,
}

impl MockApiGateway {
    /// 使用 MockConfig 创建（向后兼容）
    pub fn new(initial_balance: Decimal, config: MockConfig) -> Self {
        let mut account_state = AccountState::new();
        let usdt_balance = Balance::new(initial_balance);
        account_state.balances.insert("USDT".to_string(), usdt_balance);

        Self {
            engine: Arc::new(RwLock::new(OrderEngine::new(initial_balance, &config))),
            config: config.clone(),
            execution_config: MockExecutionConfig::default(),
            account_state: Arc::new(RwLock::new(account_state)),
        }
    }

    /// 使用 MockExecutionConfig 创建（新接口）
    pub fn with_execution_config(config: MockExecutionConfig) -> Self {
        let mut account_state = AccountState::new();
        let usdt_balance = Balance::new(config.initial_balance);
        account_state.balances.insert("USDT".to_string(), usdt_balance);

        // 从 execution_config 转换 fee_rate 到 MockConfig
        let mock_config = MockConfig {
            initial_balance: config.initial_balance,
            fee_rate: config.taker_fee / dec!(100),  // 百分比转小数
            slippage_rate: config.slippage,
            ..Default::default()
        };

        Self {
            engine: Arc::new(RwLock::new(OrderEngine::new(config.initial_balance, &mock_config))),
            config: mock_config,
            execution_config: config,
            account_state: Arc::new(RwLock::new(account_state)),
        }
    }

    pub fn with_default_config(initial_balance: Decimal) -> Self {
        Self::new(initial_balance, MockConfig::default())
    }

    pub fn update_price(&self, symbol: &str, price: Decimal) {
        self.engine.write().update_price(symbol, price);
        // 同时更新账户状态中的持仓盈亏
        let mut acc_state = self.account_state.write();
        if let Some(pos) = acc_state.positions.get_mut(symbol) {
            pos.update_pnl(price);
        }
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

        let result = self.engine.write().execute(req.clone());

        // 更新账户状态
        if result.status == a_common::models::types::OrderStatus::Filled {
            self.update_balance_from_order(&result, &req);
        }

        Ok(result)
    }

    pub fn check_liquidation(&self) -> bool {
        self.engine.read().check_liquidation()
    }

    pub fn engine(&self) -> Arc<RwLock<OrderEngine>> {
        Arc::clone(&self.engine)
    }

    /// 获取账户状态（精细化）
    pub fn get_account_state(&self) -> AccountState {
        self.account_state.read().clone()
    }

    /// 获取持仓（精细化）
    pub fn get_position_state(&self, symbol: &str) -> Option<Position> {
        self.account_state.read().positions.get(symbol).cloned()
    }

    /// 从订单结果更新账户状态
    fn update_balance_from_order(&self, result: &OrderResult, req: &OrderRequest) {
        let mut acc_state = self.account_state.write();

        if let Some(balance) = acc_state.balances.get_mut("USDT") {
            // 扣除手续费
            balance.deduct(result.commission);

            // 更新持仓
            let position = acc_state.get_or_create_position(&req.symbol);
            match req.side {
                Side::Buy => {
                    position.qty += req.qty;
                    if position.long_avg_price.is_zero() {
                        position.long_avg_price = req.price;
                    } else {
                        // 计算新的平均价
                        position.long_avg_price = (position.long_avg_price + req.price) / dec!(2);
                    }
                }
                Side::Sell => {
                    position.qty -= req.qty;
                    if position.short_avg_price.is_zero() {
                        position.short_avg_price = req.price;
                    } else {
                        position.short_avg_price = (position.short_avg_price + req.price) / dec!(2);
                    }
                }
            }
            position.update_pnl(req.price);
        }
    }

    /// 获取执行配置
    pub fn get_execution_config(&self) -> MockExecutionConfig {
        self.execution_config.clone()
    }

    /// 模拟网络延迟
    pub async fn simulate_latency(&self) {
        if self.execution_config.latency_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.execution_config.latency_ms)).await;
        }
    }
}

impl Clone for MockApiGateway {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            config: self.config.clone(),
            execution_config: self.execution_config.clone(),
            account_state: Arc::clone(&self.account_state),
        }
    }
}
