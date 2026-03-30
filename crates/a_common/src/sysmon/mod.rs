//! 系统心跳监控模块
//!
//! 无Redis的本地心跳监控系统。
//! 各模块独立写入心跳到 tmpfs，汇聚中心定期读取并展示。

pub mod config;
pub mod display;
pub mod reader;
pub mod types;
pub mod writer;

pub use config::{default_config, load_from_env};
pub use display::render;
pub use reader::HeartbeatReader;
pub use types::{Config, Heartbeat, ModuleId, ModuleStatus, ModuleSummary, OverviewStats, SystemOverview};
pub use writer::{global, init, HeartbeatWriter};
