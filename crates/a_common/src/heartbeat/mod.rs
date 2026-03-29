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

use once_cell::sync::Lazy;
use tokio::sync::Mutex;

static REPORTER: Lazy<Mutex<Option<Reporter>>> = Lazy::new(|| Mutex::new(None));

/// 初始化全局报告器
pub async fn init(config: Config) {
    let mut reporter = REPORTER.lock().await;
    *reporter = Some(Reporter::new(config));
}

/// 获取全局报告器
pub fn global() -> &'static Reporter {
    panic!("HeartbeatReporter not initialized. Call heartbeat::init() first.")
}
