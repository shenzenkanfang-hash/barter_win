//! h_15m/executor.rs
//!
//! 下单网关 - 交易所交互 + 风控前置检查

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::atomic::{AtomicU64, Ordering};
use x_data::position::PositionSide;
use e_risk_monitor::RiskPreChecker;

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

    #[error("CAS 重试超限")]
    CasRetryExceeded,
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
    /// 风控预检器（可选，不提供时跳过风控）
    risk_checker: Option<RiskPreChecker>,
}

impl Executor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            config,
            last_order_ms: AtomicU64::new(0),
            risk_checker: None,
        }
    }

    /// 创建带风控的 Executor
    pub fn with_risk_checker(config: ExecutorConfig, risk_checker: RiskPreChecker) -> Self {
        Self {
            config,
            last_order_ms: AtomicU64::new(0),
            risk_checker: Some(risk_checker),
        }
    }

    /// 频率限制检查（原子操作，CAS 循环）
    ///
    /// 最大重试次数防止高并发场景下无限自旋
    pub fn rate_limit_check(&self, interval_ms: u64) -> Result<(), ExecutorError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        const MAX_CAS_RETRIES: usize = 10;

        for _ in 0..MAX_CAS_RETRIES {
            let last = self.last_order_ms.load(Ordering::Relaxed);

            if now.saturating_sub(last) < interval_ms {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    last_ms = last,
                    now_ms = now,
                    interval_ms = interval_ms,
                    "下单频率过高，跳过"
                );
                return Err(ExecutorError::RateLimited);
            }

            match self.last_order_ms.compare_exchange_weak(
                last, now,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => {
                    // CAS 失败，重试
                    continue;
                }
            }
        }

        tracing::error!(
            symbol = %self.config.symbol,
            "CAS 重试超限，频率限制检查失败"
        );
        Err(ExecutorError::CasRetryExceeded)
    }

    /// 计算下单数量（Decimal 精度 + 步长裁剪）
    #[allow(unused_variables)]
    pub fn calculate_order_qty(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>, // 预留：将用于仓位方向风控
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
    ///
    /// 流程:
    /// 1. 频率限制检查
    /// 2. 计算下单数量
    /// 3. 风控前置检查
    /// 4. 记录订单日志
    pub fn send_order(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
        order_value: Decimal,
        available_balance: Decimal,
        total_equity: Decimal,
    ) -> Result<Decimal, ExecutorError> {
        // 1. 频率限制
        self.rate_limit_check(self.config.order_interval_ms)?;

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

        // 3. 风控前置检查
        if let Some(ref checker) = self.risk_checker {
            if let Err(e) = checker.pre_check(
                &self.config.symbol,
                available_balance,
                order_value,
                total_equity,
            ) {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    order_type = ?order_type,
                    order_value = %order_value,
                    error = %e,
                    "风控前置检查拒绝"
                );
                return Err(ExecutorError::RiskCheckFailed(e.to_string()));
            }
        }

        tracing::info!(
            symbol = %self.config.symbol,
            order_type = ?order_type,
            qty = %qty,
            "下单请求"
        );

        Ok(qty)
    }

    /// 发送订单（简化版，无风控参数，用于向后兼容）
    #[deprecated(note = "请使用 send_order_full 版本提供完整风控参数")]
    pub fn send_order_simple(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Result<(), ExecutorError> {
        self.send_order(order_type, current_qty, current_side, Decimal::ZERO, Decimal::MAX, Decimal::MAX)?;
        Ok(())
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(ExecutorConfig::default())
    }
}
