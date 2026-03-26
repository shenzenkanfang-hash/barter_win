//! trader.rs - 交易容器框架
//!
//! 纯框架，无业务逻辑
//! 业务逻辑由外部注入

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// ==================== 状态 ====================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Initial,
    Running,
    Stopped,
}

impl Default for Status {
    fn default() -> Self {
        Status::Initial
    }
}

/// ==================== 配置 ====================
#[derive(Debug, Clone)]
pub struct Config {
    pub symbol: String,
    pub interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 500,
        }
    }
}

/// ==================== 容器 ====================
pub struct Trader {
    pub config: Config,
    pub status: Status,
    running: Arc<AtomicBool>,
}

impl Trader {
    pub fn new(symbol: &str) -> Self {
        Self {
            config: Config {
                symbol: symbol.to_string(),
                ..Default::default()
            },
            status: Status::Initial,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// start：启动
    pub fn start(&mut self) {
        self.running.store(true, Ordering::SeqCst);
        self.status = Status::Running;
    }

    /// stop：停止
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.status = Status::Stopped;
    }

    /// is_running：是否运行中
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// execute：业务逻辑由外部注入
    pub async fn execute(&mut self) {
        // TODO: 业务逻辑
    }

    pub fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.symbol.clone(),
            status: format!("{:?}", self.status),
            running: self.is_running(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraderHealth {
    pub symbol: String,
    pub status: String,
    pub running: bool,
}
