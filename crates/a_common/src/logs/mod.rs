//! 日志模块
//!
//! 包含:
//! - CheckpointLogger: Pipeline 各环节的 checkpoint 日志记录器
//! - Stage/StageResult: Pipeline 环节定义

pub mod checkpoint;

pub use checkpoint::{
    CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult,
    TracingCheckpointLogger,
};
