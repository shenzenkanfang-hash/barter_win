//! StateCenter - 轻量级组件状态中心
//!
//! 核心目标：知道"组件是否活着"。只记录生死状态和最后活跃时间，不承载业务数据。
//!
//! API 对齐设计规格: docs/superpowers/specs/2026-03-30-event-driven-architecture-design.md 第三节

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
#[cfg(test)]
use chrono::Utc;

use async_trait::async_trait;
use super::component::{ComponentState, ComponentStatus};

/// StateCenter 操作错误类型
///
/// 用于 `report_alive` 和 `report_error` 方法的返回值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateCenterError {
    /// 组件不存在
    ComponentNotFound(String),
    /// 组件已停止（不能报告活跃）
    ComponentStopped(String),
}

impl std::fmt::Display for StateCenterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateCenterError::ComponentNotFound(id) => {
                write!(f, "Component not found: {}", id)
            }
            StateCenterError::ComponentStopped(id) => {
                write!(f, "Component already stopped: {}", id)
            }
        }
    }
}

impl std::error::Error for StateCenterError {}

/// StateCenter trait - 轻量级组件状态中心接口
///
/// 对齐设计规格 3.3 节，定义状态中心的核心操作。
///
/// # 设计规格方法
/// - `report_alive` - 上报存活状态（设计规格 3.3）
/// - `report_error` - 上报错误状态（设计规格 3.3）
/// - `get` - 查询组件状态（设计规格 3.3）
/// - `get_all` - 查询所有组件状态（设计规格 3.3）
/// - `get_alive` - 获取所有存活的组件（设计规格 3.3）
/// - `get_stale` - 获取所有 Stale 的组件（设计规格 3.3）
#[async_trait]
pub trait StateCenter: Send + Sync {
    /// 注册新组件（初始状态为 Running）
    fn register(&self, component_id: String);

    /// 注册新组件（指定初始状态）
    fn register_with_status(&self, component_id: String, status: ComponentStatus);

    /// 上报存活状态（对齐设计规格 3.3）
    ///
    /// 标记组件为活跃，更新 `last_active` 时间戳。
    /// 如果组件处于 Stale 状态，重置为 Running。
    fn report_alive(&self, component_id: &str) -> Result<(), StateCenterError>;

    /// 上报错误状态（对齐设计规格 3.3）
    ///
    /// 将组件标记为 Stale 状态，并记录错误信息。
    fn report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError>;

    /// 停止组件
    ///
    /// 将组件状态设置为 Stopped。
    fn stop(&self, component_id: &str) -> Result<(), StateCenterError>;

    /// 获取组件状态（对齐设计规格 3.3）
    fn get(&self, component_id: &str) -> Option<ComponentState>;

    /// 检查组件是否存在
    fn contains(&self, component_id: &str) -> bool;

    /// 获取所有组件状态（对齐设计规格 3.3）
    fn get_all(&self) -> Vec<ComponentState>;

    /// 获取所有存活的组件（对齐设计规格 3.3）
    ///
    /// # Arguments
    /// * `timeout_secs` - 存活超时阈值（秒），超过此时间未活跃的组件不算存活
    fn get_alive(&self, timeout_secs: i64) -> Vec<ComponentState>;

    /// 获取所有 Stale 的组件（对齐设计规格 3.3）
    ///
    /// # Arguments
    /// * `threshold_secs` - Stale 判断阈值（秒），超过此时间未活跃的组件标记为 Stale
    fn get_stale(&self, threshold_secs: i64) -> Vec<ComponentState>;

    /// 检查是否有任何 Stale 的组件
    fn has_stale_components(&self) -> bool;

    /// 获取存活组件数量
    fn alive_count(&self) -> usize;

    /// 获取总组件数量
    fn total_count(&self) -> usize;

    /// 移除组件
    fn unregister(&self, component_id: &str) -> Option<ComponentState>;

    /// 清除所有组件
    fn clear(&self);

    /// 获取心跳超时阈值（秒）
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

    // ── Backward compatibility aliases ────────────────────────────────────────

    /// [Deprecated] 使用 `report_alive` 代替
    #[deprecated(since = "0.2.0", note = "Use report_alive instead")]
    pub fn heartbeat(&self, component_id: &str) -> Option<()> {
        self.report_alive(component_id).ok()
    }

    /// [Deprecated] 使用 `report_error` 代替
    #[deprecated(since = "0.2.0", note = "Use report_error instead")]
    pub fn set_error(&self, component_id: &str, error: String) -> Option<()> {
        self.report_error(component_id, &error).ok()
    }

    /// [Deprecated] 使用 `get` 代替
    #[deprecated(since = "0.2.0", note = "Use get instead")]
    pub fn get_state(&self, component_id: &str) -> Option<ComponentState> {
        self.get(component_id)
    }

    /// [Deprecated] 使用 `get_all` 代替
    #[deprecated(since = "0.2.0", note = "Use get_all instead")]
    pub fn get_all_states(&self) -> Vec<ComponentState> {
        self.get_all()
    }

    /// [Deprecated] 使用 `get_alive` 代替（使用内部阈值）
    #[deprecated(since = "0.2.0", note = "Use get_alive(threshold_secs) instead")]
    pub fn get_running_components(&self) -> Vec<ComponentState> {
        self.get_alive(self.stale_threshold_secs)
    }

    /// [Deprecated] 使用 `get_stale` 代替（使用内部阈值）
    #[deprecated(since = "0.2.0", note = "Use get_stale(threshold_secs) instead")]
    pub fn get_stale_components(&self) -> Vec<ComponentState> {
        self.get_stale(self.stale_threshold_secs)
    }
}

#[async_trait]
impl StateCenter for StateCenterImpl {
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

    fn report_alive(&self, component_id: &str) -> Result<(), StateCenterError> {
        let mut components = self.components.write();
        match components.get_mut(component_id) {
            Some(state) => {
                state.mark_alive();
                Ok(())
            }
            None => Err(StateCenterError::ComponentNotFound(component_id.to_string())),
        }
    }

    fn report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError> {
        let mut components = self.components.write();
        match components.get_mut(component_id) {
            Some(state) => {
                state.status = ComponentStatus::Stale;
                state.error_msg = Some(error.to_string());
                Ok(())
            }
            None => Err(StateCenterError::ComponentNotFound(component_id.to_string())),
        }
    }

    fn stop(&self, component_id: &str) -> Result<(), StateCenterError> {
        let mut components = self.components.write();
        match components.get_mut(component_id) {
            Some(state) => {
                state.status = ComponentStatus::Stopped;
                Ok(())
            }
            None => Err(StateCenterError::ComponentNotFound(component_id.to_string())),
        }
    }

    fn get(&self, component_id: &str) -> Option<ComponentState> {
        let components = self.components.read();
        components.get(component_id).cloned()
    }

    fn contains(&self, component_id: &str) -> bool {
        let components = self.components.read();
        components.contains_key(component_id)
    }

    fn get_all(&self) -> Vec<ComponentState> {
        let components = self.components.read();
        components.values().cloned().collect()
    }

    fn get_alive(&self, timeout_secs: i64) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.status == ComponentStatus::Running && !s.is_stale(timeout_secs))
            .cloned()
            .collect()
    }

    fn get_stale(&self, threshold_secs: i64) -> Vec<ComponentState> {
        let components = self.components.read();
        components
            .values()
            .filter(|s| s.is_stale(threshold_secs))
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

    // ── 基础注册测试 ─────────────────────────────────────────────────────────

    #[test]
    fn test_register() {
        let center = create_test_center();
        center.register("component1".to_string());

        assert!(center.contains("component1"));
        let state = center.get("component1").unwrap();
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
        let state = center.get("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stopped);
    }

    // ── report_alive 测试（对齐设计规格 3.3）────────────────────────────────

    #[test]
    fn test_report_alive() {
        let center = create_test_center();
        center.register("component1".to_string());

        let original_state = center.get("component1").unwrap();
        let original_time = original_state.last_active;

        std::thread::sleep(std::time::Duration::from_millis(10));

        center.report_alive("component1").unwrap();

        let updated_state = center.get("component1").unwrap();
        assert!(updated_state.last_active >= original_time);
    }

    #[test]
    fn test_report_alive_nonexistent() {
        let center = create_test_center();
        let result = center.report_alive("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateCenterError::ComponentNotFound(_)));
    }

    #[test]
    fn test_report_alive_from_stale() {
        let center = create_test_center();
        center.register("component1".to_string());

        // 模拟 stale
        let old_time = Utc::now() - chrono::Duration::seconds(120);
        {
            let mut components = center.components.write();
            if let Some(state) = components.get_mut("component1") {
                state.last_active = old_time;
                state.status = ComponentStatus::Stale;
            }
        }

        // report_alive 应该恢复为 Running
        center.report_alive("component1").unwrap();
        let state = center.get("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Running);
    }

    // ── report_error 测试（对齐设计规格 3.3）────────────────────────────────

    #[test]
    fn test_report_error() {
        let center = create_test_center();
        center.register("component1".to_string());

        center.report_error("component1", "connection lost").unwrap();

        let state = center.get("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stale);
        assert_eq!(state.error_msg, Some("connection lost".to_string()));
    }

    #[test]
    fn test_report_error_nonexistent() {
        let center = create_test_center();
        let result = center.report_error("nonexistent", "error");
        assert!(result.is_err());
    }

    // ── stop 测试 ─────────────────────────────────────────────────────────────

    #[test]
    fn test_stop() {
        let center = create_test_center();
        center.register("component1".to_string());

        center.stop("component1").unwrap();

        let state = center.get("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stopped);
    }

    #[test]
    fn test_stop_nonexistent() {
        let center = create_test_center();
        let result = center.stop("nonexistent");
        assert!(result.is_err());
    }

    // ── get/get_all 测试（对齐设计规格 3.3）────────────────────────────────

    #[test]
    fn test_get() {
        let center = create_test_center();
        center.register("component1".to_string());

        let state = center.get("component1").unwrap();
        assert_eq!(state.component_id, "component1");
    }

    #[test]
    fn test_get_nonexistent() {
        let center = create_test_center();
        assert!(center.get("nonexistent").is_none());
    }

    #[test]
    fn test_get_all() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());

        let all = center.get_all();
        assert_eq!(all.len(), 2);
    }

    // ── get_alive 测试（对齐设计规格 3.3）──────────────────────────────────

    #[test]
    fn test_get_alive() {
        let center = create_test_center();
        center.register("component1".to_string());
        center.register("component2".to_string());
        center.stop("component1").unwrap();

        let alive = center.get_alive(60);
        assert_eq!(alive.len(), 1);
        assert_eq!(alive[0].component_id, "component2");
    }

    #[test]
    fn test_get_alive_with_stale_threshold() {
        let center = create_test_center();
        center.register("component1".to_string());

        // 模拟旧的时间
        let old_time = Utc::now() - chrono::Duration::seconds(120);
        {
            let mut components = center.components.write();
            if let Some(state) = components.get_mut("component1") {
                state.last_active = old_time;
            }
        }

        // 60s 阈值：应该算 stale，不算 alive
        let alive = center.get_alive(60);
        assert_eq!(alive.len(), 0);

        // 180s 阈值：应该算 alive
        let alive = center.get_alive(180);
        assert_eq!(alive.len(), 1);
    }

    // ── get_stale 测试（对齐设计规格 3.3）───────────────────────────────────

    #[test]
    fn test_get_stale() {
        let center = create_test_center();
        center.register("component1".to_string());

        // 模拟旧的时间
        let old_time = Utc::now() - chrono::Duration::seconds(120);
        {
            let mut components = center.components.write();
            if let Some(state) = components.get_mut("component1") {
                state.last_active = old_time;
            }
        }

        let stale = center.get_stale(60);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].component_id, "component1");

        let stale = center.get_stale(180);
        assert_eq!(stale.len(), 0);
    }

    #[test]
    fn test_has_stale_components() {
        let center = create_test_center();
        center.register("component1".to_string());

        assert!(!center.has_stale_components());

        let old_time = Utc::now() - chrono::Duration::seconds(120);
        {
            let mut components = center.components.write();
            if let Some(state) = components.get_mut("component1") {
                state.last_active = old_time;
            }
        }

        assert!(center.has_stale_components());
    }

    // ── 计数和清理测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_unregister() {
        let center = create_test_center();
        center.register("component1".to_string());
        assert!(center.contains("component1"));

        center.unregister("component1");
        assert!(!center.contains("component1"));
        assert!(center.get("component1").is_none());
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
        center.register("component3".to_string());
        center.stop("component3").unwrap();

        assert_eq!(center.total_count(), 3);
        assert_eq!(center.alive_count(), 2);
    }

    #[test]
    fn test_stale_threshold() {
        let center = StateCenterImpl::new(120);
        assert_eq!(center.stale_threshold(), 120);
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

        let state = center.get("component1").unwrap();
        assert_eq!(state.component_id, "component1");
    }

    // ── Backward compatibility 测试 ───────────────────────────────────────────

    #[test]
    #[allow(deprecated)]
    fn test_heartbeat_alias() {
        let center = create_test_center();
        center.register("component1".to_string());

        // heartbeat 应与 report_alive 等效
        #[allow(deprecated)]
        let result = center.heartbeat("component1");
        assert!(result.is_some());
    }

    #[test]
    #[allow(deprecated)]
    fn test_set_error_alias() {
        let center = create_test_center();
        center.register("component1".to_string());

        #[allow(deprecated)]
        let result = center.set_error("component1", "test".to_string());
        assert!(result.is_some());

        #[allow(deprecated)]
        let state = center.get_state("component1").unwrap();
        assert_eq!(state.status, ComponentStatus::Stale);
    }
}
