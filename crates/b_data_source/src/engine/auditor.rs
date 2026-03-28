//! 审计系统
//!
//! 提供事件审计功能，用于追踪和回放。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 引擎上下文（每次处理都会递增）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineContext {
    /// 事件序列号（单调递增）
    pub sequence: u64,
    /// 事件时间戳
    pub time: DateTime<Utc>,
}

/// 审计标记
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTick<Event> {
    /// 事件
    pub event: Event,
    /// 上下文
    pub context: EngineContext,
}

/// 审计器 trait
pub trait Auditor<AuditKind> {
    /// 快照类型
    type Snapshot;

    /// 生成审计标记
    fn audit(&mut self, output: AuditKind) -> AuditTick<AuditKind>;

    /// 生成状态快照
    fn audit_snapshot(&mut self) -> AuditTick<Self::Snapshot>;
}

/// 引擎输出
#[derive(Debug, Clone)]
pub struct EngineOutput {
    pub sequence: u64,
    pub time: DateTime<Utc>,
}
