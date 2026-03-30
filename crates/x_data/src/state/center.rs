//! StateCenter - 轻量级组件状态中心
//!
//! 核心目标：知道"组件是否活着"。只记录生死状态和最后活跃时间，不承载业务数据。

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
#[cfg(test)]
use chrono::Utc;

use super::component::{ComponentState, ComponentStatus};

/// StateCenter 轻量级状态中心
///
/// 用于追踪组件的生命状态，只记录：
/// - 组件 ID
/// - 运行状态（Running/Stopped/Stale）
/// - 最后活跃时间
///
/// 不承载业务数据，是 pure infrastructure 层。
#[derive(Debug)]
pub struct StateCenter {
    /// 组件状态表
    components: RwLock<HashMap<String, ComponentState>>,
    /// 心跳超时阈值（秒）
    stale_threshold_secs: i64,
}

impl StateCenter {
    /// 创建新的 StateCenter
    ///
    /// # Arguments
    /// * `stale_threshold_secs` - 心跳超时阈值，超过此时间未更新则标记为 Stale
    pub fn new(stale_threshold_secs: i64) -> Self {
        Self {
            components: RwLock::new(HashMap::new()),
            stale_threshold_secs,
        }
    }

    /// 创建带 Arc 的 StateCenter（用于跨线程共享）
    pub fn new_arc(stale_threshold_secs: i64) -> Arc<Self> {
        Arc::new(Self::new(stale_threshold_secs))
    }

    /// 注册新组件（初始状态为 Running）
    pub fn register(&self, component_id: String) {
        let state = ComponentState::new_running(component_id.clone());
        let mut components = self.components.write();
        components.insert(component_id, state);
    }

    /// 注册新组件（指定初始状态）
    pub fn register_with_status(&self, component_id: String, status: ComponentStatus) {
        let state = match status {
            ComponentStatus::Running => ComponentState::new_running(component_id.clone()),
            ComponentStatus::Stopped => ComponentState::new_stopped(component_id.clone()),
            ComponentStatus::Stale => ComponentState::new_running(component_id.clone()), // Stale 初始按 Running 处理
        };
        let mut components = self.components.write();
        components.insert(component_id, state);
    }

    /// 心跳更新（标记组件为活跃）
    pub fn heartbeat(&self, component_id: &str) -> Option<()> {
        let mut components = self.components.write();
        if let Some(state) = components.get_mut(component_id) {
            state.mark_alive();
            Some(())
        } else {
            None
        }
    }

    /// 停止组件
    pub fn stop(&self, component_id: &str) -> Option<()> {
        let mut components = self.components.write();
        if let Some(state) = components.get_mut(component_id) {
            state.status = ComponentStatus::Stopped;
            Some(())
        } else {
            None
        }
    }

    /// 设置组件错误状态
    pub fn set_error(&self, component_id: &str, error: String) -> Option<()> {
        let mut components = self.components.write();
        if let Some(state) = components.get_mut(component_id) {
            state.status = ComponentStatus::Stopped;
            state.error_msg = Some(error);
            Some(())
        } else {
            None
        }
    }

    /// 获取组件状态（只读）
    pub fn get_state(&self, component_id: &str) -> Option<ComponentState> {
        let components = self.components.read();
        components.get(component_id).cloned()
    }

    /// 检查组件是否存在
    pub fn contains(&self, component_id: &str) -> bool {
        let components = self.components.read();
        components.contains_key(component_id)
    }

    /// 获取所有组件状态
    pub fn get_all_states(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components.values().cloned().collect()
    }

    /// 获取所有 Running 状态的组件
    pub fn get_running_components(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Running)
            .cloned()
            .collect()
    }

    /// 获取所有已停止的组件
    pub fn get_stopped_components(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Stopped)
            .cloned()
            .collect()
    }

    /// 获取所有超时的组件（基于时间阈值）
    pub fn get_stale_components(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.is_stale(self.stale_threshold_secs))
            .cloned()
            .collect()
    }

    /// 检查是否有任何超时的组件
    pub fn has_stale_components(&self) -> bool {
        let components = self.components.read();
        components
            .values()
            .any(|s| s.is_stale(self.stale_threshold_secs))
    }

    /// 获取存活组件数量（Running 且未超时）
    pub fn alive_count(&self) -> usize {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Running && !s.is_stale(self.stale_threshold_secs))
            .count()
    }

    /// 获取总组件数量
    pub fn total_count(&self) -> usize {
        let components = self.components.read();
        components.len()
    }

    /// 移除组件
    pub fn unregister(&self, component_id: &str) -> Option<ComponentState> {
        let mut components = self.components.write();
        components.remove(component_id)
    }

    /// 清除所有组件
    pub fn clear(&self) {
        let mut components = self.components.write();
        components.clear();
    }

    /// 获取心跳超时阈值
    pub fn stale_threshold(&self) -> i64 {
        self.stale_threshold_secs
    }

    /// 更新心跳超时阈值
    pub fn set_stale_threshold(&mut self, threshold_secs: i64) {
        self.stale_threshold_secs = threshold_secs;
    }
}

impl Default for StateCenter {
    fn default() -> Self {
        // 默认 60 秒超时
        Self::new(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_center() -> StateCenter {
        StateCenter::new(60)
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
        assert_eq!(state.status, ComponentStatus::Stopped);
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
        center.stop("component3"); // 不存在，不会增加总数

        // 注意：stop 一个不存在的组件会返回 None
        center.register("component3".to_string());
        center.stop("component3").unwrap();

        assert_eq!(center.total_count(), 3);
        assert_eq!(center.alive_count(), 2); // component1 和 component2 是 Running
    }

    #[test]
    fn test_stale_threshold() {
        let center = StateCenter::new(120);
        assert_eq!(center.stale_threshold(), 120);

        center.register("component1".to_string());

        // 此时不应该 stale（阈值 120 秒）
        assert!(!center.has_stale_components());

        // 使用旧的 last_active 来模拟超时
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
        let center = StateCenter::default();
        assert_eq!(center.stale_threshold(), 60);
    }

    #[test]
    fn test_new_arc() {
        let center = StateCenter::new_arc(60);
        center.register("component1".to_string());

        // 验证 Arc 正常工作
        let state = center.get_state("component1").unwrap();
        assert_eq!(state.component_id, "component1");
    }
}
