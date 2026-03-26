//! trader_flow.rs - 交易流程编排
//!
//! 整合：信号 → 一次风控 → 二次风控 → 下单 → 持久化

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use tokio::time::sleep;

use super::{Trader, TraderConfig};
use x_data::trading::signal::StrategySignal;

/// 交易流程
pub struct TraderFlow {
    trader: Arc<Trader>,
    config: TraderConfig,
}

impl TraderFlow {
    pub fn new(trader: Arc<Trader>) -> Self {
        let config = trader.get_config();
        Self { trader, config }
    }

    /// 执行交易流程
    ///
    /// 流程：
    /// 1. 生成信号（Trader 核心逻辑）
    /// 2. 检查一次风控（信号层）
    /// 3. 检查二次风控（风控层）
    /// 4. 执行下单
    /// 5. 更新状态
    /// 6. 持久化
    pub async fn execute(&self) -> Option<StrategySignal> {
        // 1. 生成信号
        let signal = self.trader.generate_signal()?;
        
        // 2. 一次风控检查（CheckTable）
        if !self.pre_risk_check(&signal) {
            tracing::warn!("[Flow {}] Pre-risk check failed", self.config.symbol);
            return None;
        }
        
        // 3. 二次风控检查（RiskMonitor）
        if !self.risk_check(&signal).await {
            tracing::warn!("[Flow {}] Risk check failed", self.config.symbol);
            return None;
        }
        
        // 4. 执行下单
        match self.execute_order(&signal).await {
            Ok(order_id) => {
                tracing::info!("[Flow {}] Order executed: {}", self.config.symbol, order_id);
                
                // 5. 更新状态
                self.trader.update_after_order(&signal);
                
                // 6. 持久化
                self.trader.save_config();
                
                Some(signal)
            }
            Err(e) => {
                tracing::error!("[Flow {}] Order failed: {}", self.config.symbol, e);
                None
            }
        }
    }

    /// 一次风控（信号层检查）
    fn pre_risk_check(&self, _signal: &StrategySignal) -> bool {
        // TODO: 调用 CheckTable 检查
        // - 交易所规则
        // - 合约规则
        // - 数量限制
        
        tracing::debug!("[Flow {}] Pre-risk check passed", self.config.symbol);
        true
    }

    /// 二次风控（风控层检查）
    async fn risk_check(&self, _signal: &StrategySignal) -> bool {
        // TODO: 调用 RiskMonitor 检查
        // - 资金余额
        // - 仓位限制
        // - 风险敞口
        
        tracing::debug!("[Flow {}] Risk check passed", self.config.symbol);
        true
    }

    /// 执行下单
    async fn execute_order(&self, signal: &StrategySignal) -> Result<String, OrderError> {
        // TODO: 调用 OrderExecutor
        // - 发送订单到交易所
        // - 等待成交确认
        
        // 模拟返回订单ID
        let order_id = format!("ORDER_{}_{}", self.config.symbol, chrono::Utc::now().timestamp_millis());
        tracing::info!("[Flow {}] Sending order: {:?}", self.config.symbol, signal);
        
        Ok(order_id)
    }

    /// 自循环入口
    pub async fn run(&self) {
        let trader = self.trader.clone();
        trader.set_running(true);
        
        tracing::info!("[TraderFlow {}] Started", self.config.symbol);
        
        while trader.is_running() {
            self.execute().await;
            sleep(Duration::from_millis(self.config.interval_ms)).await;
        }
        
        tracing::info!("[TraderFlow {}] Stopped", self.config.symbol);
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
}
