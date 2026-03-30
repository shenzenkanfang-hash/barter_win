//! EngineManager - 协程生命周期管理器
//!
//! 管理多个协程的 spawn、restart、shutdown 功能。
//! 与 StateCenter 联动实现心跳超时自动重启（设计规格第八节）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::cmp::min;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;

use x_data::state::{ComponentState, ComponentStatus, StateCenter};

/// 协程条目（扩展版本，支持自动重启）
struct EngineEntry {
    /// 协程状态
    state: ComponentState,
    /// JoinHandle
    handle: JoinHandle<()>,
    /// 停止信号发送器
    stop_tx: mpsc::Sender<()>,
    /// 重启计数（用于指数退避）
    retry_count: AtomicU64,
    /// 是否活跃
    active: AtomicBool,
}

impl EngineEntry {
    fn new(state: ComponentState, handle: JoinHandle<()>, stop_tx: mpsc::Sender<()>) -> Self {
        Self {
            state,
            handle,
            stop_tx,
            retry_count: AtomicU64::new(0),
            active: AtomicBool::new(true),
        }
    }

    fn increment_retry(&self) -> u64 {
        self.retry_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn reset_retry(&self) {
        self.retry_count.store(0, Ordering::SeqCst);
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn mark_inactive(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

/// EngineManager 配置
#[derive(Debug, Clone)]
pub struct EngineManagerConfig {
    /// 心跳超时阈值（秒）— 传给 StateCenter
    pub stale_threshold_secs: i64,
    /// 重启检测间隔（秒）
    pub restart_check_interval_secs: u64,
    /// 关闭超时（秒）
    pub shutdown_timeout_secs: u64,
}

impl Default for EngineManagerConfig {
    fn default() -> Self {
        Self {
            stale_threshold_secs: 30,
            restart_check_interval_secs: 10,
            shutdown_timeout_secs: 5,
        }
    }
}

/// EngineManager - 协程生命周期管理器
///
/// 对齐设计规格第八节（8.1-8.3），核心功能：
/// - spawn: 启动新协程
/// - restart: 重启指定协程
/// - respawn: 自动重启（用于 handle_stale 流程）
/// - shutdown: 优雅关闭协程
/// - run_restart_loop: 后台监控循环（与 StateCenter 联动）
/// - handle_stale: 指数退避重启 stale 组件
pub struct EngineManager {
    /// 配置
    config: EngineManagerConfig,
    /// 协程表
    entries: Arc<RwLock<HashMap<String, EngineEntry>>>,
    /// StateCenter（与 Phase 1 对齐，使用 trait）
    state_center: Arc<dyn StateCenter>,
    /// shutdown 广播信号
    shutdown_tx: broadcast::Sender<()>,
}

impl EngineManager {
    /// 创建新的 EngineManager
    ///
    /// # Arguments
    /// * `config` - 配置
    /// * `state_center` - 状态中心（与 Phase 1 StateCenterTrait 对齐）
    pub fn new(config: EngineManagerConfig, state_center: Arc<dyn StateCenter>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            config,
            entries: Arc::new(RwLock::new(HashMap::new())),
            state_center,
            shutdown_tx,
        }
    }

    /// 启动新协程
    ///
    /// # Arguments
    /// * `component_id` - 组件唯一标识
    /// * `task` - 异步任务，签名：`FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()>`
    pub async fn spawn<F>(&self, component_id: String, task: F) -> Result<ComponentState, EngineError>
    where
        F: FnOnce(String, mpsc::Receiver<()>) -> tokio::task::JoinHandle<()> + Send + 'static,
    {
        // 检查是否已存在
        {
            let entries = self.entries.read().await;
            if entries.contains_key(&component_id) {
                return Err(EngineError::AlreadyExists(component_id));
            }
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>(1);

        // 创建协程状态并注册到 StateCenter
        let state = ComponentState::new_running(component_id.clone());
        self.state_center.register(component_id.clone());

        // 创建任务
        let task_component_id = component_id.clone();
        let handle = tokio::spawn(async move {
            let _ = task(task_component_id, stop_rx).await;
        });

        // 注册协程
        let entry = EngineEntry::new(state.clone(), handle, stop_tx);
        {
            let mut entries = self.entries.write().await;
            entries.insert(component_id.clone(), entry);
        }

        tracing::info!("[EngineManager] Spawned: {}", component_id);
        Ok(state)
    }

    /// 重启指定协程（手动重启）
    ///
    /// # Arguments
    /// * `component_id` - 组件唯一标识
    /// * `task` - 新的异步任务
    pub async fn restart<F>(&self, component_id: String, task: F) -> Result<ComponentState, EngineError>
    where
        F: FnOnce(String, mpsc::Receiver<()>) -> tokio::task::JoinHandle<()> + Send + 'static,
    {
        self.shutdown_one(&component_id).await?;
        self.spawn(component_id, task).await
    }

    /// 自动重启协程（用于 handle_stale 流程）
    ///
    /// 从 entries 移除旧 entry，重置 retry_count，然后使用工厂闭包重建协程。
    /// # Arguments
    /// * `component_id` - 组件唯一标识
    /// * `factory` - 工厂闭包，签名：`FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()>`
    pub async fn respawn<F>(&self, component_id: &str, factory: F) -> Result<(), EngineError>
    where
        F: FnOnce(String, mpsc::Receiver<()>) -> tokio::task::JoinHandle<()> + Send + 'static,
    {
        // 关闭旧的
        self.shutdown_one(component_id).await.ok();

        // 重置 retry_count（在新 spawn 成功后由 entry 自己重置）
        // 重建协程
        self.spawn(component_id.to_string(), factory).await?;
        Ok(())
    }

    /// 处理 stale 组件（指数退避重启策略）
    ///
    /// 对齐设计规格 8.3 节：
    /// - 计算退避延迟：min(60, 2^retry_count) 秒
    /// - retry_count++
    /// - 重新检查 stale（避免重复重启）
    /// - 调用 respawn() 重启
    ///
    /// # Arguments
    /// * `component_id` - stale 组件 ID
    /// * `factory` - 工厂闭包
    pub async fn handle_stale<F>(&self, component_id: &str, factory: F) -> Result<(), EngineError>
    where
        F: FnOnce(String, mpsc::Receiver<()>) -> tokio::task::JoinHandle<()> + Send + 'static,
    {
        let retry_count = {
            let entries = self.entries.read().await;
            match entries.get(component_id) {
                Some(e) => e.increment_retry(),
                None => {
                    tracing::warn!("[EngineManager] handle_stale: component {} not found", component_id);
                    return Err(EngineError::NotFound(component_id.to_string()));
                }
            }
        };

        // 指数退避：1s, 2s, 4s, 8s, 16s, 32s, 60s 上限
        let delay_secs = min(60, 2_i64.saturating_pow(retry_count as u32)) as u64;
        tracing::warn!(
            "[EngineManager] {} is stale, retry count={}, backing off {}s",
            component_id,
            retry_count,
            delay_secs
        );

        // 退避等待
        tokio::time::sleep(Duration::from_secs(delay_secs)).await;

        // 重新检查是否仍然 stale（避免重复重启）
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(component_id) {
                if !entry.is_active() {
                    tracing::info!("[EngineManager] {} already inactive, skipping respawn", component_id);
                    return Ok(());
                }
                // 再次查询 StateCenter
                if let Some(state) = self.state_center.get(component_id) {
                    if state.status != ComponentStatus::Stale {
                        tracing::info!("[EngineManager] {} recovered, skipping respawn", component_id);
                        return Ok(());
                    }
                }
            } else {
                return Err(EngineError::NotFound(component_id.to_string()));
            }
        }

        // 执行 respawn
        tracing::info!("[EngineManager] Respawning: {}", component_id);
        self.respawn(component_id, factory).await?;

        // 重置 retry_count
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(component_id) {
                entry.reset_retry();
            }
        }

        Ok(())
    }

    /// 后台监控循环（与 StateCenter 联动）
    ///
    /// 对齐设计规格 8.3 节 restart_loop()：
    /// - 每 restart_check_interval_secs 秒检测一次
    /// - 调用 state_center.get_stale(threshold_secs) 获取所有 stale 组件
    /// - 对每个 stale 组件进行指数退避重启
    ///
    /// # Arguments
    /// * `shutdown_rx` - shutdown 信号接收器（用于优雅停止）
    /// * `spawn_fn` - 任务工厂，传入 component_id，返回 (JoinHandle, mpsc::Sender) 元组
    pub async fn run_restart_loop(
        &self,
        mut shutdown_rx: broadcast::Receiver<()>,
        spawn_fn: Arc<dyn Fn(String) -> (JoinHandle<()>, mpsc::Sender<()>) + Send + Sync>,
    ) {
        tracing::info!(
            "[EngineManager] Restart loop started (interval={}s, stale_threshold={}s)",
            self.config.restart_check_interval_secs,
            self.config.stale_threshold_secs
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    tracing::info!("[EngineManager] Restart loop received shutdown signal");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(self.config.restart_check_interval_secs)) => {
                    let stale_components = self.state_center.get_stale(self.config.stale_threshold_secs);

                    for stale_state in stale_components {
                        let component_id = stale_state.component_id.clone();

                        let is_tracked = {
                            let entries = self.entries.read().await;
                            entries.contains_key(&component_id)
                        };

                        if !is_tracked {
                            continue;
                        }

                        tracing::warn!(
                            "[EngineManager] Detected stale component: {} (last_active={})",
                            component_id,
                            stale_state.last_active
                        );

                        let entries_clone = Arc::clone(&self.entries);
                        let state_center_clone = Arc::clone(&self.state_center);
                        let shutdown_timeout_secs = self.config.shutdown_timeout_secs;
                        let spawn_fn_clone = Arc::clone(&spawn_fn);

                        tokio::spawn(async move {
                            // 再次确认 stale（避免竞态）
                            if let Some(state) = state_center_clone.get(&component_id) {
                                if state.status != ComponentStatus::Stale {
                                    tracing::info!("[EngineManager] {} already recovered", component_id);
                                    return;
                                }
                            } else {
                                return;
                            }

                            // 获取 retry_count
                            let retry_count = {
                                let entries = entries_clone.read().await;
                                match entries.get(&component_id) {
                                    Some(e) => e.increment_retry(),
                                    None => return,
                                }
                            };

                            // 指数退避延迟
                            let delay_secs = min(60, 2_i64.saturating_pow(retry_count as u32)) as u64;
                            tracing::warn!(
                                "[EngineManager] {} stale, retry={}, backing off {}s",
                                component_id,
                                retry_count,
                                delay_secs
                            );

                            tokio::time::sleep(Duration::from_secs(delay_secs)).await;

                            // 再次检查（避免重复重启）
                            {
                                let entries = entries_clone.read().await;
                                if let Some(entry) = entries.get(&component_id) {
                                    if !entry.is_active() {
                                        tracing::info!("[EngineManager] {} inactive, skip", component_id);
                                        return;
                                    }
                                    if let Some(state) = state_center_clone.get(&component_id) {
                                        if state.status != ComponentStatus::Stale {
                                            tracing::info!("[EngineManager] {} recovered", component_id);
                                            return;
                                        }
                                    }
                                } else {
                                    return;
                                }
                            }

                            // shutdown 旧协程
                            {
                                let mut entries = entries_clone.write().await;
                                if let Some(mut entry) = entries.remove(&component_id) {
                                    entry.mark_inactive();
                                    let _ = entry.stop_tx.send(()).await;
                                    let _ = tokio::time::timeout(
                                        Duration::from_secs(shutdown_timeout_secs),
                                        &mut entry.handle,
                                    )
                                    .await;
                                }
                            }

                            // 重建
                            tracing::info!("[EngineManager] Respawning: {}", component_id);
                            let (handle, stop_tx) = spawn_fn_clone(component_id.clone());
                            let state = ComponentState::new_running(component_id.clone());

                            let entry = EngineEntry::new(state, handle, stop_tx);
                            {
                                let mut entries = entries_clone.write().await;
                                entries.insert(component_id.clone(), entry);
                            }

                            tracing::info!("[EngineManager] Respawned: {}", component_id);

                            // 重置 retry_count
                            {
                                let entries = entries_clone.read().await;
                                if let Some(entry) = entries.get(&component_id) {
                                    entry.reset_retry();
                                }
                            }
                        });
                    }
                }
            }
        }

        tracing::info!("[EngineManager] Restart loop stopped");
    }

    /// 关闭指定协程
    pub async fn shutdown_one(&self, component_id: &str) -> Result<(), EngineError> {
        let entry = {
            let mut entries = self.entries.write().await;
            entries.remove(component_id)
        };

        match entry {
            Some(mut e) => {
                e.mark_inactive();
                let _ = e.stop_tx.send(()).await;
                let _ = tokio::time::timeout(
                    Duration::from_secs(self.config.shutdown_timeout_secs),
                    &mut e.handle,
                )
                .await;
                tracing::info!("[EngineManager] Shutdown: {}", component_id);
                Ok(())
            }
            None => Err(EngineError::NotFound(component_id.to_string())),
        }
    }

    /// 关闭所有协程
    pub async fn shutdown_all(&self) {
        let entries: Vec<(String, EngineEntry)> = {
            let mut entries = self.entries.write().await;
            entries.drain().collect()
        };

        for (id, mut entry) in entries {
            entry.mark_inactive();
            let _ = entry.stop_tx.send(()).await;
            let _ = tokio::time::timeout(
                Duration::from_secs(self.config.shutdown_timeout_secs),
                &mut entry.handle,
            )
            .await;
            tracing::info!("[EngineManager] Shutdown: {}", id);
        }

        tracing::info!("[EngineManager] All engines stopped");
    }

    /// 获取协程状态
    pub async fn get_state(&self, component_id: &str) -> Option<ComponentState> {
        let entries = self.entries.read().await;
        entries.get(component_id).map(|e| e.state.clone())
    }

    /// 获取所有协程状态
    pub async fn get_all_states(&self) -> Vec<ComponentState> {
        let entries = self.entries.read().await;
        entries.values().map(|e| e.state.clone()).collect()
    }

    /// 获取活跃协程数量
    pub async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }

    /// 检查是否为空
    pub async fn is_empty(&self) -> bool {
        let entries = self.entries.read().await;
        entries.is_empty()
    }

    /// 获取 shutdown tx 的新 receiver（用于 run_restart_loop）
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
}

/// EngineManager 错误类型
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("组件 {0} 已存在")]
    AlreadyExists(String),

    #[error("组件 {0} 不存在")]
    NotFound(String),

    #[error("组件 {0} 重启次数超限")]
    RestartLimitExceeded(String),

    #[error("组件 {0} 关闭失败: {1}")]
    ShutdownFailed(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use x_data::state::StateCenterImpl;

    fn create_test_manager() -> (EngineManager, Arc<StateCenterImpl>) {
        let state_center = Arc::new(StateCenterImpl::new(30));
        let config = EngineManagerConfig {
            stale_threshold_secs: 30,
            restart_check_interval_secs: 1,
            shutdown_timeout_secs: 1,
        };
        let manager = EngineManager::new(config, state_center.clone());
        (manager, state_center)
    }

    fn make_task() -> impl FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()> + Send + 'static {
        |id, mut stop_rx| {
            tokio::spawn(async move {
                tokio::select! {
                    _ = stop_rx.recv() => {
                        tracing::info!("[{}] Stopped", id);
                    }
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                }
            })
        }
    }

    #[tokio::test]
    async fn test_spawn_and_shutdown() {
        let (manager, _state_center) = create_test_manager();

        let state = manager
            .spawn("test-engine".to_string(), make_task())
            .await;
        assert!(state.is_ok());
        assert_eq!(state.unwrap().component_id, "test-engine");

        manager.shutdown_one("test-engine").await.unwrap();
    }

    #[tokio::test]
    async fn test_spawn_duplicate() {
        let (manager, _state_center) = create_test_manager();

        let _ = manager.spawn("dup".to_string(), make_task()).await;
        let result = manager.spawn("dup".to_string(), make_task()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EngineError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn test_get_all_states() {
        let (manager, _state_center) = create_test_manager();

        for i in 0..3 {
            let _ = manager
                .spawn(format!("engine-{}", i), make_task())
                .await;
        }

        let states = manager.get_all_states().await;
        assert_eq!(states.len(), 3);
        assert_eq!(manager.len().await, 3);

        manager.shutdown_all().await;
        assert!(manager.is_empty().await);
    }

    #[tokio::test]
    async fn test_restart() {
        let (manager, _state_center) = create_test_manager();

        let _ = manager.spawn("restart-test".to_string(), make_task()).await;
        let result = manager.restart("restart-test".to_string(), make_task()).await;
        assert!(result.is_ok());

        manager.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_shutdown_nonexistent() {
        let (manager, _state_center) = create_test_manager();
        let result = manager.shutdown_one("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EngineError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_subscribe_shutdown() {
        let (manager, _state_center) = create_test_manager();
        let _rx = manager.subscribe_shutdown();
        // Should not panic
    }
}
