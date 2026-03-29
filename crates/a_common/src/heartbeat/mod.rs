pub mod clock;
pub mod config;
pub mod entry;
pub mod macros;
pub mod mode;
pub mod points;
pub mod reporter;
pub mod token;

pub use clock::HeartbeatClock as Clock;
pub use config::ReporterConfig as Config;
pub use entry::ReportEntry as Entry;
pub use crate::heartbeat_with_info;
pub use crate::function_name;
pub use crate::heartbeat;
pub use mode::ReportMode as Mode;
pub use points::{TEST_POINT_NAMES, get_point_name as Points};
pub use reporter::{HeartbeatReporter as Reporter, Summary, HeartbeatReport, StalePoint, PointDetail};
pub use token::HeartbeatToken as Token;

use std::sync::Arc;
use tokio::sync::RwLock;

/// 全局报告器（使用 RwLock 支持重置）
static REPORTER: RwLock<Option<Arc<Reporter>>> = RwLock::const_new(None);

/// 初始化全局报告器（如果已存在则替换）
pub fn init(config: Config) {
    let reporter = Arc::new(Reporter::new(config));
    let mut global = REPORTER.try_write();
    if let Ok(mut guard) = global {
        *guard = Some(reporter);
    }
}

/// 重置全局报告器（用于测试隔离）
pub fn reset() {
    let mut global = REPORTER.try_write();
    if let Ok(mut guard) = global {
        *guard = None;
    }
}

/// 获取全局报告器引用（克隆Arc以确保所有权正确）
pub fn global() -> Arc<Reporter> {
    // 尝试获取，如果不存在则panic
    let guard = REPORTER.try_read();
    if let Ok(guard) = guard {
        if let Some(reporter) = guard.as_ref() {
            return reporter.clone();
        }
    }
    panic!("HeartbeatReporter not initialized. Call heartbeat::init() first.")
}
