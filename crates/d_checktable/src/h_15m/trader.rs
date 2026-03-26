//! trader.rs - 交易器（自包含实例）
//!
//! ## 概述
//!
//! 对标 Python 版本的 `singleAssetTrader`，一个实例包含：
//! - 完整的初始化（交易规则、账户信息等）
//! - 自我控制的循环（`_run_loop`）
//! - 心跳上报（`health_check`）
//!
//! ## 设计
//!
//! ```text
//! Trader::new(symbol)
//!     │
//!     ▼
//! startloop() ──── spawn thread ────► _run_loop()
//!     │                                    │
//!     │◄──── is_running = false ──────────┘
//!     │
//! stoploop()
//!     │
//!     ▼
//! health_check() ──── 返回状态快照
//! ```

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::time::{sleep, Duration};

// ============================================================================
// 状态枚举
// ============================================================================

/// 交易器状态
///
/// ```text
/// Initial ──startloop()──► Trading ──stoploop()──► Stopped
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 初始状态
    Initial,
    /// 运行中
    Trading,
    /// 已停止
    Stopped,
}

impl Default for Status {
    fn default() -> Self {
        Status::Initial
    }
}

// ============================================================================
// 配置
// ============================================================================

/// 交易器配置
#[derive(Debug, Clone)]
pub struct Config {
    /// 交易品种
    pub symbol: String,
    /// 循环间隔（毫秒）
    pub interval_ms: u64,
    /// 数据超时时间（秒）
    pub data_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 100,      // 100ms 循环
            data_timeout_secs: 180, // 3分钟超时
        }
    }
}

// ============================================================================
// 交易器主体
// ============================================================================

/// 单币种交易器
///
/// 自包含实例，包含：
/// - 初始化（交易规则、账户信息）
/// - 自我控制的循环
/// - 心跳上报
pub struct Trader {
    /// 配置
    pub config: Config,

    /// 当前状态
    pub status: Status,

    /// 是否运行（原子标志，跨线程安全）
    is_running: Arc<AtomicBool>,

    /// 运行时信息（线程安全）
    runtime: Mutex<RuntimeInfo>,
}

impl Default for Trader {
    fn default() -> Self {
        Self::new("BTCUSDT")
    }
}

/// 运行时信息
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    /// 启动时间
    pub started_at: Option<i64>,
    /// 最后一次执行时间
    pub last_execute_at: Option<i64>,
    /// 执行次数
    pub execute_count: u64,
    /// 连续错误数
    pub consecutive_errors: u32,
    /// 最后错误信息
    pub last_error: Option<String>,
}

impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            started_at: None,
            last_execute_at: None,
            execute_count: 0,
            consecutive_errors: 0,
            last_error: None,
        }
    }
}

// ============================================================================
// 方法实现
// ============================================================================

impl Trader {
    /// 创建新的交易器
    pub fn new(symbol: &str) -> Self {
        Self {
            config: Config {
                symbol: symbol.to_string(),
                ..Default::default()
            },
            status: Status::Initial,
            is_running: Arc::new(AtomicBool::new(false)),
            runtime: Mutex::new(RuntimeInfo::default()),
        }
    }

    /// 启动交易循环（在独立线程中运行）
    pub fn startloop(&mut self) {
        if self.is_running.load(Ordering::SeqCst) {
            tracing::warn!("[{}] Trader already running", self.config.symbol);
            return;
        }

        self.is_running.store(true, Ordering::SeqCst);
        self.status = Status::Trading;
        self.runtime.lock().started_at = Some(chrono::Utc::now().timestamp());

        tracing::info!("[{}] Trading loop started", self.config.symbol);

        // 克隆数据用于新线程
        let is_running = self.is_running.clone();
        let config = self.config.clone();

        std::thread::spawn(move || {
            tracing::info!("[{}] Thread started", config.symbol);

            while is_running.load(Ordering::SeqCst) {
                // TODO: 调用注入的 execute 函数
                std::thread::sleep(Duration::from_millis(config.interval_ms).into());
            }

            tracing::info!("[{}] Thread stopped", config.symbol);
        });
    }

    /// 直接运行（当前线程）
    pub async fn run(&mut self) {
        if self.is_running.load(Ordering::SeqCst) {
            tracing::warn!("[{}] Trader already running", self.config.symbol);
            return;
        }

        self.is_running.store(true, Ordering::SeqCst);
        self.status = Status::Trading;
        self.runtime.lock().started_at = Some(chrono::Utc::now().timestamp());

        tracing::info!("[{}] Trading started", self.config.symbol);

        while self.is_running.load(Ordering::SeqCst) {
            self.execute().await;
            sleep(Duration::from_millis(self.config.interval_ms)).await;
        }

        tracing::info!("[{}] Trading stopped", self.config.symbol);
    }

    /// 停止交易循环
    pub fn stoploop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.status = Status::Stopped;
        tracing::info!("[{}] Stop requested", self.config.symbol);
    }

    /// 检查是否运行中
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// 执行一次交易逻辑
    ///
    /// 这是业务逻辑的注入点。
    pub async fn execute(&mut self) {
        // TODO: 业务逻辑注入点
        // 
        // 示例：
        // 1. 获取市场数据
        // let market = fetch_market_data(&self.config.symbol).await;
        //
        // 2. 获取持仓数据
        // let position = fetch_position(&self.config.symbol).await;
        //
        // 3. 生成信号
        // let signal = self.strategy.generate(&market, &position);
        //
        // 4. 发送订单
        // if let Some(s) = signal {
        //     self.order_sender.send(s).await;
        // }

        // 更新运行时信息
        let mut runtime = self.runtime.lock();
        runtime.execute_count += 1;
        runtime.last_execute_at = Some(chrono::Utc::now().timestamp());
    }

    /// 心跳检查
    ///
    /// 返回当前交易器的健康状态快照，用于监控。
    pub fn health_check(&self) -> HealthCheck {
        let runtime = self.runtime.lock();

        HealthCheck {
            symbol: self.config.symbol.clone(),
            status: format!("{:?}", self.status),
            is_running: self.is_running(),
            started_at: runtime.started_at,
            last_execute_at: runtime.last_execute_at,
            execute_count: runtime.execute_count,
            consecutive_errors: runtime.consecutive_errors,
            last_error: runtime.last_error.clone(),
        }
    }

    /// 记录错误
    ///
    /// 内部调用，记录连续错误。
    /// 连续错误超过阈值时自动停止。
    pub fn record_error(&mut self, error: &str) {
        let mut runtime = self.runtime.lock();
        runtime.consecutive_errors += 1;
        runtime.last_error = Some(error.to_string());

        // 连续错误超过 10 次则停止
        if runtime.consecutive_errors >= 10 {
            tracing::error!(
                "[{}] Too many consecutive errors ({}), stopping",
                self.config.symbol,
                runtime.consecutive_errors
            );
            drop(runtime); // 释放锁
            self.stoploop();
        }
    }

    /// 重置错误计数
    ///
    /// 成功执行后调用。
    pub fn reset_errors(&mut self) {
        let mut runtime = self.runtime.lock();
        runtime.consecutive_errors = 0;
        runtime.last_error = None;
    }
}

// ============================================================================
// 健康检查结果
// ============================================================================

/// 健康检查结果
///
/// 用于监控接口，返回交易器的当前状态快照。
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// 品种代码
    pub symbol: String,
    /// 状态描述
    pub status: String,
    /// 是否运行中
    pub is_running: bool,
    /// 启动时间戳
    pub started_at: Option<i64>,
    /// 最后执行时间戳
    pub last_execute_at: Option<i64>,
    /// 累计执行次数
    pub execute_count: u64,
    /// 连续错误数
    pub consecutive_errors: u32,
    /// 最后错误信息
    pub last_error: Option<String>,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_trader() {
        let trader = Trader::new("BTCUSDT");
        assert_eq!(trader.config.symbol, "BTCUSDT");
        assert_eq!(trader.status, Status::Initial);
        assert!(!trader.is_running());
    }

    #[test]
    fn test_start_stop() {
        let mut trader = Trader::new("BTCUSDT");

        trader.startloop();
        assert!(trader.is_running());
        assert_eq!(trader.status, Status::Trading);

        trader.stoploop();
        assert!(!trader.is_running());
        assert_eq!(trader.status, Status::Stopped);
    }

    #[test]
    fn test_health_check() {
        let trader = Trader::new("BTCUSDT");
        let health = trader.health_check();

        assert_eq!(health.symbol, "BTCUSDT");
        assert_eq!(health.status, "Initial");
        assert!(!health.is_running);
        assert_eq!(health.execute_count, 0);
    }
}
