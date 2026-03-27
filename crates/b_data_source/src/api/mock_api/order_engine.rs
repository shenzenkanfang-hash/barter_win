//! Simulator Order - 订单执行引擎
//!
//! 订单逻辑：开仓/平仓/拒绝判断
//! 调用 Account 更新状态

use rust_decimal::Decimal;

use a_common::exchange::{OrderResult, RejectReason};
use a_common::models::types::{OrderStatus, Side};
use super::config::MockConfig;
use super::account::Account;

/// 订单请求（内部使用）
#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Decimal,
    pub leverage: Decimal,
}

/// 订单引擎
///
/// 处理订单逻辑，调用 Account 更新状态
pub struct OrderEngine {
    /// 账户引用
    account: Account,
    /// 配置
    config: MockConfig,
}

impl OrderEngine {
    /// 创建新订单引擎
    pub fn new(initial_balance: Decimal, config: &MockConfig) -> Self {
        Self {
            account: Account::new(initial_balance, config),
            config: config.clone(),
        }
    }

    /// 获取账户引用
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// 获取账户可变引用
    pub fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }

    /// 更新价格
    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.account.update_price(symbol, price);
    }

    /// 获取当前价格
    pub fn get_current_price(&self, symbol: &str) -> Option<Decimal> {
        let price = self.account.get_price(symbol);
        if price == Decimal::ZERO {
            None
        } else {
            Some(price)
        }
    }

    /// 检查是否为平仓
    pub fn is_closing(&self, symbol: &str, side: Side) -> bool {
        self.account.get_position(symbol)
            .map(|p| match side {
                Side::Buy => p.short_qty > Decimal::ZERO,
                Side::Sell => p.long_qty > Decimal::ZERO,
            })
            .unwrap_or(false)
    }

    /// 执行订单
    pub fn execute(&mut self, req: OrderRequest) -> OrderResult {
        // 检查是否为平仓
        let is_closing = self.is_closing(&req.symbol, req.side);

        if is_closing {
            self.execute_close(req)
        } else {
            self.execute_open(req)
        }
    }

    /// 执行开仓
    fn execute_open(&mut self, req: OrderRequest) -> OrderResult {
        // 前置检查
        if let Err(reason) = self.account.pre_check(&req.symbol, req.qty, req.price, req.leverage) {
            return self.reject_result(reason);
        }

        // 计算手续费
        let notional = req.price * req.qty;
        let commission = notional * self.config.fee_rate;

        // 扣除手续费
        if self.account.available() < commission {
            return self.reject_result(RejectReason::InsufficientBalance);
        }

        // 更新账户状态
        self.account.apply_open(&req.symbol, req.side, req.qty, req.price, req.leverage);
        self.account.deduct_fee(commission);

        let order_id = self.account.next_order_id();

        OrderResult {
            order_id,
            status: OrderStatus::Filled,
            filled_qty: req.qty,
            filled_price: req.price,
            commission,
            reject_reason: None,
            message: "模拟成交成功".to_string(),
        }
    }

    /// 执行平仓
    fn execute_close(&mut self, req: OrderRequest) -> OrderResult {
        let position = match self.account.get_position(&req.symbol) {
            Some(p) => p.clone(),
            None => return self.reject_result(RejectReason::SymbolNotTradable),
        };

        // 检查平仓数量
        let closeable_qty = match req.side {
            Side::Buy => position.short_qty,
            Side::Sell => position.long_qty,
        };

        if req.qty > closeable_qty {
            return self.reject_result(RejectReason::PositionLimitExceeded);
        }

        // 计算手续费
        let notional = req.price * req.qty;
        let commission = notional * self.config.fee_rate;

        // 执行平仓
        let realized_pnl = self.account.apply_close(&req.symbol, req.side, req.qty, req.price);
        
        // 扣除手续费
        self.account.deduct_fee(commission);

        let order_id = self.account.next_order_id();

        OrderResult {
            order_id,
            status: OrderStatus::Filled,
            filled_qty: req.qty,
            filled_price: req.price,
            commission,
            reject_reason: None,
            message: format!("平仓成功, 实现盈亏: {}", realized_pnl),
        }
    }

    /// 生成拒绝结果
    fn reject_result(&self, reason: RejectReason) -> OrderResult {
        OrderResult {
            order_id: format!("REJECT_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()),
            status: OrderStatus::Rejected,
            filled_qty: Decimal::ZERO,
            filled_price: Decimal::ZERO,
            commission: Decimal::ZERO,
            reject_reason: Some(reason.clone()),
            message: format!("模拟拒绝: {}", reason),
        }
    }

    /// 强平检测
    pub fn check_liquidation(&self) -> bool {
        self.account.check_liquidation()
    }

    /// 获取账户信息
    pub fn get_account(&self) -> a_common::exchange::ExchangeAccount {
        self.account.to_exchange_account()
    }

    /// 获取持仓
    pub fn get_position(&self, symbol: &str) -> Option<a_common::exchange::ExchangePosition> {
        self.account.to_exchange_position(symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_open_position() {
        let mut engine = OrderEngine::new(dec!(100000.0), &MockConfig::default());
        
        let result = engine.execute(OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            qty: dec!(0.1),
            price: dec!(50000.0),
            leverage: dec!(10),
        });
        
        assert_eq!(result.status, OrderStatus::Filled);
        assert_eq!(result.filled_qty, dec!(0.1));
    }

    #[test]
    fn test_close_position() {
        let mut engine = OrderEngine::new(dec!(100000.0), &MockConfig::default());
        
        // 先开仓
        engine.execute(OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            qty: dec!(0.1),
            price: dec!(50000.0),
            leverage: dec!(10),
        });
        
        // 再平仓
        let result = engine.execute(OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Sell,
            qty: dec!(0.1),
            price: dec!(51000.0),
            leverage: dec!(1),
        });
        
        assert_eq!(result.status, OrderStatus::Filled);
    }

    #[test]
    fn test_insufficient_balance() {
        let mut engine = OrderEngine::new(dec!(1000.0), &MockConfig::default());
        
        let result = engine.execute(OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            qty: dec!(1.0), // 太大
            price: dec!(50000.0),
            leverage: dec!(10),
        });
        
        assert_eq!(result.status, OrderStatus::Rejected);
        assert!(result.reject_reason.is_some());
    }
}
