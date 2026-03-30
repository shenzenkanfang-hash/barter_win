//! StateCenter - 轻量级组件状态中心
//!
//! 核心目标：知道"组件是否活着"。只记录生死状态和最后活跃时间，不承载业务数据。

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
#[cfg(test)]
use chrono::Utc;

use async_trait::async_trait;
use super::component::{ComponentState, ComponentStatus};

/// StateCenter trait - 轻量级组件状态中心接口
///
/// 定义状态中心的核心操作，用于追踪组件的生命状态。
/// 只记录：组件 ID、运行状态、最后活跃时间。
///
/// 不承载业务数据，是 pure infrastructure 层。
#[async_trait]
pub trait StateCenterTrait: Send + Sync {
    /// 注册新组件（初始状态为 Running）
    fn register(&self, component_id: String);

    /// 注册新组件（指定初始状态）
    fn register_with_status(&self, component_id: String, status: ComponentStatus);

    /// 心跳更新（标记组件为活跃）
    fn heartbeat(&self, component_id: &str) -> Option<()>;

    /// 停止组件
    fn stop(&self, component_id: &str) -> Option<()>;

    /// 设置组件错误状态
    fn set_error(&self, component_id: &str, error: String) -> Option<()>;

    /// 获取组件状态（只读）
    fn get_state(&self, component_id: &str) -> Option<ComponentState>;

    /// 检查组件是否存在
    fn contains(&self, component_id: &str) -> bool;

    /// 获取所有组件状态
    fn get_all_states(&self) -> Vec<ComponentState>;

    /// 获取所有 Running 状态的组件
    fn get_running_components(&self) -> Vec<ComponentState>;

    /// 获取所有已停止的组件
    fn get_stopped_components(&self) -> Vec<ComponentState>;

    /// 获取所有超时的组件（基于时间阈值）
    fn get_stale_components(&self) -> Vec<ComponentState>;

    /// 检查是否有任何超时的组件
    fn has_stale_components(&self) -> bool;

    /// 获取存活组件数量（Running 且未超时）
    fn alive_count(&self) -> usize;

    /// 获取总组件数量
    fn total_count(&self) -> usize;

    /// 移除组件
    fn unregister(&self, component_id: &str) -> Option<ComponentState>;

    /// 清除所有组件
    fn clear(&self);

    /// 获取心跳超时阈值
    fn stale_threshold(&self) -> i64;
}

/// StateCenterImpl - StateCenter trait 的具体实现
#[derive(Debug)]
pub struct StateCenterImpl {
    /// 组件状态表
    components: RwLock<HashMap<String, ComponentState>>,
    /// 心跳超时阈值（秒）
    stale_threshold_secs: i64,
}

impl StateCenterImpl {
    /// 创建新的 StateCenterImpl
    ///
    /// # Arguments
    /// * `stale_threshold_secs` - 心跳超时阈值，超过此时间未更新则标记为 Stale
    pub fn new(stale_threshold_secs: i64) -> Self {
        Self {
            components: RwLock::new(HashMap::new()),
            stale_threshold_secs,
        }
    }

    /// 创建带 Arc 的 StateCenterImpl（用于跨线程共享）
    pub fn new_arc(stale_threshold_secs: i64) -> Arc<Self> {
        Arc::new(Self::new(stale_threshold_secs))
    }
}

#[async_trait]
impl StateCenterTrait for StateCenterImpl {
    fn register(&self, component_id: String) {
        let state = ComponentState::new_running(component_id.clone());
        let mut components = self.components.write();
        components.insert(component_id, state);
    }

    fn register_with_status(&self, component_id: String, status: ComponentStatus) {
        let state = match status {
            ComponentStatus::Running => ComponentState::new_running(component_id.clone()),
            ComponentStatus::Stopped => ComponentState::new_stopped(component_id.clone()),
            ComponentStatus::Stale => ComponentState::new_running(component_id.clone()),
        };
        let mut components = self.components.write();
        components.insert(component_id, state);
    }

    fn heartbeat(&self, component_id: &str) -> Option<()> {
        let mut components = self.components.write();
        if let Some(state) = components.get_mut(component_id) {
            state.mark_alive();
            Some(())
        } else {
            None
        }
    }

    fn stop(&self, component_id: &str) -> Option<()> {
        let mut components = self.components.write();
        if let Some(state) = components.get_mut(component_id) {
            state.status = ComponentStatus::Stopped;
            Some(())
        } else {
            None
        }
    }

    fn set_error(&self, component_id: &str, error: String) -> Option<()> {
        let mut components = self.components.write();
        if let Some(state) = components.get_mut(component_id) {
            state.status = ComponentStatus::Stale;
            state.error_msg = Some(error);
            Some(())
        } else {
            None
        }
    }

    fn get_state(&self, component_id: &str) -> Option<ComponentState> {
        let components = self.components.read();
        components.get(component_id).cloned()
    }

    fn contains(&self, component_id: &str) -> bool {
        let components = self.components.read();
        components.contains_key(component_id)
    }

    fn get_all_states(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components.values().cloned().collect()
    }

    fn get_running_components(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Running)
            .cloned()
            .collect()
    }

    fn get_stopped_components(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Stopped)
            .cloned()
            .collect()
    }

    fn get_stale_components(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.is_stale(self.stale_threshold_secs))
            .cloned()
            .collect()
    }

    fn has_stale_components(&self) -> bool {
        let components = self.components.read();
        components
            .values()
            .any(|s| s.is_stale(self.stale_threshold_secs))
    }

    fn alive_count(&self) -> usize {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Running && !s.is_stale(self.stale_threshold_secs))
            .count()
    }

    fn total_count(&self) -> usize {
        let components = self.components.read();
        components.len()
    }

    fn unregister(&self, component_id: &str) -> Option<ComponentState> {
        let mut components = self.components.write();
        components.remove(component_id)
    }

    fn clear(&self) {
        let mut components = self.components.write();
        components.clear();
    }

    fn stale_threshold(&self) -> i64 {
        self.stale_threshold_secs
    }
}

/// Type alias for backward compatibility
pub type StateCenter = StateCenterImpl;

impl Default for StateCenterImpl {
    fn default() -> Self {
        Self::new(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_center() -> StateCenterImpl {
        StateCenterImpl::new(60)
    }

    #[test]
    fn test_register() {
        let center = create_test_center();
        center.register("component1".to_string());

        assert!(center.contains("component1"));
        let state = center.get_state("component1").unwrap();
        assert_eq!(state.component_id, "component1");
        assert_eq!(state.status, ComponentStatus::Running);
    }

    #[test]
    fn test_register_multiple() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());
        center.register("component3".to_string());

        assert_eq!(center.total_count(), 3);
    }

    #[test]
    fn test_register_with_status() {
        let center = create_test_center();
        center.register_with_status("component1".to_string(), ComponentStatus::Stopped);

        let state = center.get_state("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stopped);
    }

    #[test]
    fn test_heartbeat() {
        let center = create_test_center();
        center.register("component1".to_string());

        let original_state = center.get_state("component1").unwrap();
        let original_time = original_state.last_active;

        std::thread::sleep(std::time::Duration::from_millis(10));

        center.heartbeat("component1").unwrap();

        let updated_state = center.get_state("component1").unwrap();
        assert!(updated_state.last_active >= original_time);
    }

    #[test]
    fn test_heartbeat_nonexistent() {
        let center = create_test_center();
        assert!(center.heartbeat("nonexistent").is_none());
    }

    #[test]
    fn test_stop() {
        let center = create_test_center();
        center.register("component1".to_string());

        center.stop("component1").unwrap();

        let state = center.get_state("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stopped);
    }

    #[test]
    fn test_set_error() {
        let center = create_test_center();
        center.register("component1".to_string());

        center.set_error("component1", "test error message".to_string()).unwrap();

        let state = center.get_state("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stale);
        assert_eq!(state.error_msg, Some("test error message".to_string()));
    }

    #[test]
    fn test_get_running_components() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());
        center.stop("component1").unwrap();

        let running = center.get_running_components();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].component_id, "component2");
    }

    #[test]
    fn test_get_stopped_components() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());
        center.stop("component1").unwrap();

        let stopped = center.get_stopped_components();
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0].component_id, "component1");
    }

    #[test]
    fn test_unregister() {
        let center = create_test_center();
        center.register("component1".to_string());
        assert!(center.contains("component1"));

        center.unregister("component1");

        assert!(!center.contains("component1"));
        assert!(center.get_state("component1").is_none());
    }

    #[test]
    fn test_clear() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());

        center.clear();

        assert_eq!(center.total_count(), 0);
    }

    #[test]
    fn test_alive_count() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());
        center.stop("component3");

        center.register("component3".to_string());
        center.stop("component3").unwrap();

        assert_eq!(center.total_count(), 3);
        assert_eq!(center.alive_count(), 2);
    }

    #[test]
    fn test_stale_threshold() {
        let center = StateCenterImpl::new(120);
        assert_eq!(center.stale_threshold(), 120);

        center.register("component1".to_string());

        assert!(!center.has_stale_components());

        let old_time = Utc::now() - chrono::Duration::seconds(180);
        {
            let mut components = center.components.write();
            if let Some(state) = components.get_mut("component1") {
                state.last_active = old_time;
            }
        }

        assert!(center.has_stale_components());
        let stale = center.get_stale_components();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].component_id, "component1");
    }

    #[test]
    fn test_default_threshold() {
        let center = StateCenterImpl::default();
        assert_eq!(center.stale_threshold(), 60);
    }

    #[test]
    fn test_new_arc() {
        let center = StateCenterImpl::new_arc(60);
        center.register("component1".to_string());

        let state = center.get_state("component1").unwrap();
        assert_eq!(state.component_id, "component1");
    }
}
