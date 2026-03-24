//! 监控与超时模块
//!
//! 实现超时监控、健康检查、API 健康检查。

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

/// 超时监控器
pub struct TimeoutMonitor {
    /// 服务名称 -> 最后活跃时间
    services: Arc<RwLock<HashMap<String, ServiceStatus>>>,
    /// 超时阈值（秒）
    timeout_threshold_secs: u64,
}

impl TimeoutMonitor {
    pub fn new(timeout_threshold_secs: u64) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            timeout_threshold_secs,
        }
    }

    /// 记录服务活跃
    pub fn record_activity(&self, service_name: &str) {
        let mut services = self.services.write();
        services.insert(
            service_name.to_string(),
            ServiceStatus::Active(Instant::now()),
        );
    }

    /// 检查服务是否超时
    pub fn is_timeout(&self, service_name: &str) -> bool {
        let services = self.services.read();
        if let Some(status) = services.get(service_name) {
            if let ServiceStatus::Active(instant) = status {
                instant.elapsed().as_secs() > self.timeout_threshold_secs
            } else {
                true
            }
        } else {
            true // 未记录的服务视为超时
        }
    }

    /// 获取超时列表
    pub fn get_timeout_services(&self) -> Vec<String> {
        let services = self.services.read();
        services
            .iter()
            .filter_map(|(name, status)| {
                if let ServiceStatus::Active(instant) = status {
                    if instant.elapsed().as_secs() > self.timeout_threshold_secs {
                        return Some(name.clone());
                    }
                }
                None
            })
            .collect()
    }

    /// 移除服务
    pub fn remove_service(&self, service_name: &str) {
        let mut services = self.services.write();
        services.remove(service_name);
    }
}

impl Clone for TimeoutMonitor {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            timeout_threshold_secs: self.timeout_threshold_secs,
        }
    }
}

/// 服务状态
#[derive(Debug, Clone)]
enum ServiceStatus {
    /// 活跃中
    Active(Instant),
    /// 已停止
    Stopped,
}

// ============================================================================
// 健康检查器
// ============================================================================

/// 健康检查结果
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// 服务名称
    pub service_name: String,
    /// 是否健康
    pub is_healthy: bool,
    /// 延迟（毫秒）
    pub latency_ms: u64,
    /// 错误消息
    pub error: Option<String>,
}

impl HealthCheckResult {
    pub fn healthy(service_name: String, latency_ms: u64) -> Self {
        Self {
            service_name,
            is_healthy: true,
            latency_ms,
            error: None,
        }
    }

    pub fn unhealthy(service_name: String, error: String) -> Self {
        Self {
            service_name,
            is_healthy: false,
            latency_ms: 0,
            error: Some(error),
        }
    }
}

/// 健康检查器
pub struct HealthChecker {
    /// 健康检查间隔（秒）
    check_interval_secs: u64,
    /// 服务超时阈值（秒）
    service_timeout_secs: u64,
}

impl HealthChecker {
    pub fn new(check_interval_secs: u64, service_timeout_secs: u64) -> Self {
        Self {
            check_interval_secs,
            service_timeout_secs,
        }
    }

    /// 检查超时阈值
    pub fn service_timeout_secs(&self) -> u64 {
        self.service_timeout_secs
    }

    /// 检查间隔
    pub fn check_interval_secs(&self) -> u64 {
        self.check_interval_secs
    }

    /// 判断超时严重程度
    pub fn classify_timeout(&self, elapsed_secs: u64) -> TimeoutSeverity {
        let normal = self.service_timeout_secs;
        let severe = normal * 5;

        if elapsed_secs >= severe {
            TimeoutSeverity::Severe
        } else if elapsed_secs >= normal {
            TimeoutSeverity::Mild
        } else {
            TimeoutSeverity::Normal
        }
    }
}

/// 超时严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutSeverity {
    /// 正常
    Normal,
    /// 轻微（告警）
    Mild,
    /// 严重（重启）
    Severe,
}

/// 默认健康检查器
impl Default for HealthChecker {
    fn default() -> Self {
        Self::new(30, 180) // 30秒检查一次，180秒超时
    }
}
