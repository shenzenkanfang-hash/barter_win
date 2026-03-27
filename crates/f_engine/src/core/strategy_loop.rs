//! strategy_loop.rs - 策略自循环协程
//!
//! Engine 协程管理 - spawn / stop / 心跳监控 / 指数退避重启

#![forbid(unsafe_code)]

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as TokioRwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// 最大重启次数
pub const MAX_RESTART_COUNT: u32 = 10;
/// 心跳超时时间（毫秒）
pub const HEARTBEAT_TIMEOUT_MS: u64 = 30_000;

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// 策略运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningState {
    Running,
    Stopped,
}

impl Default for RunningState {
    fn default() -> Self {
        RunningState::Stopped
    }
}

/// 策略自循环配置
#[derive(Debug, Clone)]
pub struct StrategyLoopConfig {
    /// 循环间隔 (ms)
    pub interval_ms: u64,
    /// 最大连续错误数
    pub max_consecutive_errors: u32,
}

impl Default for StrategyLoopConfig {
    fn default() -> Self {
        Self {
            interval_ms: 500,
            max_consecutive_errors: 3,
        }
    }
}

/// 策略自循环
pub struct StrategyLoop {
    pub symbol: String,
    pub config: StrategyLoopConfig,
    running_state: Arc<RwLock<RunningState>>,
    consecutive_errors: u32,
}

impl StrategyLoop {
    pub fn new(symbol: String, config: StrategyLoopConfig) -> Self {
        Self {
            symbol,
            config,
            running_state: Arc::new(RwLock::new(RunningState::Stopped)),
            consecutive_errors: 0,
        }
    }

    pub fn start(&self) {
        *self.running_state.write() = RunningState::Running;
    }

    pub fn stop(&self) {
        *self.running_state.write() = RunningState::Stopped;
    }

    pub fn is_running(&self) -> bool {
        *self.running_state.read() == RunningState::Running
    }

    pub fn state(&self) -> RunningState {
        *self.running_state.read()
    }

    pub async fn run(mut self) {
        self.start();
        info!("[{}] Strategy loop started", self.symbol);

        loop {
            if !self.is_running() {
                info!("[{}] Strategy loop exiting (stopped)", self.symbol);
                break;
            }

            if let Err(e) = self.execute_once().await {
                self.consecutive_errors += 1;
                error!("[{}] Execute error: {}", self.symbol, e);

                if self.consecutive_errors >= self.config.max_consecutive_errors {
                    error!("[{}] Max errors reached, stopping", self.symbol);
                    self.stop();
                    break;
                }
            } else {
                self.consecutive_errors = 0;
            }

            tokio::time::sleep(Duration::from_millis(self.config.interval_ms)).await;
        }

        info!("[{}] Strategy loop stopped", self.symbol);
    }

    async fn execute_once(&self) -> Result<(), StrategyLoopError> {
        // StrategyLoop 作为独立监控协程时，定期检查心跳状态
        // 注意：Trader 自循环已在 TraderManager 中管理，此处仅作监控桩
        tracing::debug!(symbol = %self.symbol, "StrategyLoop tick");
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StrategyLoopError {
    #[error("执行错误: {0}")]
    Execute(String),
}

// =============================================================================
// Engine 协程管理器（新增）
// =============================================================================

/// TraderHandle - Trader 协程句柄
pub struct TraderHandle {
    pub join_handle: TokioRwLock<Option<JoinHandle<()>>>,
    pub last_heartbeat_ms: AtomicU64,
    pub restart_count: AtomicU32,
    pub symbol: String,
}

impl TraderHandle {
    pub fn new(symbol: String) -> Self {
        Self {
            join_handle: TokioRwLock::new(None),
            last_heartbeat_ms: AtomicU64::new(current_time_ms()),
            restart_count: AtomicU32::new(0),
            symbol,
        }
    }

    /// 更新心跳
    pub fn heartbeat(&self) {
        self.last_heartbeat_ms.store(current_time_ms(), Ordering::Relaxed);
    }

    /// 检查是否超时
    pub fn is_stale(&self) -> bool {
        current_time_ms() - self.last_heartbeat_ms.load(Ordering::Relaxed) > HEARTBEAT_TIMEOUT_MS
    }

    /// 设置 JoinHandle
    pub async fn set_join_handle(&self, handle: JoinHandle<()>) {
        let mut guard = self.join_handle.write().await;
        *guard = Some(handle);
    }

    /// 检查是否已完成
    pub async fn is_finished(&self) -> bool {
        let guard = self.join_handle.read().await;
        guard.as_ref().map(|h| h.is_finished()).unwrap_or(true)
    }

    /// 重置重启计数
    pub fn reset_restart_count(&self) {
        self.restart_count.store(0, Ordering::Relaxed);
    }

    /// 获取当前重启计数
    pub fn load_restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::Relaxed)
    }
}

/// Engine - 协程管理器
pub struct Engine {
    instances: TokioRwLock<HashMap<String, Arc<TraderHandle>>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            instances: TokioRwLock::new(HashMap::new()),
        }
    }

    /// 启动 Trader 协程
    pub async fn spawn(&self, symbol: &str) {
        self.spawn_with_count(symbol, 0).await;
    }

    /// 带计数启动（用于重启时继承 restart_count）
    pub async fn spawn_with_count(&self, symbol: &str, restart_count: u32) {
        let handle = Arc::new(TraderHandle::new(symbol.to_string()));
        handle.restart_count.store(restart_count, Ordering::Relaxed);

        let mut instances = self.instances.write().await;
        instances.insert(symbol.to_string(), handle);

        info!(symbol = symbol, restart_count = restart_count, "Trader 协程已启动");
    }

    /// 停止 Trader
    pub async fn stop(&self, symbol: &str) {
        let handle = {
            let mut instances = self.instances.write().await;
            instances.remove(symbol)
        };

        if let Some(handle) = handle {
            let join_handle = {
                let mut guard = handle.join_handle.write().await;
                guard.take()
            };
            if let Some(h) = join_handle {
                let _ = h.await;
            }
            info!(symbol = symbol, "Trader 协程已停止");
        }
    }

    /// 心跳监控协程
    pub async fn monitor_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let mut to_restart = Vec::new();

            let instances = self.instances.read().await;
            for (symbol, handle) in instances.iter() {
                if handle.is_finished().await {
                    error!(symbol = %symbol, "协程已退出但未清理");
                    to_restart.push(symbol.clone());
                } else if handle.is_stale() {
                    warn!(symbol = %symbol, "心跳超时");
                    to_restart.push(symbol.clone());
                }
            }
            drop(instances);

            for symbol in to_restart {
                self.restart_with_backoff(&symbol).await;
            }
        }
    }

    /// 指数退避重启
    async fn restart_with_backoff(&self, symbol: &str) {
        let old_count = {
            let instances = self.instances.read().await;
            instances.get(symbol).map(|h| h.load_restart_count()).unwrap_or(0)
        };

        if old_count >= MAX_RESTART_COUNT {
            error!(
                symbol = %symbol,
                restart_count = old_count,
                "达到最大重启次数（{}），停止自动重启",
                MAX_RESTART_COUNT
            );
            return;
        }

        self.stop(symbol).await;

        let delay_secs = 2u64.saturating_pow(old_count.min(5));
        info!(symbol = %symbol, delay_secs = delay_secs, "等待重启");
        tokio::time::sleep(Duration::from_secs(delay_secs)).await;

        self.spawn_with_count(symbol, old_count + 1).await;
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
