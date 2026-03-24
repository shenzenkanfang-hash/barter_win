//! ShadowBinanceGateway - 劫持模式模拟网关
//!
//! 劫持账户/持仓/下单 API 使用本地模拟
//!
//! # 特性
//! - 账户余额/持仓/订单本地模拟
//! - 真实 Binance 行情通过外部注入
//! - 线程安全设计
//!
//! # 使用方式
//! ```rust,ignore
//! let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
//!
//! // 外部行情注入
//! gateway.update_price("BTCUSDT", dec!(50000.0));
//!
//! // 下单
//! let req = OrderRequest::new_market("BTCUSDT".into(), Side::Buy, dec!(0.1));
//! let result = gateway.place_order(req)?;
//! ```

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;

use a_common::exchange::{ExchangeAccount, ExchangePosition, OrderResult, RejectReason};
use a_common::EngineError;
use f_engine::types::{OrderRequest, Side, OrderType, OrderStatus};

use crate::shadow_account::{ShadowAccount, Side as ShadowSide};
use crate::shadow_config::ShadowConfig;

/// ShadowBinanceGateway - 劫持模式网关
///
/// 账户/持仓/下单用自己的模拟账户，其他 API 转发到真实 Binance
pub struct ShadowBinanceGateway {
    /// 模拟账户（线程安全）
    account: Arc<RwLock<ShadowAccount>>,
    /// 配置
    config: ShadowConfig,
}

impl ShadowBinanceGateway {
    /// 创建新的 ShadowBinanceGateway
    pub fn new(initial_balance: Decimal, config: ShadowConfig) -> Self {
        Self {
            account: Arc::new(RwLock::new(ShadowAccount::new(initial_balance, &config))),
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
        self.account.write().update_price(symbol, price);
    }

    /// 获取账户信息
    pub fn get_account(&self) -> Result<ExchangeAccount, EngineError> {
        Ok(self.account.read().account_summary())
    }

    /// 获取持仓
    pub fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError> {
        Ok(self.account.read().get_position_detail(symbol))
    }

    /// 下单
    pub fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> {
        let side = match req.side {
            Side::Buy => ShadowSide::Buy,
            Side::Sell => ShadowSide::Sell,
        };

        let price = req.price.unwrap_or(Decimal::ZERO);
        let leverage = 1; // 默认1倍杠杆，后续可配置

        // 检查是否为平仓
        let position = self.account.read().get_position(&req.symbol);
        let is_closing = position.map(|p| {
            match side {
                ShadowSide::Buy => p.short_qty > Decimal::ZERO,
                ShadowSide::Sell => p.long_qty > Decimal::ZERO,
            }
        }).unwrap_or(false);

        let result = if is_closing {
            // 平仓
            self.account.write().close(&req.symbol, side, req.qty, price)
        } else {
            // 开仓
            self.account.write().open(&req.symbol, side, req.qty, price, leverage)
        };

        match result {
            Ok((order_id, filled_price, filled_qty, commission)) => {
                let status = if filled_qty == req.qty {
                    OrderStatus::Filled
                } else {
                    OrderStatus::PartiallyFilled
                };

                Ok(OrderResult {
                    order_id,
                    status,
                    filled_qty,
                    filled_price,
                    commission,
                    reject_reason: None,
                    message: "模拟成交成功".to_string(),
                })
            }
            Err(reason) => {
                let status = match reason {
                    RejectReason::InsufficientBalance => OrderStatus::Rejected,
                    RejectReason::MarginInsufficient => OrderStatus::Rejected,
                    RejectReason::PositionLimitExceeded => OrderStatus::Rejected,
                    _ => OrderStatus::Rejected,
                };

                Ok(OrderResult {
                    order_id: format!("REJECT_{}", chrono::Utc::now().timestamp_millis()),
                    status,
                    filled_qty: Decimal::ZERO,
                    filled_price: Decimal::ZERO,
                    commission: Decimal::ZERO,
                    reject_reason: Some(reason),
                    message: format!("模拟拒绝: {}", reason),
                })
            }
        }
    }

    /// 爆仓检测
    pub fn check_liquidation(&self) -> bool {
        self.account.read().check_liquidation()
    }

    /// 获取账户引用（用于高级操作）
    pub fn account(&self) -> Arc<RwLock<ShadowAccount>> {
        Arc::clone(&self.account)
    }
}

impl Clone for ShadowBinanceGateway {
    fn clone(&self) -> Self {
        Self {
            account: Arc::clone(&self.account),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_shadow_gateway_creation() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
        let account = gateway.get_account().unwrap();
        
        assert_eq!(account.total_equity, dec!(100000.0));
        assert_eq!(account.available, dec!(100000.0));
    }

    #[test]
    fn test_open_position() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(100000.0));
        
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Market,
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
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Market,
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
        gateway.place_order(OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Market,
            qty: dec!(0.1),
            price: Some(dec!(50000.0)),
        }).unwrap();
        
        // 平多仓
        let close_result = gateway.place_order(OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Sell,
            order_type: OrderType::Market,
            qty: dec!(0.1),
            price: Some(dec!(51000.0)),
        }).unwrap();
        
        assert_eq!(close_result.status, OrderStatus::Filled);
    }

    #[test]
    fn test_insufficient_balance() {
        let gateway = ShadowBinanceGateway::with_default_config(dec!(1000.0));
        
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Market,
            qty: dec!(1.0), // 太大
            price: Some(dec!(50000.0)),
        };
        
        let result = gateway.place_order(req).unwrap();
        assert_eq!(result.status, OrderStatus::Rejected);
        assert!(result.reject_reason.is_some());
    }
}
