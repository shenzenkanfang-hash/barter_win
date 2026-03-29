use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// 心跳时钟 - 负责生成单调递增的心跳序号
pub struct HeartbeatClock {
    sequence: AtomicU64,
    started_at: Instant,
}

impl HeartbeatClock {
    pub fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            started_at: Instant::now(),
        }
    }

    /// 生成下一个心跳序号
    pub fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// 获取当前序号
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// 获取启动后经过的时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }
}

impl Default for HeartbeatClock {
    fn default() -> Self {
        Self::new()
    }
}
