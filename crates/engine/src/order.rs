use crate::error::EngineError;
use strategy::types::{OrderRequest, OrderType, Side};

pub struct OrderExecutor;

impl OrderExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn execute_market_order(
        &self,
        order: &OrderRequest,
    ) -> Result<(), EngineError> {
        match order.side {
            Side::Long => {
                // 执行买入开多
                Ok(())
            }
            Side::Short => {
                // 执行卖出开空
                Ok(())
            }
        }
    }

    pub fn execute_limit_order(
        &self,
        order: &OrderRequest,
    ) -> Result<(), EngineError> {
        match order.order_type {
            OrderType::Limit => {
                // 执行限价单
                Ok(())
            }
            OrderType::Market => {
                // 市场单不进入这里
                Err(EngineError::OrderExecutionFailed(
                    "Market order should use execute_market_order".to_string(),
                ))
            }
        }
    }
}
