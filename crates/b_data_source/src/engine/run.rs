//! 运行模块
//!
//! 提供引擎的同步运行模式，用于回测和模拟。

use crate::engine::auditor::{AuditTick, Auditor, EngineOutput};
use crate::engine::clock::{EngineClock, LiveClock};

/// 同步运行器
///
/// 用于同步回测场景，避免 async/await 的复杂性。
pub struct SyncRunner {
    /// 引擎时钟
    clock: LiveClock,
    /// 当前序列号
    sequence: u64,
}

impl SyncRunner {
    /// 创建新的同步运行器
    pub fn new() -> Self {
        Self {
            clock: LiveClock,
            sequence: 0,
        }
    }

    /// 获取引擎时钟
    pub fn clock(&self) -> &LiveClock {
        &self.clock
    }

    /// 获取当前序列号
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// 推进时钟
    pub fn advance(&mut self) -> (u64, chrono::DateTime<chrono::Utc>) {
        self.sequence += 1;
        let time = self.clock.time();
        (self.sequence, time)
    }
}

impl Default for SyncRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl Auditor<EngineOutput> for SyncRunner {
    type Snapshot = EngineOutput;

    fn audit(&mut self, output: EngineOutput) -> AuditTick<EngineOutput> {
        let (sequence, time) = self.advance();
        AuditTick {
            event: output,
            context: crate::engine::auditor::EngineContext { sequence, time },
        }
    }

    fn audit_snapshot(&mut self) -> AuditTick<Self::Snapshot> {
        let output = EngineOutput {
            sequence: self.sequence,
            time: self.clock.time(),
        };
        AuditTick {
            event: output,
            context: crate::engine::auditor::EngineContext {
                sequence: self.sequence,
                time: self.clock.time(),
            },
        }
    }
}
