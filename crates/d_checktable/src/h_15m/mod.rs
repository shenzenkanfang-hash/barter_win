//! h_15m - 高频15分钟策略检查模块
//!
//! 模块入口，导出所有子模块

#![forbid(unsafe_code)]

pub mod executor;
pub mod quantity_calculator;
pub mod repository;
pub mod signal;
pub mod status;
pub mod strategy_service;
pub mod trader;

pub use executor::{Executor, ExecutorConfig, ExecutorError, OrderType};
pub use repository::{PENDING_TIMEOUT_SECS, RepoError, RecordStatus, Repository, TradeRecord};
pub use signal::MinSignalGenerator;
pub use status::{PinStatus, PinStatusMachine};
pub use strategy_service::{H15mStrategyService, H15mStrategyServiceConfig};
// P0-3 修复：导出新增类型
// v3.0: 导出 ThresholdConfig（Python 对齐阈值配置）
#[allow(deprecated)]
pub use trader::{
    AccountInfo, AccountProvider, ExecutionResult, QuantityCalculatorConfig, ThresholdConfig, Trader,
    TraderConfig, TraderError, TraderHealth,
};

