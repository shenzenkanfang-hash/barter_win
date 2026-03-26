//! order_executor.rs - 下单执行
//!
//! 执行订单 + 状态更新

#![forbid(unsafe_code)]

use x_data::trading::signal::StrategySignal;
use super::PinStatus;

/// 订单执行结果
#[derive(Debug)]
pub struct OrderResult {
    pub order_id: String,
    pub symbol: String,
    pub success: bool,
    pub avg_price: Option<rust_decimal::Decimal>,
    pub filled_qty: Option<rust_decimal::Decimal>,
}

/// 下单执行器
pub struct OrderExecutor;

impl OrderExecutor {
    /// 执行订单
    pub async fn execute(signal: &StrategySignal) -> Result<OrderResult, OrderError> {
        tracing::info!("[OrderExecutor] Executing: {:?}", signal);

        // TODO: 实际调用交易所API
        // let order = exchange_gateway.send_order(signal).await?;

        // 模拟返回
        let order_id = format!("ORDER_{}_{}", signal.strategy_id.instance_id, chrono::Utc::now().timestamp_millis());

        Ok(OrderResult {
            order_id,
            symbol: signal.strategy_id.instance_id.clone(),
            success: true,
            avg_price: Some(signal.target_price),
            filled_qty: Some(signal.quantity),
        })
    }

    /// 下单后更新状态
    pub fn update_status(signal: &StrategySignal) -> PinStatus {
        match signal.command {
            x_data::trading::signal::TradeCommand::Open => {
                match signal.direction {
                    x_data::position::PositionSide::Long => PinStatus::LongFirstOpen,
                    x_data::position::PositionSide::Short => PinStatus::ShortFirstOpen,
                    _ => PinStatus::Initial,
                }
            }
            x_data::trading::signal::TradeCommand::Add => {
                match signal.direction {
                    x_data::position::PositionSide::Long => PinStatus::LongDoubleAdd,
                    x_data::position::PositionSide::Short => PinStatus::ShortDoubleAdd,
                    _ => PinStatus::Initial,
                }
            }
            x_data::trading::signal::TradeCommand::FlatPosition 
            | x_data::trading::signal::TradeCommand::FlatAll => {
                PinStatus::Initial
            }
            x_data::trading::signal::TradeCommand::HedgeOpen => {
                PinStatus::HedgeEnter
            }
            x_data::trading::signal::TradeCommand::HedgeClose => {
                PinStatus::PosLocked
            }
            _ => PinStatus::Initial,
        }
    }
}

/// 订单错误
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("Order rejected: {0}")]
    Rejected(String),

    #[error("Order timeout")]
    Timeout,

    #[error("Network error: {0}")]
    Network(String),

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Position limit exceeded")]
    PositionLimitExceeded,
}
