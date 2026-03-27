//! h_15m - 高频15分钟策略检查模块
//!
//! 模块入口，导出所有子模块

#![forbid(unsafe_code)]

pub mod executor;
pub mod quantity_calculator;
pub mod repository;
pub mod signal;
pub mod status;
pub mod trader;

pub use executor::{Executor, ExecutorConfig, ExecutorError, OrderType};
pub use repository::{PENDING_TIMEOUT_SECS, RepoError, RecordStatus, Repository, TradeRecord};
pub use signal::MinSignalGenerator;
pub use status::{PinStatus, PinStatusMachine};
pub use trader::{Trader, TraderConfig, TraderHealth};

