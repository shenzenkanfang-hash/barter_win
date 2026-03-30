use chrono::{DateTime, Utc};

/// 心跳标记 - 携带心跳序号、延迟追踪和时间信息
///
/// 注意：使用 `tokio::time::Instant` 而非 `std::time::Instant`，
/// 因为前者实现了 `Send + Sync`，允许跨 tokio 任务边界传递。
#[derive(Clone, Debug)]
pub struct HeartbeatToken {
    /// 心跳序号
    pub sequence: u64,
    /// 数据产生时间（用于延迟计算）
    pub data_timestamp: Option<DateTime<Utc>>,
    /// 墙钟时间 (用于展示)
    pub created_at: DateTime<Utc>,
    /// 单调时钟锚点 (用于计算elapsed)
    /// 使用 tokio::time::Instant 保证 Send/Sync 可跨任务边界
    started_at: tokio::time::Instant,
}

impl HeartbeatToken {
    /// 创建新的心跳（无数据时间戳）
    pub(crate) fn new(sequence: u64, started_at: tokio::time::Instant) -> Self {
        Self {
            sequence,
            data_timestamp: None,
            created_at: Utc::now(),
            started_at,
        }
    }

    /// 创建带数据时间戳的心跳（用于延迟追踪）
    pub fn with_data_timestamp(sequence: u64, data_timestamp: DateTime<Utc>) -> Self {
        Self {
            sequence,
            data_timestamp: Some(data_timestamp),
            created_at: Utc::now(),
            started_at: tokio::time::Instant::now(),
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

    /// 计算数据延迟（毫秒）- 数据产生到现在的时间
    pub fn data_latency_ms(&self) -> Option<i64> {
        self.data_timestamp.map(|ts| {
            (Utc::now() - ts).num_milliseconds()
        })
    }

    /// 获取数据延迟（秒）- 方便显示
    pub fn data_latency_secs(&self) -> Option<f64> {
        self.data_timestamp.map(|ts| {
            (Utc::now() - ts).num_milliseconds() as f64 / 1000.0
        })
    }
}

impl std::fmt::Display for HeartbeatToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HB_{}", self.sequence)
    }
}
