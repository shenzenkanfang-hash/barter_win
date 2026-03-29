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
pub use reporter::{HeartbeatReporter as Reporter, Summary};
pub use token::HeartbeatToken as Token;

use once_cell::sync::OnceCell;

static REPORTER: OnceCell<Reporter> = OnceCell::new();

/// 初始化全局报告器
pub fn init(config: Config) {
    let _ = REPORTER.set(Reporter::new(config));
}

/// 获取全局报告器
pub fn global() -> &'static Reporter {
    REPORTER.get().expect("HeartbeatReporter not initialized. Call heartbeat::init() first.")
}
