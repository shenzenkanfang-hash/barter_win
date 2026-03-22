use a_common::EngineError;
use crate::gateway::ExchangeGateway;
use h_sandbox::mock_binance_gateway::OrderResult;
use d_risk_monitor::risk::RiskPreChecker;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::sync::Arc;
use crate::strategy::types::{OrderRequest, OrderType, Side};

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
        decision: &crate::strategy::types::TradingDecision,
    ) -> Result<OrderResult, EngineError> {
        use crate::strategy::types::{TradingAction, Side};

        match decision.action {
            TradingAction::OpenLong => {
                self.execute_market_order(
                    &decision.symbol,
                    Side::Long,
                    decision.qty,
                    decision.price,
                )
            }
            TradingAction::OpenShort => {
                self.execute_market_order(
                    &decision.symbol,
                    Side::Short,
                    decision.qty,
                    decision.price,
                )
            }
            TradingAction::CloseLong => {
                // 平多 - 获取当前持仓数量
                let position = self.gateway.get_position(&decision.symbol)?;
                if let Some(pos) = position {
                    let qty = pos.long_qty;
                    if qty > Decimal::ZERO {
                        self.execute_market_order(
                            &decision.symbol,
                            Side::Short,
                            qty,
                            decision.price,
                        )
                    } else {
                        Err(EngineError::OrderExecutionFailed("No long position to close".to_string()))
                    }
                } else {
                    Err(EngineError::OrderExecutionFailed("Position not found".to_string()))
                }
            }
            TradingAction::CloseShort => {
                // 平空 - 获取当前持仓数量
                let position = self.gateway.get_position(&decision.symbol)?;
                if let Some(pos) = position {
                    let qty = pos.short_qty;
                    if qty > Decimal::ZERO {
                        self.execute_market_order(
                            &decision.symbol,
                            Side::Long,
                            qty,
                            decision.price,
                        )
                    } else {
                        Err(EngineError::OrderExecutionFailed("No short position to close".to_string()))
                    }
                } else {
                    Err(EngineError::OrderExecutionFailed("Position not found".to_string()))
                }
            }
            TradingAction::NoAction => {
                // 无操作
                Ok(OrderResult {
                    order_id: String::new(),
                    status: h_sandbox::mock_binance_gateway::OrderStatus::Cancelled,
                    filled_qty: Decimal::ZERO,
                    filled_price: Decimal::ZERO,
                    commission: Decimal::ZERO,
                    reject_reason: None,
                    message: "No action".to_string(),
                })
            }
        }
    }
}
