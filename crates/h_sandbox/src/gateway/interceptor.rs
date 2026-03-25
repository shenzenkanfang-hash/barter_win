//! Gateway Interceptor - API 拦截器
//!
//! 实现 ExchangeGateway trait，拦截 API 调用
//! 账户/持仓/下单走模拟账户，其他转发到真实 Binance

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;

use a_common::exchange::{ExchangeAccount, ExchangePosition, OrderResult};
use a_common::EngineError;
use f_engine::types::{OrderRequest as EngineOrderRequest, Side as EngineSide};

use rust_decimal_macros::dec;

use crate::config::ShadowConfig;
use crate::simulator::{OrderEngine, OrderRequest, Side};

/// ShadowBinanceGateway - 劫持模式网关
///
/// 账户/持仓/下单用自己的模拟账户，其他 API 转发到真实 Binance
pub struct ShadowBinanceGateway {
    /// 订单引擎（线程安全）
    engine: Arc<RwLock<OrderEngine>>,
    /// 配置
    config: ShadowConfig,
}

impl ShadowBinanceGateway {
    /// 创建新的 ShadowBinanceGateway
    pub fn new(initial_balance: Decimal, config: ShadowConfig) -> Self {
        Self {
            engine: Arc::new(RwLock::new(OrderEngine::new(initial_balance, &config))),
            config,
        }
    }

    /// 创建默认配置的 ShadowBinanceGateway
    pub fn with_default_config(initial_balance: Decimal) -> Self {
        Self::new(initial_balance, ShadowConfig::default())
    }

    /// 更新价格（用于计算未实现盈亏）
    ///
    /// 每次行情 tick 到达时调用
    pub fn update_price(&self, symbol: &str, price: Decimal) {
        self.engine.write().update_price(symbol, price);
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
        let side = match req.side {
            EngineSide::Buy => Side::Buy,
            EngineSide::Sell => Side::Sell,
        };

        // 市价单需要使用当前市场价格
        let price = req.price.unwrap_or_else(|| {
            self.engine.read().get_current_price(&req.symbol).unwrap_or(Decimal::ZERO)
        });
        let leverage = dec!(1); // 默认1倍杠杆，后续可配置

        let order_req = OrderRequest {
            symbol: req.symbol.clone(),
            side,
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

    /// 获取订单引擎引用（用于高级操作）
    pub fn engine(&self) -> Arc<RwLock<OrderEngine>> {
        Arc::clone(&self.engine)
    }
}

impl Clone for ShadowBinanceGateway {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_gateway_creation() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
        let account = gateway.get_account().unwrap();
        
        assert_eq!(account.total_equity, dec!(100000.0));
        assert_eq!(account.available, dec!(100000.0));
    }

    #[test]
    fn test_open_position() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
        
        let req = EngineOrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: EngineSide::Buy,
            order_type: f_engine::types::OrderType::Market,
            qty: dec!(0.1),
            price: Some(dec!(50000.0)),
        };
        
        let result = gateway.place_order(req).unwrap();
        assert_eq!(result.status, OrderStatus::Filled);
        assert!(result.filled_qty > Decimal::ZERO);
    }

    #[test]
    fn test_update_price() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
        
        // 先开仓
        let req = EngineOrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: EngineSide::Buy,
            order_type: f_engine::types::OrderType::Market,
            qty: dec!(0.1),
            price: Some(dec!(50000.0)),
        };
        gateway.place_order(req).unwrap();
        
        // 更新价格
        gateway.update_price("BTCUSDT", dec!(51000.0));
        
        // 检查未实现盈亏
        let account = gateway.get_account().unwrap();
        assert_eq!(account.unrealized_pnl, dec!(100.0));
    }

    #[test]
    fn test_close_position() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
        
        // 开多仓
        gateway.place_order(EngineOrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: EngineSide::Buy,
            order_type: f_engine::types::OrderType::Market,
            qty: dec!(0.1),
            price: Some(dec!(50000.0)),
        }).unwrap();
        
        // 平多仓
        let close_result = gateway.place_order(EngineOrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: EngineSide::Sell,
            order_type: f_engine::types::OrderType::Market,
            qty: dec!(0.1),
            price: Some(dec!(51000.0)),
        }).unwrap();
        
        assert_eq!(close_result.status, OrderStatus::Filled);
    }

    #[test]
    fn test_insufficient_balance() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(1000.0));
        
        let req = EngineOrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: EngineSide::Buy,
            order_type: f_engine::types::OrderType::Market,
            qty: dec!(1.0), // 太大
            price: Some(dec!(50000.0)),
        };
        
        let result = gateway.place_order(req).unwrap();
        assert_eq!(result.status, OrderStatus::Rejected);
        assert!(result.reject_reason.is_some());
    }
}
