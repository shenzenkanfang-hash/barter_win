//! strategy_service.rs - 策略服务统一接口
//!
//! 为所有策略协程提供统一的生命周期管理接口。
//! EngineManager 通过此 trait 管理所有策略的 spawn/stop/health_report。

#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// 策略服务错误类型
#[derive(Debug, thiserror::Error)]
pub enum StrategyServiceError {
    #[error("策略已停止")]
    AlreadyStopped,

    #[error("策略运行中")]
    AlreadyRunning,

    #[error("启动失败: {0}")]
    StartFailed(String),

    #[error("停止失败: {0}")]
    StopFailed(String),

    #[error("健康检查失败: {0}")]
    HealthCheckFailed(String),
}

/// 策略健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyHealth {
    /// 正常运行
    Healthy,
    /// 异常但可恢复
    Degraded,
    /// 不可用
    Unhealthy,
    /// 已停止
    Stopped,
}

impl Default for StrategyHealth {
    fn default() -> Self {
        StrategyHealth::Healthy
    }
}

/// 策略运行信息
#[derive(Debug, Clone)]
pub struct StrategyInfo {
    /// 策略标识
    pub strategy_id: String,
    /// 策略类型
    pub strategy_type: StrategyType,
    /// 当前健康状态
    pub health: StrategyHealth,
    /// 启动时间
    pub started_at: Option<DateTime<Utc>>,
    /// 最后活跃时间
    pub last_active_at: Option<DateTime<Utc>>,
    /// 错误信息
    pub error_msg: Option<String>,
}

impl StrategyInfo {
    pub fn new(strategy_id: String, strategy_type: StrategyType) -> Self {
        Self {
            strategy_id,
            strategy_type,
            health: StrategyHealth::Stopped,
            started_at: None,
            last_active_at: None,
            error_msg: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.health = StrategyHealth::Healthy;
        self.started_at = Some(Utc::now());
        self.last_active_at = Some(Utc::now());
        self.error_msg = None;
    }

    pub fn mark_degraded(&mut self, reason: &str) {
        self.health = StrategyHealth::Degraded;
        self.last_active_at = Some(Utc::now());
        self.error_msg = Some(reason.to_string());
    }

    pub fn mark_unhealthy(&mut self, reason: &str) {
        self.health = StrategyHealth::Unhealthy;
        self.error_msg = Some(reason.to_string());
    }

    pub fn mark_stopped(&mut self) {
        self.health = StrategyHealth::Stopped;
    }
}

/// 策略类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyType {
    /// 高频15分钟策略
    HighFrequency15m,
    /// 低频1天策略
    LowFrequency1d,
    /// 高波动率交易器
    HighVolatility,
    /// 未知类型
    Unknown,
}

impl Default for StrategyType {
    fn default() -> Self {
        StrategyType::Unknown
    }
}

/// StrategyService - 策略协程统一接口
///
/// 所有策略协程必须实现此 trait，EngineManager 通过此接口
/// 统一管理策略的生命周期。
#[async_trait]
pub trait StrategyService: Send + Sync {
    /// 获取策略基本信息
    fn strategy_info(&self) -> StrategyInfo;

    /// 启动策略协程
    ///
    /// 返回启动结果，成功时返回 JoinHandle 可用于等待协程结束
    async fn start(&self) -> Result<(), StrategyServiceError>;

    /// 停止策略协程
    async fn stop(&self) -> Result<(), StrategyServiceError>;

    /// 检查策略健康状态
    async fn health_check(&self) -> Result<StrategyHealth, StrategyServiceError>;

    /// 获取策略运行快照（用于监控）
    async fn snapshot(&self) -> Result<StrategySnapshot, StrategyServiceError>;
}

/// 策略运行快照（用于监控/日志）
#[derive(Debug, Clone)]
pub struct StrategySnapshot {
    /// 策略标识
    pub strategy_id: String,
    /// 健康状态
    pub health: StrategyHealth,
    /// 运行时间（秒）
    pub running_secs: i64,
    /// 最后活跃距今秒数
    pub idle_secs: i64,
    /// 错误信息
    pub error_msg: Option<String>,
}

impl StrategySnapshot {
    pub fn from_info(info: &StrategyInfo) -> Self {
        let running_secs = info
            .started_at
            .map(|started| Utc::now().signed_duration_since(started).num_seconds())
            .unwrap_or(0);

        let idle_secs = info
            .last_active_at
            .map(|last| Utc::now().signed_duration_since(last).num_seconds())
            .unwrap_or(i64::MAX);

        Self {
            strategy_id: info.strategy_id.clone(),
            health: info.health,
            running_secs,
            idle_secs,
            error_msg: info.error_msg.clone(),
        }
    }
}

/// StrategyServiceRegistry - 策略服务注册表
///
/// 用于 EngineManager 管理所有注册的策略服务
pub trait StrategyServiceRegistry: Send + Sync {
    /// 注册策略服务
    fn register(&self, service: Arc<dyn StrategyService>) -> Result<(), StrategyServiceError>;

    /// 注销策略服务
    fn unregister(&self, strategy_id: &str) -> Result<(), StrategyServiceError>;

    /// 获取策略服务
    fn get(&self, strategy_id: &str) -> Option<Arc<dyn StrategyService>>;

    /// 获取所有策略服务
    fn get_all(&self) -> Vec<Arc<dyn StrategyService>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_info_creation() {
        let info = StrategyInfo::new(
            "test-001".to_string(),
            StrategyType::HighFrequency15m,
        );

        assert_eq!(info.strategy_id, "test-001");
        assert_eq!(info.strategy_type, StrategyType::HighFrequency15m);
        assert_eq!(info.health, StrategyHealth::Stopped);
        assert!(info.started_at.is_none());
        assert!(info.error_msg.is_none());
    }

    #[test]
    fn test_strategy_info_mark_running() {
        let mut info = StrategyInfo::new(
            "test-001".to_string(),
            StrategyType::HighFrequency15m,
        );

        info.mark_running();

        assert_eq!(info.health, StrategyHealth::Healthy);
        assert!(info.started_at.is_some());
        assert!(info.last_active_at.is_some());
        assert!(info.error_msg.is_none());
    }

    #[test]
    fn test_strategy_info_mark_degraded() {
        let mut info = StrategyInfo::new(
            "test-001".to_string(),
            StrategyType::HighFrequency15m,
        );

        info.mark_running();
        info.mark_degraded("high latency");

        assert_eq!(info.health, StrategyHealth::Degraded);
        assert_eq!(info.error_msg, Some("high latency".to_string()));
    }

    #[test]
    fn test_strategy_snapshot_from_info() {
        let mut info = StrategyInfo::new(
            "test-001".to_string(),
            StrategyType::HighFrequency15m,
        );

        info.mark_running();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let snapshot = StrategySnapshot::from_info(&info);

        assert_eq!(snapshot.strategy_id, "test-001");
        assert_eq!(snapshot.health, StrategyHealth::Healthy);
        assert!(snapshot.running_secs >= 0);
        assert!(snapshot.idle_secs <= 1); // 10ms ~= 0s or 1s
    }

    #[test]
    fn test_strategy_type_default() {
        let default_type = StrategyType::default();
        assert_eq!(default_type, StrategyType::Unknown);
    }
}
