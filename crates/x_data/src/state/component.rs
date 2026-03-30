//! ComponentState - 组件状态数据结构
//!
//! 用于 StateCenter 轻量级状态中心，记录组件生死状态和最后活跃时间。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 组件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentStatus {
    /// 正常运行
    Running,
    /// 已停止
    Stopped,
    /// 心跳超时（疑似死亡）
    Stale,
}

/// 组件状态记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentState {
    /// 组件唯一标识
    pub component_id: String,
    /// 当前状态
    pub status: ComponentStatus,
    /// 最后活跃时间
    pub last_active: DateTime<Utc>,
    /// 错误信息（如果有）
    pub error_msg: Option<String>,
}

impl ComponentState {
    /// 创建新的运行中状态
    pub fn new_running(component_id: String) -> Self {
        Self {
            component_id,
            status: ComponentStatus::Running,
            last_active: Utc::now(),
            error_msg: None,
        }
    }

    /// 创建新的已停止状态
    pub fn new_stopped(component_id: String) -> Self {
        Self {
            component_id,
            status: ComponentStatus::Stopped,
            last_active: Utc::now(),
            error_msg: None,
        }
    }

    /// 创建新的错误状态
    pub fn new_error(component_id: String, error: String) -> Self {
        Self {
            component_id,
            status: ComponentStatus::Stopped,
            last_active: Utc::now(),
            error_msg: Some(error),
        }
    }

    /// 更新为活跃状态
    pub fn mark_alive(&mut self) {
        self.last_active = Utc::now();
        if self.status == ComponentStatus::Stale {
            self.status = ComponentStatus::Running;
        }
        self.error_msg = None;
    }

    /// 检查是否超时（基于时间阈值判断是否为 Stale）
    pub fn is_stale(&self, threshold_secs: i64) -> bool {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.last_active);
        elapsed.num_seconds() >= threshold_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_running() {
        let state = ComponentState::new_running("test-component".to_string());
        assert_eq!(state.component_id, "test-component");
        assert_eq!(state.status, ComponentStatus::Running);
        assert!(state.error_msg.is_none());
    }

    #[test]
    fn test_new_stopped() {
        let state = ComponentState::new_stopped("test-component".to_string());
        assert_eq!(state.status, ComponentStatus::Stopped);
        assert!(state.error_msg.is_none());
    }

    #[test]
    fn test_new_error() {
        let state = ComponentState::new_error("test-component".to_string(), "test error".to_string());
        assert_eq!(state.status, ComponentStatus::Stopped);
        assert_eq!(state.error_msg, Some("test error".to_string()));
    }

    #[test]
    fn test_mark_alive() {
        let mut state = ComponentState::new_running("test-component".to_string());
        let original_time = state.last_active;

        // 使用模拟时间测试
        std::thread::sleep(std::time::Duration::from_millis(10));
        state.mark_alive();

        assert!(state.last_active >= original_time);
        assert_eq!(state.status, ComponentStatus::Running);
        assert!(state.error_msg.is_none());
    }

    #[test]
    fn test_mark_alive_from_stale() {
        let mut state = ComponentState::new_running("test-component".to_string());
        state.status = ComponentStatus::Stale;

        state.mark_alive();

        assert_eq!(state.status, ComponentStatus::Running);
    }

    #[test]
    fn test_is_stale() {
        let state = ComponentState::new_running("test-component".to_string());

        // 当前应该是活的
        assert!(!state.is_stale(60));

        // 模拟旧的时间（创建时就已经很旧）
        let old_time = Utc::now() - chrono::Duration::seconds(120);
        let mut old_state = ComponentState::new_running("old-component".to_string());
        old_state.last_active = old_time;

        assert!(old_state.is_stale(60));
        assert!(!old_state.is_stale(180));
    }
}
