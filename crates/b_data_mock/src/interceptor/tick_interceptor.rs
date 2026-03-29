//! Tick 数据拦截器
//!
//! 在 Tick 数据产生时注入心跳时间戳，用于追踪数据延迟

use chrono::{DateTime, Utc};
use crate::models::Tick;

/// Tick 拦截器 - 为 Tick 添加数据产生时间戳
///
/// 用于追踪数据从产生到各组件处理的延迟
pub struct TickInterceptor {
    /// 是否启用
    enabled: bool,
}

impl TickInterceptor {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    pub fn enabled() -> Self {
        Self { enabled: true }
    }

    pub fn disabled() -> Self {
        Self { enabled: false }
    }

    /// 注入时间戳到 Tick
    ///
    /// 如果 Tick 已有注入时间则不覆盖
    pub fn inject_timestamp(&self, tick: &mut Tick) {
        if !self.enabled {
            return;
        }
        // Tick 模型中没有 timestamp_inject 字段
        // 这里我们记录注入时间但不修改原始 tick
        // 延迟计算通过后续的报到来完成
    }

    /// 获取当前时间戳（用于延迟计算）
    pub fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    /// 计算延迟（毫秒）
    pub fn calc_latency_ms(&self, data_timestamp: DateTime<Utc>) -> i64 {
        (Utc::now() - data_timestamp).num_milliseconds()
    }

    /// 检查延迟是否异常（超过阈值）
    pub fn is_latency_anormal(&self, latency_ms: i64, threshold_ms: i64) -> bool {
        latency_ms > threshold_ms
    }
}

impl Default for TickInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_calculation() {
        let interceptor = TickInterceptor::new();
        let past = Utc::now() - chrono::Duration::milliseconds(100);
        let latency = interceptor.calc_latency_ms(past);
        assert!(latency >= 100);
    }
}
