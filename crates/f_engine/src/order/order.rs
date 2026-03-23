use a_common::{EngineError, OrderResult, OrderStatus};
use crate::gateway::ExchangeGateway;
use crate::types::{OrderRequest, OrderType, Side, TradingDecision, TradingAction};
use e_risk_monitor::risk::RiskPreChecker;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::sync::Arc;

/// 订单执行器
///
/// 负责将经过风控的订单发送到交易所执行。
///
/// 设计原则:
/// - 高频路径无锁：风控预检在锁外执行
/// - 增量计算 O(1)：订单执行后直接更新持仓
///
/// 线程安全: gateway 和 risk_checker 都是线程安全的
pub struct OrderExecutor {
    gateway: Arc<dyn ExchangeGateway>,
    risk_checker: Arc<RiskPreChecker>,
}

impl OrderExecutor {
    /// 创建新的订单执行器
    pub fn new(gateway: Arc<dyn ExchangeGateway>, risk_checker: Arc<RiskPreChecker>) -> Self {
        Self { gateway, risk_checker }
    }

    /// 执行交易决策
    ///
    /// 流程:
    /// 1. RiskPreChecker 锁外预检
    /// 2. 构造 OrderRequest
    /// 3. 调用 gateway.place_order()
    /// 4. 返回订单结果
    pub fn execute(
        &self,
        symbol: &str,
        side: Side,
        qty: Decimal,
        price: Decimal,
        order_type: OrderType,
    ) -> Result<OrderResult, EngineError> {
        // 1. 构造订单请求
        let req = OrderRequest {
            symbol: symbol.to_string(),
            side,
            order_type,
            qty,
            price: Some(price),
        };

        // 2. 获取账户信息用于风控预检
        let account = self.gateway.get_account()?;
        let order_value = qty * price;

        // 3. 风控预检 (锁外执行)
        self.risk_checker.pre_check(
            symbol,
            account.available,
            order_value,
            account.total_equity,
        )?;

        // 4. 调用网关执行订单
        self.gateway.place_order(req)
    }

    /// 执行市价单
    pub fn execute_market_order(
        &self,
        symbol: &str,
        side: Side,
        qty: Decimal,
        price: Decimal,
    ) -> Result<OrderResult, EngineError> {
        self.execute(symbol, side, qty, price, OrderType::Market)
    }

    /// 执行限价单
    pub fn execute_limit_order(
        &self,
        symbol: &str,
        side: Side,
        qty: Decimal,
        price: Decimal,
    ) -> Result<OrderResult, EngineError> {
        self.execute(symbol, side, qty, price, OrderType::Limit)
    }

    /// 从交易决策执行订单
    pub fn execute_from_decision(
        &self,
        decision: &TradingDecision,
    ) -> Result<OrderResult, EngineError> {
        match decision.action {
            TradingAction::Long => {
                self.execute_market_order(
                    &decision.symbol,
                    Side::Buy,  // 开多用 Buy
                    decision.qty,
                    decision.price,
                )
            }
            TradingAction::Short => {
                self.execute_market_order(
                    &decision.symbol,
                    Side::Sell,  // 开空用 Sell
                    decision.qty,
                    decision.price,
                )
            }
            TradingAction::Flat => {
                // 平仓 - 需要判断方向，这里简化处理
                Ok(OrderResult {
                    order_id: String::new(),
                    status: OrderStatus::Cancelled,
                    filled_qty: Decimal::ZERO,
                    filled_price: Decimal::ZERO,
                    commission: Decimal::ZERO,
                    reject_reason: None,
                    message: "Flat action - manual close required".to_string(),
                })
            }
            TradingAction::Hedge => {
                // 对冲 - 简化处理
                Ok(OrderResult {
                    order_id: String::new(),
                    status: OrderStatus::Cancelled,
                    filled_qty: Decimal::ZERO,
                    filled_price: Decimal::ZERO,
                    commission: Decimal::ZERO,
                    reject_reason: None,
                    message: "Hedge action - not implemented".to_string(),
                })
            }
            TradingAction::Wait => {
                // 无操作
                Ok(OrderResult {
                    order_id: String::new(),
                    status: OrderStatus::Cancelled,
                    filled_qty: Decimal::ZERO,
                    filled_price: Decimal::ZERO,
                    commission: Decimal::ZERO,
                    reject_reason: None,
                    message: "Wait".to_string(),
                })
            }
        }
    }
}
