//! h_15m/executor.rs
//!
//! 下单网关 - 交易所交互 + 风控前置检查

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::atomic::{AtomicU64, Ordering};
use x_data::position::PositionSide;

/// Executor 错误类型
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("频率限制")]
    RateLimited,

    #[error("数量为零")]
    ZeroQuantity,

    #[error("风控拒绝: {0}")]
    RiskCheckFailed(String),

    #[error("网关错误")]
    Gateway,

    #[error("超时: {0}")]
    Timeout(String),
}

/// 下单类型枚举（对齐 Python place_order order_type）
#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    InitialOpen = 0,
    HedgeOpen   = 1,
    DoubleAdd   = 2,
    DoubleClose = 3,
    DayHedge    = 4,
    DayClose    = 5,
}

/// Executor 配置
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub symbol: String,
    pub order_interval_ms: u64,
    pub initial_ratio: Decimal,
    pub lot_size: Decimal,
    pub max_position: Decimal,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 100,
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            max_position: dec!(0.15),
        }
    }
}

/// Executor - 下单网关
pub struct Executor {
    config: ExecutorConfig,
    last_order_ms: AtomicU64,
}

impl Executor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            config,
            last_order_ms: AtomicU64::new(0),
        }
    }

    /// 频率限制检查（原子操作，CAS 循环）
    pub fn rate_limit_check(&self, interval_ms: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        loop {
            let last = self.last_order_ms.load(Ordering::Relaxed);

            if now.saturating_sub(last) < interval_ms {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    last_ms = last,
                    now_ms = now,
                    interval_ms = interval_ms,
                    "下单频率过高，跳过"
                );
                return false;
            }

            match self.last_order_ms.compare_exchange_weak(
                last, now,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    /// 计算下单数量（Decimal 精度 + 步长裁剪）
    pub fn calculate_order_qty(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Decimal {
        let raw_qty = match order_type {
            OrderType::InitialOpen => self.config.initial_ratio,
            OrderType::HedgeOpen => {
                if current_qty.abs() > Decimal::ZERO {
                    current_qty.abs()
                } else {
                    Decimal::ZERO
                }
            }
            OrderType::DoubleAdd => current_qty.abs() * dec!(0.5),
            OrderType::DoubleClose | OrderType::DayClose => current_qty.abs(),
            OrderType::DayHedge => current_qty.abs(),
        };

        self.round_to_lot_size(raw_qty)
    }

    /// 按步长裁剪数量
    fn round_to_lot_size(&self, qty: Decimal) -> Decimal {
        let step = self.config.lot_size;
        if step <= Decimal::ZERO {
            return qty;
        }
        (qty / step).floor() * step
    }

    /// 发送订单（完整流程）
    pub fn send_order(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Result<(), ExecutorError> {
        // 1. 频率限制
        if !self.rate_limit_check(self.config.order_interval_ms) {
            return Err(ExecutorError::RateLimited);
        }

        // 2. 计算数量
        let qty = self.calculate_order_qty(order_type, current_qty, current_side);
        if qty <= Decimal::ZERO {
            tracing::warn!(
                symbol = %self.config.symbol,
                order_type = ?order_type,
                "计算下单数量为 0，跳过"
            );
            return Err(ExecutorError::ZeroQuantity);
        }

        // 3. 风控前置检查（TODO: 实际实现）
        // self.pre_risk_check(qty)?;

        tracing::info!(
            symbol = %self.config.symbol,
            order_type = ?order_type,
            qty = %qty,
            "下单请求"
        );

        Ok(())
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(ExecutorConfig::default())
    }
}
