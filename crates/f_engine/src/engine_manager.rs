//! EngineManager - 协程生命周期管理器
//!
//! 管理多个协程的 spawn、restart、shutdown 功能。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use x_data::state::{ComponentState, ComponentStatus};

/// 协程条目
struct EngineEntry {
    /// 协程状态
    state: ComponentState,
    /// JoinHandle
    handle: JoinHandle<()>,
    /// 停止信号发送器
    stop_tx: mpsc::Sender<()>,
}

/// EngineManager 配置
#[derive(Debug, Clone)]
pub struct EngineManagerConfig {
    /// 心跳超时阈值（秒）
    pub heartbeat_timeout_secs: i64,
    /// 最大重启次数
    pub max_restart_count: u32,
    /// 重启间隔（秒）
    pub restart_interval_secs: u64,
}

impl Default for EngineManagerConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_secs: 30,
            max_restart_count: 3,
            restart_interval_secs: 5,
        }
    }
}

/// EngineManager - 协程生命周期管理器
///
/// # 功能
/// - spawn: 启动新协程
/// - restart: 重启指定协程
/// - shutdown: 优雅关闭协程
/// - 心跳跟踪: 记录每个协程的最后活跃时间
pub struct EngineManager {
    /// 配置
    config: EngineManagerConfig,
    /// 协程表
    entries: Arc<RwLock<HashMap<String, EngineEntry>>>,
}

impl EngineManager {
    /// 创建新的 EngineManager
    pub fn new(config: EngineManagerConfig) -> Self {
        Self {
            config,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 启动新协程
    ///
    /// # Arguments
    /// * `component_id` - 组件唯一标识
    /// * `task` - 异步任务，签名：`FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()>`
    ///
    /// # Returns
    /// 成功返回新协程的状态，失败返回错误
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

        // 创建协程状态
        let state = ComponentState::new_running(component_id.clone());

        // 创建任务（提前克隆用于日志）
        let task_component_id = component_id.clone();
        let handle = tokio::spawn(async move {
            let _ = task(task_component_id, stop_rx).await;
        });

        // 注册协程
        {
            let mut entries = self.entries.write().await;
            entries.insert(
                component_id.clone(),
                EngineEntry {
                    state: state.clone(),
                    handle,
                    stop_tx,
                },
            );
        }

        tracing::info!("[EngineManager] Spawned: {}", component_id);
        Ok(state)
    }

    /// 重启指定协程
    ///
    /// # Arguments
    /// * `component_id` - 组件唯一标识
    /// * `task` - 新的异步任务，签名：`FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()>`
    ///
    /// # Returns
    /// 成功返回新协程的状态，失败返回错误
    pub async fn restart<F>(&self, component_id: String, task: F) -> Result<ComponentState, EngineError>
    where
        F: FnOnce(String, mpsc::Receiver<()>) -> tokio::task::JoinHandle<()> + Send + 'static,
    {
        // 先关闭旧的
        self.shutdown_one(&component_id).await?;

        // 创建新的
        self.spawn(component_id, task).await
    }

    /// 关闭指定协程
    pub async fn shutdown_one(&self, component_id: &str) -> Result<(), EngineError> {
        let entry = {
            let mut entries = self.entries.write().await;
            entries.remove(component_id)
        };

        match entry {
            Some(mut e) => {
                // 发送停止信号
                let _ = e.stop_tx.send(()).await;

                // 等待协程结束（带超时）
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(self.config.restart_interval_secs),
                    &mut e.handle,
                )
                .await;

                e.state.status = ComponentStatus::Stopped;
                tracing::info!("[EngineManager] Shutdown: {}", component_id);
                Ok(())
            }
            None => Err(EngineError::NotFound(component_id.to_string())),
        }
    }

    /// 关闭所有协程
    pub async fn shutdown_all(&self) {
        // 获取所有条目
        let entries: Vec<(String, EngineEntry)> = {
            let mut entries = self.entries.write().await;
            entries.drain().collect()
        };

        for (id, mut entry) in entries {
            // 发送停止信号
            let _ = entry.stop_tx.send(()).await;

            // 等待协程结束
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(self.config.restart_interval_secs),
                &mut entry.handle,
            )
            .await;

            entry.state.status = ComponentStatus::Stopped;
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

    /// 检查协程是否存活（心跳超时检测）
    pub async fn is_alive(&self, component_id: &str) -> bool {
        let entries = self.entries.read().await;
        match entries.get(component_id) {
            Some(e) => {
                !e.state.is_stale(self.config.heartbeat_timeout_secs)
            }
            None => false,
        }
    }

    /// 更新心跳（标记为活跃）
    pub async fn heartbeat(&self, component_id: &str) -> Result<(), EngineError> {
        let mut entries = self.entries.write().await;
        match entries.get_mut(component_id) {
            Some(e) => {
                e.state.mark_alive();
                Ok(())
            }
            None => Err(EngineError::NotFound(component_id.to_string())),
        }
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

    #[tokio::test]
    async fn test_spawn_and_shutdown() {
        let manager = EngineManager::new(EngineManagerConfig::default());

        // Spawn 一个协程
        let state = manager
            .spawn("test-engine".to_string(), |id, mut stop_rx| {
                tokio::spawn(async move {
                    tokio::select! {
                        _ = stop_rx.recv() => {
                            tracing::info!("[{}] Received stop signal", id);
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                            tracing::info!("[{}] Timeout", id);
                        }
                    }
                })
            })
            .await;

        assert!(state.is_ok());
        assert_eq!(state.unwrap().component_id, "test-engine");

        // 检查状态
        assert!(manager.is_alive("test-engine").await);

        // 关闭
        manager.shutdown_one("test-engine").await.unwrap();

        // 再次检查状态
        assert!(!manager.is_alive("test-engine").await);
    }

    #[tokio::test]
    async fn test_spawn_duplicate() {
        let manager = EngineManager::new(EngineManagerConfig::default());

        // Spawn 第一个
        let _ = manager
            .spawn("dup-engine".to_string(), |id, stop_rx| {
                tokio::spawn(async move {
                    let _ = stop_rx;
                })
            })
            .await;

        // 尝试重复 Spawn
        let result = manager
            .spawn("dup-engine".to_string(), |id, stop_rx| {
                tokio::spawn(async move {
                    let _ = stop_rx;
                })
            })
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EngineError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let manager = EngineManager::new(EngineManagerConfig::default());

        // Spawn
        let _ = manager
            .spawn("hb-engine".to_string(), |id, stop_rx| {
                tokio::spawn(async move {
                    let _ = stop_rx;
                })
            })
            .await;

        // 更新心跳
        let result = manager.heartbeat("hb-engine").await;
        assert!(result.is_ok());

        // 获取状态
        let state = manager.get_state("hb-engine").await;
        assert!(state.is_some());
        assert_eq!(state.unwrap().status, ComponentStatus::Running);
    }

    #[tokio::test]
    async fn test_get_all_states() {
        let manager = EngineManager::new(EngineManagerConfig::default());

        // Spawn 多个
        for i in 0..3 {
            let _ = manager
                .spawn(format!("engine-{}", i), |id, stop_rx| {
                    tokio::spawn(async move {
                        let _ = stop_rx;
                    })
                })
                .await;
        }

        // 获取所有状态
        let states = manager.get_all_states().await;
        assert_eq!(states.len(), 3);
        assert_eq!(manager.len().await, 3);

        // 清理
        manager.shutdown_all().await;
        assert!(manager.is_empty().await);
    }
}
