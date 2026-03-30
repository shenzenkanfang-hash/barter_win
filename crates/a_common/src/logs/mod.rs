//! 日志模块
//!
//! 包含:
//! - CheckpointLogger: Pipeline 各环节的 checkpoint 日志记录器
//! - Stage/StageResult: Pipeline 环节定义
//! - ComponentHealth/HealthAccumulator/ComponentHealthLogger: 组件健康监控
//! - TradingLogEventType: 交易系统日志事件类型

pub mod checkpoint;
pub mod writer;

pub use checkpoint::{
    CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult,
    TracingCheckpointLogger, ComponentHealth, HealthAccumulator, ComponentHealthLogger,
    TradingLogEventType,
};

pub use writer::{JsonLinesWriter, init_log_dir};
