//! trader.rs - 交易容器框架
//!
//! 纯框架，无业务逻辑
//! 业务逻辑由外部注入

#![forbid(unsafe_code)]

use std::future::Future;
use std::pin::Pin;
use tokio::time::{sleep, Duration};

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
}

impl Trader {
    pub fn new(symbol: &str) -> Self {
        Self {
            config: Config {
                symbol: symbol.to_string(),
                ..Default::default()
            },
            status: Status::Initial,
        }
    }

    /// execute：业务逻辑由外部注入
    pub fn execute(&mut self) {
        // TODO: 业务逻辑
    }

    pub fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.symbol.clone(),
            status: format!("{:?}", self.status),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraderHealth {
    pub symbol: String,
    pub status: String,
}

/// ==================== 自循环框架 ====================

/// 数据获取函数
pub type DataFn = Box<dyn Fn(&str) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// 订单发送函数
pub type OrderFn = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// 启动自循环
pub async fn run_loop(symbol: &str, interval_ms: u64) {
    let mut trader = Trader::new(symbol);
    tracing::info!("[{}] Loop started", symbol);

    loop {
        // 1. execute
        trader.execute();

        // 2. 等待
        sleep(Duration::from_millis(interval_ms)).await;
    }
}
