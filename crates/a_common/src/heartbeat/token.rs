use chrono::{DateTime, Utc};

/// 心跳标记 - 携带心跳序号和时间信息
#[derive(Clone, Debug)]
pub struct HeartbeatToken {
    /// 心跳序号
    pub sequence: u64,
    /// 墙钟时间 (用于展示)
    pub created_at: DateTime<Utc>,
    /// 单调时钟锚点 (用于计算elapsed)
    started_at: std::time::Instant,
}

impl HeartbeatToken {
    pub(crate) fn new(sequence: u64, started_at: std::time::Instant) -> Self {
        Self {
            sequence,
            created_at: Utc::now(),
            started_at,
        }
    }

    /// 获取序号字符串 "HB_1"
    pub fn sequence_str(&self) -> String {
        format!("HB_{}", self.sequence)
    }

    /// 获取自创建以来经过的时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }
}

impl std::fmt::Display for HeartbeatToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HB_{}", self.sequence)
    }
}
