//! trader.rs - 交易容器框架
//!
//! ## 概述
//! 
//! 本模块是分钟级策略的交易容器框架，提供：
//! - 品种交易的状态管理
//! - 生命周期控制（启动/停止）
//! - 健康检查
//!
//! ## 设计原则
//!
//! 1. **纯框架**：不包含任何业务逻辑，业务逻辑由外部在 `execute()` 中注入
//! 2. **线程安全**：使用 `Arc<AtomicBool>` 保证 `running` 状态跨线程安全
//! 3. **简单清晰**：只有 3 个核心状态，接口简单明了
//!
//! ## 使用方式
//!
//! ```rust,ignore
//! // 1. 创建 Trader
//! let mut trader = Trader::new("BTCUSDT");
//!
//! // 2. 启动
//! trader.start();
//!
//! // 3. 主循环中调用 execute
//! loop {
//!     trader.execute().await;
//!     tokio::time::sleep(Duration::from_millis(500)).await;
//! }
//!
//! // 4. 停止
//! trader.stop();
//! ```
//!
//! ## 状态流转
//!
//! ```text
//!   new()           start()          stop()
//!   ──────► Initial ─────────► Running ─────────► Stopped
//!                    ▲               │
//!                    └───────────────┘
//!                      (stop 后可重新 start)
//! ```

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ============================================================================
// 类型定义
// ============================================================================

/// 交易器状态
///
/// 代表 Trader 的生命周期状态，用于控制交易循环的启停。
///
/// # 状态说明
///
/// - **Initial**: 初始状态，`new()` 后处于此状态
/// - **Running**: 运行中，`start()` 后处于此状态
/// - **Stopped**: 已停止，`stop()` 后处于此状态
///
/// # 状态流转
///
/// ```text
/// Initial ──start()──► Running ──stop()──► Stopped
///     ▲                                     │
///     └───────────────start()───────────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 初始状态
    ///
    /// Trader 创建后的默认状态，表示尚未启动。
    Initial,

    /// 运行中
    ///
    /// 调用 `start()` 后进入此状态，表示交易循环正在执行。
    Running,

    /// 已停止
    ///
    /// 调用 `stop()` 后进入此状态，表示交易循环已停止。
    Stopped,
}

impl Default for Status {
    /// 默认状态为 Initial
    fn default() -> Self {
        Status::Initial
    }
}

// ============================================================================
// 配置
// ============================================================================

/// 交易器配置
///
/// 包含 Trader 运行所需的基础配置信息。
///
/// # 字段说明
///
/// - `symbol`: 交易品种代码，如 "BTCUSDT"
/// - `interval_ms`: 执行间隔（毫秒），默认 500ms
#[derive(Debug, Clone)]
pub struct Config {
    /// 交易品种代码
    pub symbol: String,

    /// 执行间隔（毫秒）
    ///
    /// 每次 `execute()` 执行后的等待时间。
    /// 控制交易循环的频率。
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

// ============================================================================
// 交易器主体
// ============================================================================

/// 交易器容器
///
/// 负责单个品种的交易循环管理。
///
/// # 设计说明
///
/// - **无状态业务逻辑**：Trader 只管理生命周期，不包含任何交易策略逻辑
/// - **线程安全**：`running` 使用 `Arc<AtomicBool>`，可安全跨线程共享
/// - **状态可见**：`status` 字段公开，可直接访问当前状态
///
/// # 使用示例
///
/// ```rust,ignore
/// let trader = Trader::new("BTCUSDT");
/// trader.start();
///
/// loop {
///     if !trader.is_running() {
///         break;
///     }
///     trader.execute().await;
///     sleep(Duration::from_millis(trader.config.interval_ms)).await;
/// }
/// ```
pub struct Trader {
    /// 配置信息
    pub config: Config,

    /// 当前状态
    ///
    /// 反映 Trader 的生命周期状态。
    /// 可直接访问用于监控。
    pub status: Status,

    /// 运行标志（原子布尔）
    ///
    /// 使用原子操作保证线程安全。
    /// - `true`: 运行中
    /// - `false`: 已停止
    ///
    /// 使用 `Arc` 允许跨线程共享同一次引用。
    running: Arc<AtomicBool>,
}

impl Trader {
    /// 创建新的 Trader 实例
    ///
    /// # 参数
    ///
    /// - `symbol`: 交易品种代码
    ///
    /// # 返回
    ///
    /// 新创建的 Trader，处于 `Status::Initial` 状态
    ///
    /// # 示例
    ///
    /// ```rust
    /// let trader = Trader::new("ETHUSDT");
    /// assert_eq!(trader.status, Status::Initial);
    /// assert_eq!(trader.config.symbol, "ETHUSDT");
    /// ```
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

    /// 启动交易器
    ///
    /// 将状态从 `Initial` 或 `Stopped` 切换到 `Running`。
    /// 设置 `running = true`，允许交易循环继续执行。
    ///
    /// # 效果
    ///
    /// - `status` → `Status::Running`
    /// - `running` → `true`
    ///
    /// # 示例
    ///
    /// ```rust
    /// let mut trader = Trader::new("BTCUSDT");
    /// trader.start();
    /// assert!(trader.is_running());
    /// ```
    pub fn start(&mut self) {
        self.running.store(true, Ordering::SeqCst);
        self.status = Status::Running;
    }

    /// 停止交易器
    ///
    /// 将状态从 `Running` 切换到 `Stopped`。
    /// 设置 `running = false`，交易循环应立即退出。
    ///
    /// # 效果
    ///
    /// - `status` → `Status::Stopped`
    /// - `running` → `false`
    ///
    /// # 示例
    ///
    /// ```rust
    /// let mut trader = Trader::new("BTCUSDT");
    /// trader.start();
    /// trader.stop();
    /// assert!(!trader.is_running());
    /// ```
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.status = Status::Stopped;
    }

    /// 检查是否运行中
    ///
    /// # 返回
    ///
    /// - `true`: 交易循环应该继续执行
    /// - `false`: 交易循环应该退出
    ///
    /// # 示例
    ///
    /// ```rust
    /// let mut trader = Trader::new("BTCUSDT");
    /// assert!(!trader.is_running()); // 初始为 false
    /// trader.start();
    /// assert!(trader.is_running()); // 启动后为 true
    /// ```
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 执行交易逻辑
    ///
    /// 这是业务逻辑的注入点。
    /// 由外部实现具体的交易策略。
    ///
    /// # 设计说明
    ///
    /// 此方法目前为空实现（TODO）。
    /// 实际使用时，由外部调用者在此处实现：
    /// - 获取市场数据
    /// - 计算指标
    /// - 生成信号
    /// - 发送订单
    ///
    /// # 使用模式
    ///
    /// ```rust,ignore
    /// // 外部实现
    /// pub async fn execute(&mut self) {
    ///     // 1. 获取数据
    ///     let market = fetch_market_data(&self.config.symbol).await;
    ///     let position = fetch_position(&self.config.symbol).await;
    ///
    ///     // 2. 计算信号
    ///     if let Some(signal) = self.strategy.generate(&market, &position) {
    ///         // 3. 发送订单
    ///         self.order_sender.send(signal).await;
    ///     }
    /// }
    ///
    /// // 使用
    /// let trader = Arc::new(Mutex::new(Trader::new("BTCUSDT")));
    /// trader.lock().unwrap().start();
    ///
    /// loop {
    ///     if !trader.lock().unwrap().is_running() {
    ///         break;
    ///     }
    ///     trader.lock().unwrap().execute().await;
    ///     sleep(500).await;
    /// }
    /// ```
    pub async fn execute(&mut self) {
        // TODO: 业务逻辑注入点
        // 外部实现具体的市场数据获取、信号生成、订单发送逻辑
    }

    /// 获取健康状态
    ///
    /// 用于监控和调试。
    ///
    /// # 返回
    ///
    /// 包含当前交易器状态的快照：
    /// - 品种代码
    /// - 状态名称
    /// - 运行标志
    ///
    /// # 示例
    ///
    /// ```rust
    /// let trader = Trader::new("BTCUSDT");
    /// let health = trader.health();
    /// println!("{}: {} (running={})",
    ///     health.symbol,
    ///     health.status,
    ///     health.running
    /// );
    /// ```
    pub fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.symbol.clone(),
            status: format!("{:?}", self.status),
            running: self.is_running(),
        }
    }
}

// ============================================================================
// 健康状态
// ============================================================================

/// 交易器健康状态
///
/// 用于监控接口，返回 Trader 的当前状态快照。
///
/// # 字段
///
/// - `symbol`: 品种代码
/// - `status`: 状态字符串（用于日志/监控）
/// - `running`: 是否运行中
#[derive(Debug, Clone)]
pub struct TraderHealth {
    /// 品种代码
    pub symbol: String,

    /// 状态描述
    ///
    /// 格式为 `{:?}` 的状态名称字符串：
    /// - "Initial"
    /// - "Running"
    /// - "Stopped"
    pub status: String,

    /// 运行标志
    ///
    /// - `true`: 正在执行
    /// - `false`: 已停止
    pub running: bool,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_status() {
        let trader = Trader::new("BTCUSDT");
        assert_eq!(trader.status, Status::Initial);
        assert!(!trader.is_running());
    }

    #[test]
    fn test_start_stop() {
        let mut trader = Trader::new("BTCUSDT");
        
        // 初始状态
        assert_eq!(trader.status, Status::Initial);
        assert!(!trader.is_running());
        
        // 启动
        trader.start();
        assert_eq!(trader.status, Status::Running);
        assert!(trader.is_running());
        
        // 停止
        trader.stop();
        assert_eq!(trader.status, Status::Stopped);
        assert!(!trader.is_running());
    }

    #[test]
    fn test_restart() {
        let mut trader = Trader::new("BTCUSDT");
        
        trader.start();
        trader.stop();
        
        // 可以重新启动
        trader.start();
        assert!(trader.is_running());
    }

    #[test]
    fn test_config() {
        let trader = Trader::new("ETHUSDT");
        assert_eq!(trader.config.symbol, "ETHUSDT");
        assert_eq!(trader.config.interval_ms, 500);
    }

    #[test]
    fn test_health() {
        let trader = Trader::new("BTCUSDT");
        let health = trader.health();
        
        assert_eq!(health.symbol, "BTCUSDT");
        assert_eq!(health.status, "Initial");
        assert!(!health.running);
    }
}
