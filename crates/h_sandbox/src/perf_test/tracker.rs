//! PerformanceTracker - 性能追踪
//!
//! 记录延迟、吞吐量等指标

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use parking_lot::RwLock;

/// 性能统计
#[derive(Debug, Clone)]
pub struct PerfStats {
    /// 总 tick 数
    pub total: u64,
    /// 成功数
    pub success: u64,
    /// 失败数
    pub failed: u64,
    /// 平均延迟（毫秒）
    pub avg_ms: f64,
    /// 中位数延迟（毫秒）
    pub p50_ms: f64,
    /// P95 延迟（毫秒）
    pub p95_ms: f64,
    /// P99 延迟（毫秒）
    pub p99_ms: f64,
    /// 最大延迟（毫秒）
    pub max_ms: f64,
    /// 最小延迟（毫秒）
    pub min_ms: f64,
    /// 吞吐量（ticks/秒）
    pub throughput: f64,
    /// 测试时长（秒）
    pub duration_secs: f64,
}

impl PerfStats {
    /// 平均延迟（毫秒）
    pub fn avg_ms(&self) -> f64 {
        self.avg_ms
    }

    /// P99 延迟（毫秒）
    pub fn p99_ms(&self) -> f64 {
        self.p99_ms
    }

    /// 成功率
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.success as f64 / self.total as f64 * 100.0
        }
    }
}

impl Default for PerfStats {
    fn default() -> Self {
        Self {
            total: 0,
            success: 0,
            failed: 0,
            avg_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            max_ms: 0.0,
            min_ms: 0.0,
            throughput: 0.0,
            duration_secs: 0.0,
        }
    }
}

/// 性能追踪器
pub struct PerformanceTracker {
    /// 延迟记录
    latencies: RwLock<Vec<u64>>,
    /// 成功数
    success: AtomicU64,
    /// 失败数
    failed: AtomicU64,
    /// 开始时间
    start: Instant,
    /// 最后记录时间（用于计算积压）
    last_record_time: RwLock<Instant>,
    /// 最后记录的 tick 数
    last_tick_count: RwLock<u64>,
}

impl PerformanceTracker {
    pub fn new() -> Self {
        Self {
            latencies: RwLock::new(Vec::with_capacity(10000)),
            success: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            start: Instant::now(),
            last_record_time: RwLock::new(Instant::now()),
            last_tick_count: RwLock::new(0),
        }
    }

    /// 记录一次处理
    pub fn record(&self, latency: Duration, success: bool) {
        let latency_ms = latency.as_micros() as u64;

        // 记录延迟
        self.latencies.write().push(latency_ms);

        // 记录成功/失败
        if success {
            self.success.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failed.fetch_add(1, Ordering::SeqCst);
        }

        // 更新最后记录时间
        *self.last_record_time.write() = Instant::now();
    }

    /// 获取当前积压量
    ///
    /// 估算：当前时间 - 最后记录时间 = 积压时间
    /// 积压量 = 积压时间 / 预期间隔（16ms）
    pub fn backlog(&self) -> u64 {
        let elapsed = self.last_record_time.read().elapsed().as_millis() as u64;
        let interval_ms = 16u64;
        elapsed / interval_ms
    }

    /// 获取统计信息
    pub fn stats(&self) -> PerfStats {
        let latencies = self.latencies.read();
        let total = self.success.load(Ordering::SeqCst) + self.failed.load(Ordering::SeqCst);
        let success = self.success.load(Ordering::SeqCst);
        let failed = self.failed.load(Ordering::SeqCst);
        let duration = self.start.elapsed();

        if latencies.is_empty() {
            return PerfStats {
                total,
                success,
                failed,
                avg_ms: 0.0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                max_ms: 0.0,
                min_ms: 0.0,
                throughput: 0.0,
                duration_secs: duration.as_secs_f64(),
            };
        }

        let mut sorted = latencies.clone();
        sorted.sort();

        let sum: u64 = sorted.iter().sum();
        let avg = sum as f64 / sorted.len() as f64;

        let p50 = Self::percentile(&sorted, 50);
        let p95 = Self::percentile(&sorted, 95);
        let p99 = Self::percentile(&sorted, 99);
        let max = *sorted.last().unwrap_or(&0);
        let min = *sorted.first().unwrap_or(&0);

        let throughput = if duration.as_secs() > 0 {
            total as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        PerfStats {
            total,
            success,
            failed,
            avg_ms: avg / 1000.0,
            p50_ms: p50 as f64 / 1000.0,
            p95_ms: p95 as f64 / 1000.0,
            p99_ms: p99 as f64 / 1000.0,
            max_ms: max as f64 / 1000.0,
            min_ms: min as f64 / 1000.0,
            throughput,
            duration_secs: duration.as_secs_f64(),
        }
    }

    /// 计算百分位数
    fn percentile(sorted: &[u64], p: u8) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = (sorted.len() as f64 * p as f64 / 100.0).ceil() as usize;
        let idx = idx.min(sorted.len() - 1);
        sorted[idx]
    }

    /// 重置
    pub fn reset(&self) {
        self.latencies.write().clear();
        self.success.store(0, Ordering::SeqCst);
        self.failed.store(0, Ordering::SeqCst);
        *self.last_record_time.write() = Instant::now();
    }
}

impl Default for PerformanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_tracker() {
        let tracker = PerformanceTracker::new();

        // 记录一些数据
        for i in 0..100 {
            let latency = Duration::from_micros(1000 + i * 100);
            tracker.record(latency, i % 10 != 0);
        }

        let stats = tracker.stats();

        assert_eq!(stats.total, 100);
        assert_eq!(stats.success, 90);
        assert_eq!(stats.failed, 10);
        assert!(stats.avg_ms > 0.0);
        assert!(stats.p99_ms > stats.p95_ms);
        assert!(stats.p95_ms > stats.p50_ms);
    }

    #[test]
    fn test_backlog() {
        let tracker = PerformanceTracker::new();

        // 刚记录，积压应该为 0
        assert_eq!(tracker.backlog(), 0);
    }
}
