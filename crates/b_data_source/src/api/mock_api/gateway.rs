//! MockApiGateway - 模拟API网关
//!
//! 替代真实Binance API，账户/持仓/下单走模拟账户

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use a_common::exchange::{ExchangeAccount, ExchangePosition, OrderResult};
use a_common::EngineError;

use super::config::MockConfig;
use super::order_engine::{OrderEngine, OrderRequest as InnerOrderRequest};
use super::account::Side;

/// 订单请求（外部接口，与 f_engine::types::OrderRequest 兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineOrderRequest {
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Option<Decimal>,
}

/// MockApiGateway - 模拟API网关
///
/// 替代真实Binance API，账户/持仓/下单走模拟账户
pub struct MockApiGateway {
    engine: Arc<RwLock<OrderEngine>>,
    config: MockConfig,
}

impl MockApiGateway {
    /// 创建MockApiGateway
    pub fn new(initial_balance: Decimal, config: MockConfig) -> Self {
        Self {
            engine: Arc::new(RwLock::new(OrderEngine::new(initial_balance, &config))),
            config,
        }
    }

    /// 创建默认配置的MockApiGateway
    pub fn with_default_config(initial_balance: Decimal) -> Self {
        Self::new(initial_balance, MockConfig::default())
    }

    /// 更新价格（用于计算未实现盈亏）
    pub fn update_price(&self, symbol: &str, price: Decimal) {
        self.engine.write().update_price(symbol, price);
    }

    /// 获取当前价格
    pub fn get_current_price(&self, symbol: &str) -> Decimal {
        self.engine.read().get_current_price(symbol).unwrap_or(Decimal::ZERO)
    }

    /// 获取账户信息
    pub fn get_account(&self) -> Result<ExchangeAccount, EngineError> {
        Ok(self.engine.read().get_account())
    }

    /// 获取持仓
    pub fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError> {
        Ok(self.engine.read().get_position(symbol))
    }

    /// 下单
    pub fn place_order(&self, req: EngineOrderRequest) -> Result<OrderResult, EngineError> {
        let price = req.price.unwrap_or_else(|| {
            self.engine.read().get_current_price(&req.symbol).unwrap_or(Decimal::ZERO)
        });
        let leverage = dec!(1);

        let order_req = InnerOrderRequest {
            symbol: req.symbol,
            side: req.side,
            qty: req.qty,
            price,
            leverage,
        };

        let result = self.engine.write().execute(order_req);
        Ok(result)
    }

    /// 爆仓检测
    pub fn check_liquidation(&self) -> bool {
        self.engine.read().check_liquidation()
    }

    /// 获取订单引擎引用
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
