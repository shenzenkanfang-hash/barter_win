//! f_engine 引擎状态管理（生产级）
//!
//! # 核心原则
//! 1. **所有字段私有化** - 外部模块无法直接访问数据
//! 2. **接口隔离** - 所有操作必须通过方法调用
//! 3. **线程安全** - 使用 Arc<RwLock> 保护所有共享状态
//! 4. **原子操作** - 高频指标使用 Atomic 类型
//!
//! # 模块间调用规则
//! ⚠️ **严格禁止**：
//! - 直接读写 `EngineState.xxx` 字段
//! - 跨模块访问 `SymbolState.xxx` 字段
//! - 绕过接口方法直接操作内存
//!
//! ✅ **正确方式**：
//! ```rust,ignore
//! // 通过句柄访问
//! let state = EngineStateHandle::new(EngineMode::Production);
//!
//! // 原子操作（无锁）
//! state.read().record_tick();
//!
//! // 状态查询（读锁）
//! if state.read().can_trade() {
//!     // trading logic
//! }
//!
//! // 状态修改（写锁）
//! {
//!     let mut s = state.write();
//!     s.start();
//!     s.register_symbol("BTC-USDT");
//! }
//! ```

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::state::SymbolState;

// ============================================================================
// 错误类型
// ============================================================================

/// 引擎状态错误
#[derive(Debug, Clone, Error)]
pub enum EngineStateError {
    #[error("引擎已停止: {0}")]
    EngineStopped(String),

    #[error("品种未注册: {0}")]
    SymbolNotFound(String),

    #[error("品种已存在: {0}")]
    SymbolExists(String),

    #[error("引擎已暂停: {0}")]
    EnginePaused(String),

    #[error("状态不一致: {0}")]
    StateInconsistent(String),

    #[error("自检失败: {0}")]
    SelfCheckFailed(String),
}

pub type Result<T> = std::result::Result<T, EngineStateError>;

// ============================================================================
// 引擎状态枚举
// ============================================================================

/// 引擎运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    /// 初始化中
    Initializing,
    /// 运行中
    Running,
    /// 已暂停
    Paused,
    /// 优雅关闭中
    ShuttingDown,
    /// 已停止
    Stopped,
    /// 错误状态
    Error,
}

impl Default for EngineStatus {
    fn default() -> Self {
        EngineStatus::Initializing
    }
}

impl EngineStatus {
    /// 是否处于活跃状态
    pub fn is_active(&self) -> bool {
        matches!(self, EngineStatus::Running | EngineStatus::Paused)
    }

    /// 是否可以接收新请求
    pub fn accepts_requests(&self) -> bool {
        matches!(self, EngineStatus::Running)
    }
}

// ============================================================================
// 运行模式
// ============================================================================

/// 引擎运行模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMode {
    /// 回测模式
    Backtest,
    /// 模拟交易
    Simulation,
    /// 实盘交易
    Production,
}

impl Default for EngineMode {
    fn default() -> Self {
        EngineMode::Simulation
    }
}

/// 运行环境
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    /// 开发环境
    Development,
    /// 测试环境
    Test,
    /// 预发布环境
    Staging,
    /// 生产环境
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Environment::Development
    }
}

// ============================================================================
// 健康状态
// ============================================================================

/// 健康检查状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// 健康
    Healthy,
    /// 降级（部分功能异常）
    Degraded,
    /// 不健康（严重问题）
    Unhealthy,
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Healthy
    }
}

// ============================================================================
// 熔断器
// ============================================================================

/// 熔断配置
///
/// 所有字段私有化，通过方法访问
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 最大连续错误次数
    max_consecutive_errors: u32,
    /// 暂停时长（秒）
    pause_duration_secs: u64,
    /// 是否自动恢复
    auto_resume: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_consecutive_errors: 5,
            pause_duration_secs: 60,
            auto_resume: true,
        }
    }
}

impl CircuitBreakerConfig {
    /// 创建生产配置
    pub fn production() -> Self {
        Self {
            max_consecutive_errors: 5,
            pause_duration_secs: 60,
            auto_resume: true,
        }
    }

    /// 创建回测配置（更宽松）
    pub fn backtest() -> Self {
        Self {
            max_consecutive_errors: 100,
            pause_duration_secs: 0,
            auto_resume: false,
        }
    }

    /// 获取最大连续错误次数
    pub fn max_consecutive_errors(&self) -> u32 {
        self.max_consecutive_errors
    }

    /// 获取暂停时长（秒）
    pub fn pause_duration_secs(&self) -> u64 {
        self.pause_duration_secs
    }

    /// 是否自动恢复
    pub fn auto_resume(&self) -> bool {
        self.auto_resume
    }
}

/// 熔断动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerAction {
    /// 无动作
    None,
    /// 暂停
    Pause,
    /// 停止
    Stop,
}

/// 熔断器状态
///
/// 所有字段私有化，通过方法访问
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreaker {
    /// 配置
    config: CircuitBreakerConfig,
    /// 当前连续错误计数
    consecutive_errors: u32,
    /// 是否触发熔断
    is_triggered: bool,
    /// 触发时间
    triggered_at: Option<DateTime<Utc>>,
    /// 计划恢复时间
    scheduled_resume_at: Option<DateTime<Utc>>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            config: CircuitBreakerConfig::default(),
            consecutive_errors: 0,
            is_triggered: false,
            triggered_at: None,
            scheduled_resume_at: None,
        }
    }
}

impl CircuitBreaker {
    /// 使用配置创建
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            consecutive_errors: 0,
            is_triggered: false,
            triggered_at: None,
            scheduled_resume_at: None,
        }
    }

    // ─────────────────────────────────────────────────────────
    // 查询方法
    // ─────────────────────────────────────────────────────────

    /// 是否触发熔断
    pub fn is_triggered(&self) -> bool {
        self.is_triggered
    }

    /// 获取连续错误计数
    pub fn consecutive_errors(&self) -> u32 {
        self.consecutive_errors
    }

    /// 获取配置
    pub fn config(&self) -> &CircuitBreakerConfig {
        &self.config
    }

    /// 获取触发时间
    pub fn triggered_at(&self) -> Option<DateTime<Utc>> {
        self.triggered_at
    }

    /// 获取计划恢复时间
    pub fn scheduled_resume_at(&self) -> Option<DateTime<Utc>> {
        self.scheduled_resume_at
    }

    // ─────────────────────────────────────────────────────────
    // 修改方法
    // ─────────────────────────────────────────────────────────

    /// 记录错误
    pub fn record_error(&mut self) {
        self.consecutive_errors += 1;
    }

    /// 重置
    pub fn reset(&mut self) {
        self.consecutive_errors = 0;
        self.is_triggered = false;
        self.triggered_at = None;
        self.scheduled_resume_at = None;
    }

    /// 是否应触发熔断
    pub fn should_trigger(&self) -> bool {
        !self.is_triggered && self.consecutive_errors >= self.config.max_consecutive_errors
    }

    /// 触发熔断
    pub fn trigger(&mut self) {
        if self.should_trigger() {
            self.is_triggered = true;
            self.triggered_at = Some(Utc::now());
            if self.config.auto_resume {
                self.scheduled_resume_at =
                    Some(Utc::now() + ChronoDuration::seconds(self.config.pause_duration_secs as i64));
            }
        }
    }

    /// 是否应自动恢复
    pub fn should_auto_resume(&self) -> bool {
        self.config.auto_resume && self.is_triggered && !self.is_pause_duration_active()
    }

    /// 暂停时长是否还在生效
    fn is_pause_duration_active(&self) -> bool {
        if let Some(resume_at) = self.scheduled_resume_at {
            return Utc::now() < resume_at;
        }
        false
    }

    /// 检查并返回动作
    pub fn check(&mut self) -> CircuitBreakerAction {
        if self.should_trigger() {
            self.trigger();
            return CircuitBreakerAction::Pause;
        }

        if self.is_triggered && !self.is_pause_duration_active() {
            if self.config.auto_resume {
                self.reset();
                return CircuitBreakerAction::None;
            }
            return CircuitBreakerAction::Stop;
        }

        CircuitBreakerAction::None
    }
}

// ============================================================================
// 引擎状态
// ============================================================================

/// 引擎全局状态
///
/// # 设计原则
/// - 所有字段**私有化**
/// - 所有访问通过**方法**
/// - 高频指标使用**原子类型**
/// - 品种管理使用 **FnvHashMap**
///
/// # 模块间调用规则
/// ⚠️ **禁止**直接访问字段，必须使用方法
/// ```rust,ignore
/// // ❌ 错误
/// state.status = EngineStatus::Running;
///
/// // ✅ 正确
/// state.start();
/// ```
pub struct EngineState {
    // ─────────────────────────────────────────────────────────
    // 标识
    // ─────────────────────────────────────────────────────────
    /// 引擎唯一ID
    engine_id: String,
    /// 运行模式
    mode: EngineMode,
    /// 运行环境
    environment: Environment,

    // ─────────────────────────────────────────────────────────
    // 生命周期
    // ─────────────────────────────────────────────────────────
    start_time: DateTime<Utc>,
    last_active_time: DateTime<Utc>,
    restart_count: u32,
    status: EngineStatus,
    health: HealthStatus,
    error_message: Option<String>,

    // ─────────────────────────────────────────────────────────
    // 风控熔断
    // ─────────────────────────────────────────────────────────
    circuit_breaker: CircuitBreaker,
    is_shutting_down: bool,
    shutdown_start_time: Option<DateTime<Utc>>,

    // ─────────────────────────────────────────────────────────
    // 原子指标（无锁高性能）
    // ─────────────────────────────────────────────────────────
    tick_processed: AtomicU64,
    order_sent: AtomicU64,
    order_filled: AtomicU64,
    order_failed: AtomicU64,
    signal_generated: AtomicU64,
    error_count: AtomicU32,

    // ─────────────────────────────────────────────────────────
    // 配置热更新
    // ─────────────────────────────────────────────────────────
    config_version: u64,
    config_updated_at: Option<DateTime<Utc>>,

    // ─────────────────────────────────────────────────────────
    // 品种管理
    // ─────────────────────────────────────────────────────────
    symbols: FnvHashMap<String, SymbolState>,
}

// ============================================================================
// EngineState 实现
// ============================================================================

impl EngineState {
    // ═══════════════════════════════════════════════════════════════
    // 构造函数
    // ═══════════════════════════════════════════════════════════════

    /// 创建新引擎状态
    pub fn new(mode: EngineMode) -> Self {
        let now = Utc::now();
        let engine_id = format!(
            "engine_{}_{}",
            now.timestamp(),
            std::process::id()
        );
        Self {
            engine_id,
            mode,
            environment: Environment::default(),
            start_time: now,
            last_active_time: now,
            restart_count: 0,
            status: EngineStatus::Initializing,
            health: HealthStatus::Healthy,
            error_message: None,
            circuit_breaker: CircuitBreaker::default(),
            is_shutting_down: false,
            shutdown_start_time: None,
            tick_processed: AtomicU64::new(0),
            order_sent: AtomicU64::new(0),
            order_filled: AtomicU64::new(0),
            order_failed: AtomicU64::new(0),
            signal_generated: AtomicU64::new(0),
            error_count: AtomicU32::new(0),
            config_version: 1,
            config_updated_at: None,
            symbols: FnvHashMap::default(),
        }
    }

    /// 使用自定义熔断配置创建
    pub fn with_circuit_breaker(mode: EngineMode, config: CircuitBreakerConfig) -> Self {
        let mut state = Self::new(mode);
        state.circuit_breaker = CircuitBreaker::new(config);
        state
    }

    /// 使用完整配置创建
    pub fn with_config(mode: EngineMode, env: Environment, config: CircuitBreakerConfig) -> Self {
        let mut state = Self::new(mode);
        state.environment = env;
        state.circuit_breaker = CircuitBreaker::new(config);
        state
    }

    // ═══════════════════════════════════════════════════════════════
    // 标识查询
    // ═══════════════════════════════════════════════════════════════

    /// 获取引擎唯一ID
    pub fn engine_id(&self) -> &str {
        &self.engine_id
    }

    /// 获取运行模式
    pub fn mode(&self) -> EngineMode {
        self.mode
    }

    /// 获取运行环境
    pub fn environment(&self) -> Environment {
        self.environment
    }

    // ═══════════════════════════════════════════════════════════════
    // 生命周期管理
    // ═══════════════════════════════════════════════════════════════

    /// 启动引擎
    pub fn start(&mut self) {
        self.status = EngineStatus::Running;
        self.last_active_time = Utc::now();
    }

    /// 开始优雅关闭
    ///
    /// 调用后：
    /// - `can_trade()` 返回 `false`
    /// - 停止接收新订单
    /// - 等待未完成订单
    pub fn start_shutdown(&mut self) {
        self.is_shutting_down = true;
        self.shutdown_start_time = Some(Utc::now());
        self.status = EngineStatus::ShuttingDown;
    }

    /// 完成关闭
    pub fn complete_shutdown(&mut self) {
        self.status = EngineStatus::Stopped;
        self.is_shutting_down = false;
    }

    /// 暂停引擎
    pub fn pause(&mut self) {
        self.status = EngineStatus::Paused;
        self.last_active_time = Utc::now();
    }

    /// 恢复引擎
    pub fn resume(&mut self) {
        if self.status == EngineStatus::Paused {
            self.status = EngineStatus::Running;
            self.last_active_time = Utc::now();
        }
    }

    /// 停止引擎
    pub fn stop(&mut self) {
        self.status = EngineStatus::Stopped;
        self.is_shutting_down = false;
    }

    /// 设置错误状态
    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg.clone());
        self.status = EngineStatus::Error;
        self.health = HealthStatus::Unhealthy;
    }

    /// 清除错误状态
    pub fn clear_error(&mut self) {
        self.error_message = None;
        if self.status == EngineStatus::Error {
            self.status = EngineStatus::Running;
        }
        self.health = HealthStatus::Healthy;
    }

    /// 增加重启次数
    pub fn increment_restart(&mut self) {
        self.restart_count += 1;
    }

    // ═══════════════════════════════════════════════════════════════
    // 状态查询
    // ═══════════════════════════════════════════════════════════════

    /// 检查是否可以交易
    ///
    /// 返回 `true` 当且仅当：
    /// - 状态为 Running
    /// - 未在关闭中
    /// - 熔断器未触发
    pub fn can_trade(&self) -> bool {
        self.status == EngineStatus::Running
            && !self.is_shutting_down
            && !self.circuit_breaker.is_triggered
    }

    /// 获取当前状态
    pub fn status(&self) -> EngineStatus {
        self.status
    }

    /// 获取健康状态
    pub fn health(&self) -> HealthStatus {
        self.health
    }

    /// 是否正在关闭
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down
    }

    /// 是否暂停
    pub fn is_paused(&self) -> bool {
        self.status == EngineStatus::Paused
    }

    /// 是否已停止
    pub fn is_stopped(&self) -> bool {
        self.status == EngineStatus::Stopped
    }

    /// 是否处于错误状态
    pub fn is_error(&self) -> bool {
        self.status == EngineStatus::Error
    }

    /// 获取运行时间
    pub fn uptime(&self) -> Duration {
        Utc::now()
            .signed_duration_since(self.start_time)
            .to_std()
            .unwrap_or_default()
    }

    /// 获取启动时间
    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }

    /// 获取最后活跃时间
    pub fn last_active_time(&self) -> DateTime<Utc> {
        self.last_active_time
    }

    /// 获取错误消息
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// 获取重启次数
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// 获取关闭开始时间
    pub fn shutdown_start_time(&self) -> Option<DateTime<Utc>> {
        self.shutdown_start_time
    }

    // ═══════════════════════════════════════════════════════════════
    // 指标查询（原子操作）
    // ═══════════════════════════════════════════════════════════════

    /// 累计处理 tick 数
    pub fn tick_processed(&self) -> u64 {
        self.tick_processed.load(Ordering::Relaxed)
    }

    /// 累计发送订单数
    pub fn order_sent(&self) -> u64 {
        self.order_sent.load(Ordering::Relaxed)
    }

    /// 累计成交订单数
    pub fn order_filled(&self) -> u64 {
        self.order_filled.load(Ordering::Relaxed)
    }

    /// 累计失败订单数
    pub fn order_failed(&self) -> u64 {
        self.order_failed.load(Ordering::Relaxed)
    }

    /// 累计生成信号数
    pub fn signal_generated(&self) -> u64 {
        self.signal_generated.load(Ordering::Relaxed)
    }

    /// 累计错误数
    pub fn error_count(&self) -> u32 {
        self.error_count.load(Ordering::Relaxed)
    }

    /// 连续错误计数
    pub fn consecutive_errors(&self) -> u32 {
        self.circuit_breaker.consecutive_errors
    }

    /// 订单成交率
    pub fn fill_rate(&self) -> f64 {
        let sent = self.order_sent();
        if sent == 0 {
            return 0.0;
        }
        self.order_filled() as f64 / sent as f64
    }

    /// 订单失败率
    pub fn fail_rate(&self) -> f64 {
        let sent = self.order_sent();
        if sent == 0 {
            return 0.0;
        }
        self.order_failed() as f64 / sent as f64
    }

    /// 获取所有指标快照
    pub fn metrics_snapshot(&self) -> EngineMetricsSnapshot {
        EngineMetricsSnapshot {
            engine_id: self.engine_id.clone(),
            tick_processed: self.tick_processed(),
            order_sent: self.order_sent(),
            order_filled: self.order_filled(),
            order_failed: self.order_failed(),
            signal_generated: self.signal_generated(),
            error_count: self.error_count(),
            consecutive_errors: self.consecutive_errors(),
            fill_rate: self.fill_rate(),
            fail_rate: self.fail_rate(),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 指标更新（原子操作，无锁）
    // ═══════════════════════════════════════════════════════════════

    /// 记录 tick 处理
    ///
    /// ⚠️ 此方法使用原子操作，无需锁
    pub fn record_tick(&self) {
        self.tick_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录订单发送
    pub fn record_order_sent(&self) {
        self.order_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录订单成交
    pub fn record_order_filled(&self) {
        self.order_filled.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录订单失败
    pub fn record_order_failed(&self) {
        self.order_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录信号生成
    pub fn record_signal(&self) {
        self.signal_generated.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录错误（需要写锁）
    pub fn record_error(&mut self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
        self.circuit_breaker.record_error();
    }

    /// 重置连续错误计数
    pub fn reset_consecutive_errors(&mut self) {
        self.circuit_breaker.reset();
    }

    /// 批量更新指标（用于恢复）
    pub fn update_metrics(&mut self, snapshot: EngineMetricsSnapshot) {
        self.tick_processed
            .store(snapshot.tick_processed, Ordering::Relaxed);
        self.order_sent.store(snapshot.order_sent, Ordering::Relaxed);
        self.order_filled
            .store(snapshot.order_filled, Ordering::Relaxed);
        self.order_failed
            .store(snapshot.order_failed, Ordering::Relaxed);
        self.signal_generated
            .store(snapshot.signal_generated, Ordering::Relaxed);
        self.error_count
            .store(snapshot.error_count, Ordering::Relaxed);
        self.circuit_breaker.consecutive_errors = snapshot.consecutive_errors;
    }

    // ═══════════════════════════════════════════════════════════════
    // 健康检查
    // ═══════════════════════════════════════════════════════════════

    /// 更新健康状态
    pub fn update_health(&mut self) {
        let fail_rate = self.fail_rate();
        let consecutive = self.consecutive_errors();

        self.health = if consecutive >= 10 || fail_rate > 0.5 {
            HealthStatus::Unhealthy
        } else if consecutive >= 3 || fail_rate > 0.2 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
    }

    /// 自检
    ///
    /// 检查：
    /// - 品种是否重复
    /// - 状态一致性
    /// - 计数器异常
    pub fn self_check(&self) -> Result<()> {
        // 检查品种是否重复
        let mut symbols = std::collections::HashSet::new();
        for symbol in self.symbols.keys() {
            if !symbols.insert(symbol) {
                return Err(EngineStateError::SelfCheckFailed(format!(
                    "重复品种: {}",
                    symbol
                )));
            }
        }

        // 检查状态一致性
        if self.is_shutting_down && self.status != EngineStatus::ShuttingDown {
            return Err(EngineStateError::StateInconsistent(
                "is_shutting_down 与 status 不一致".to_string(),
            ));
        }

        // 检查计数器
        if self.tick_processed.load(Ordering::Relaxed) > u64::MAX / 2 {
            return Err(EngineStateError::SelfCheckFailed(
                "tick_processed 异常".to_string(),
            ));
        }

        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════
    // 熔断检查
    // ═══════════════════════════════════════════════════════════════

    /// 检查熔断器并返回动作
    pub fn check_circuit_breaker(&mut self) -> CircuitBreakerAction {
        self.circuit_breaker.check()
    }

    /// 触发熔断
    pub fn trigger_circuit_breaker(&mut self) {
        self.circuit_breaker.trigger();
        self.pause();
    }

    /// 重置熔断器
    pub fn reset_circuit_breaker(&mut self) {
        self.circuit_breaker.reset();
    }

    /// 获取熔断器状态（只读引用）
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    // ═══════════════════════════════════════════════════════════════
    // 品种管理
    // ═══════════════════════════════════════════════════════════════

    /// 注册品种
    ///
    /// # 示例
    /// ```rust,ignore
    /// {
    ///     let mut s = state.write();
    ///     s.register_symbol("BTC-USDT");
    /// }
    /// ```
    pub fn register_symbol(&mut self, symbol: &str) -> &mut SymbolState {
        if !self.symbols.contains_key(symbol) {
            self.symbols
                .insert(symbol.to_string(), SymbolState::new(symbol.to_string()));
        }
        self.symbols.get_mut(symbol).unwrap()
    }

    /// 批量注册品种
    pub fn register_symbols(&mut self, symbols: &[&str]) {
        for symbol in symbols {
            self.register_symbol(symbol);
        }
    }

    /// 注销品种
    pub fn unregister_symbol(&mut self, symbol: &str) -> bool {
        self.symbols.remove(symbol).is_some()
    }

    /// 获取品种状态
    pub fn get_symbol(&self, symbol: &str) -> Option<&SymbolState> {
        self.symbols.get(symbol)
    }

    /// 获取品种状态（可变）
    pub fn get_symbol_mut(&mut self, symbol: &str) -> Option<&mut SymbolState> {
        self.symbols.get_mut(symbol)
    }

    /// 检查品种是否注册
    pub fn has_symbol(&self, symbol: &str) -> bool {
        self.symbols.contains_key(symbol)
    }

    /// 获取所有注册的品种
    pub fn registered_symbols(&self) -> Vec<String> {
        self.symbols.keys().cloned().collect()
    }

    /// 获取品种数量
    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }

    // ═══════════════════════════════════════════════════════════════
    // 配置热更新
    // ═══════════════════════════════════════════════════════════════

    /// 更新熔断配置
    pub fn update_circuit_breaker_config(&mut self, config: CircuitBreakerConfig) {
        self.circuit_breaker.config = config;
        self.config_version += 1;
        self.config_updated_at = Some(Utc::now());
    }

    /// 获取配置版本
    pub fn config_version(&self) -> u64 {
        self.config_version
    }

    /// 获取配置更新时间
    pub fn config_updated_at(&self) -> Option<DateTime<Utc>> {
        self.config_updated_at
    }
}

// ============================================================================
// 指标快照
// ============================================================================

/// 引擎指标快照
///
/// 用于持久化和监控
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineMetricsSnapshot {
    /// 引擎ID
    pub engine_id: String,
    /// tick 处理数
    pub tick_processed: u64,
    /// 订单发送数
    pub order_sent: u64,
    /// 订单成交数
    pub order_filled: u64,
    /// 订单失败数
    pub order_failed: u64,
    /// 信号生成数
    pub signal_generated: u64,
    /// 错误计数
    pub error_count: u32,
    /// 连续错误计数
    pub consecutive_errors: u32,
    /// 成交率
    pub fill_rate: f64,
    /// 失败率
    pub fail_rate: f64,
}

impl Default for EngineMetricsSnapshot {
    fn default() -> Self {
        Self {
            engine_id: String::new(),
            tick_processed: 0,
            order_sent: 0,
            order_filled: 0,
            order_failed: 0,
            signal_generated: 0,
            error_count: 0,
            consecutive_errors: 0,
            fill_rate: 0.0,
            fail_rate: 0.0,
        }
    }
}

// ============================================================================
// 线程安全句柄
// ============================================================================

/// 线程安全的引擎状态句柄
///
/// # 设计
/// - `Arc`: 多所有权，跨线程共享
/// - `RwLock`: 读写锁，读并发，写独占
///
/// # 使用示例
/// ```rust,ignore
/// let state = EngineStateHandle::new(EngineMode::Production);
///
/// // 原子操作（无锁）
/// state.read().record_tick();
///
/// // 状态查询（读锁）
/// if state.read().can_trade() { ... }
///
/// // 状态修改（写锁）
/// {
///     let mut s = state.write();
///     s.start();
/// }
/// ```
pub struct EngineStateHandle {
    inner: Arc<RwLock<EngineState>>,
}

impl EngineStateHandle {
    /// 创建新句柄
    pub fn new(mode: EngineMode) -> Self {
        Self {
            inner: Arc::new(RwLock::new(EngineState::new(mode))),
        }
    }

    /// 使用自定义熔断配置创建
    pub fn with_circuit_breaker(mode: EngineMode, config: CircuitBreakerConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(EngineState::with_circuit_breaker(
                mode, config,
            ))),
        }
    }

    /// 使用完整配置创建
    pub fn with_config(mode: EngineMode, env: Environment, config: CircuitBreakerConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(EngineState::with_config(
                mode, env, config,
            ))),
        }
    }

    /// 获取读锁
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, EngineState> {
        self.inner.read()
    }

    /// 获取写锁
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, EngineState> {
        self.inner.write()
    }

    /// 克隆句柄（共享底层状态）
    pub fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Clone for EngineStateHandle {
    fn clone(&self) -> Self {
        self.clone_handle()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_lifecycle() {
        let mut state = EngineState::new(EngineMode::Production);

        assert_eq!(state.status(), EngineStatus::Initializing);
        assert!(!state.can_trade());

        state.start();
        assert_eq!(state.status(), EngineStatus::Running);
        assert!(state.can_trade());

        state.pause();
        assert!(!state.can_trade());

        state.resume();
        assert!(state.can_trade());

        state.stop();
        assert_eq!(state.status(), EngineStatus::Stopped);
        assert!(!state.can_trade());
    }

    #[test]
    fn test_atomic_metrics() {
        let state = EngineState::new(EngineMode::Simulation);

        state.record_tick();
        state.record_tick();
        state.record_signal();
        state.record_order_sent();
        state.record_order_filled();

        assert_eq!(state.tick_processed(), 2);
        assert_eq!(state.signal_generated(), 1);
        assert_eq!(state.fill_rate(), 1.0);
    }

    #[test]
    fn test_circuit_breaker() {
        let config = CircuitBreakerConfig {
            max_consecutive_errors: 3,
            pause_duration_secs: 60,
            auto_resume: true,
        };
        let mut state = EngineState::with_circuit_breaker(EngineMode::Production, config);

        assert!(!state.circuit_breaker().is_triggered());

        for _ in 0..3 {
            state.record_error();
        }

        let action = state.check_circuit_breaker();
        assert_eq!(action, CircuitBreakerAction::Pause);
        assert!(state.circuit_breaker().is_triggered());
        assert!(!state.can_trade());
    }

    #[test]
    fn test_graceful_shutdown() {
        let mut state = EngineState::new(EngineMode::Production);
        state.start();

        assert!(state.can_trade());

        state.start_shutdown();
        assert!(state.is_shutting_down());
        assert!(!state.can_trade());

        state.complete_shutdown();
        assert_eq!(state.status(), EngineStatus::Stopped);
    }

    #[test]
    fn test_symbol_registration() {
        let mut state = EngineState::new(EngineMode::Simulation);

        state.register_symbol("BTC-USDT");
        state.register_symbol("ETH-USDT");

        assert_eq!(state.symbol_count(), 2);
        assert!(state.has_symbol("BTC-USDT"));
        assert!(state.has_symbol("ETH-USDT"));

        state.unregister_symbol("BTC-USDT");
        assert_eq!(state.symbol_count(), 1);
        assert!(!state.has_symbol("BTC-USDT"));
    }

    #[test]
    fn test_health_update() {
        let mut state = EngineState::new(EngineMode::Production);

        assert_eq!(state.health(), HealthStatus::Healthy);

        for _ in 0..5 {
            state.record_error();
        }
        state.update_health();
        assert_eq!(state.health(), HealthStatus::Degraded);

        for _ in 0..5 {
            state.record_error();
        }
        state.update_health();
        assert_eq!(state.health(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_engine_id() {
        let state = EngineState::new(EngineMode::Production);
        assert!(state.engine_id().starts_with("engine_"));
    }

    #[test]
    fn test_metrics_snapshot() {
        let state = EngineState::new(EngineMode::Backtest);

        state.record_tick();
        state.record_order_sent();
        state.record_order_filled();

        let snapshot = state.metrics_snapshot();
        assert!(!snapshot.engine_id.is_empty());
        assert_eq!(snapshot.tick_processed, 1);
        assert_eq!(snapshot.order_sent, 1);
    }

    #[test]
    fn test_self_check() {
        let state = EngineState::new(EngineMode::Production);
        assert!(state.self_check().is_ok());
    }
}
