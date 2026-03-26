//! trader_flow.rs - 交易流程编排
//!
//! 串联各模块：
//! market_data → signal_generator → order_executor → status_update

#![forbid(unsafe_code)]

use std::time::Duration;

use tokio::time::sleep;

use super::PinStatus;
use super::market_data;
use super::signal_generator::SignalGenerator;
use super::order_executor::OrderExecutor;
use x_data::trading::signal::StrategySignal;

/// 交易流程
pub struct TraderFlow {
    pub symbol: String,
    pub interval_ms: u64,
    signal_gen: SignalGenerator,
    is_running: bool,
}

impl TraderFlow {
    pub fn new(symbol: &str, interval_ms: u64) -> Self {
        Self {
            symbol: symbol.to_string(),
            interval_ms,
            signal_gen: SignalGenerator::new(),
            is_running: false,
        }
    }

    /// 设置状态
    pub fn set_status(&mut self, status: PinStatus) {
        self.signal_gen.set_status(status);
    }

    /// 获取当前状态
    pub fn current_status(&self) -> PinStatus {
        self.signal_gen.current_status()
    }

    /// 执行一次交易流程
    pub fn execute(&mut self) -> Option<StrategySignal> {
        // 1. 读取市场数据
        let market = market_data::read_market_data(&self.symbol)?;

        // 2. 生成信号
        let signal = self.signal_gen.generate(&market)?;

        tracing::info!("[Flow {}] Signal generated: {:?}", self.symbol, signal);
        Some(signal)
    }

    /// 执行交易 + 风控
    pub async fn execute_with_risk(&mut self) -> Option<StrategySignal> {
        let signal = self.execute()?;

        // 3. 风控检查
        if !self.pre_risk_check(&signal) {
            tracing::warn!("[Flow {}] Pre-risk check failed", self.symbol);
            return None;
        }

        if !self.risk_check(&signal).await {
            tracing::warn!("[Flow {}] Risk check failed", self.symbol);
            return None;
        }

        // 4. 执行下单
        match OrderExecutor::execute(&signal).await {
            Ok(result) => {
                tracing::info!("[Flow {}] Order executed: {}", self.symbol, result.order_id);

                // 5. 更新状态
                let new_status = OrderExecutor::update_status(&signal);
                self.set_status(new_status);

                Some(signal)
            }
            Err(e) => {
                tracing::error!("[Flow {}] Order failed: {}", self.symbol, e);
                None
            }
        }
    }

    /// 一次风控
    fn pre_risk_check(&self, _signal: &StrategySignal) -> bool {
        // TODO: 调用 CheckTable
        true
    }

    /// 二次风控
    async fn risk_check(&self, _signal: &StrategySignal) -> bool {
        // TODO: 调用 RiskMonitor
        true
    }

    /// 自循环入口
    pub async fn run(&mut self) {
        self.is_running = true;
        tracing::info!("[TraderFlow {}] Started", self.symbol);

        while self.is_running {
            self.execute_with_risk().await;
            sleep(Duration::from_millis(self.interval_ms)).await;
        }

        tracing::info!("[TraderFlow {}] Stopped", self.symbol);
    }

    /// 停止
    pub fn stop(&mut self) {
        self.is_running = false;
    }

    /// 健康状态
    pub fn health(&self) -> TraderFlowHealth {
        TraderFlowHealth {
            symbol: self.symbol.clone(),
            is_running: self.is_running,
            status: self.current_status().as_str().to_string(),
        }
    }
}

/// 健康状态
#[derive(Debug)]
pub struct TraderFlowHealth {
    pub symbol: String,
    pub is_running: bool,
    pub status: String,
}
