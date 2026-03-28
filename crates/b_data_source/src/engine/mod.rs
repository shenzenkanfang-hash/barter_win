//! 引擎核心模块
//!
//! 提供统一的事件处理架构，基于 barter-rs 的 Processor/Auditor 模式。

pub mod processor;
pub mod auditor;
pub mod clock;
pub mod run;

// Re-exports - Clock
pub use clock::{EngineClock, LiveClock, HistoricalClock};

// Re-exports - Engine
pub use processor::{Processor, TickProcessor};
pub use auditor::{Auditor, EngineContext, AuditTick, EngineOutput};
