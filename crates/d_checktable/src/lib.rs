//! d_checktable - 检查层
//!
//! 按周期组织的策略检查：高频15分钟、高频1分钟、低频1天
//! 检查层异步并发执行，由引擎层统一调度

#![forbid(unsafe_code)]

pub mod check_table;
pub mod types;
pub mod strategy_service;  // 策略服务统一接口

// 周期策略模块
pub mod h_15m;         // 高频15分钟策略检查
pub mod l_1d;           // 低频1天策略检查
pub mod h_volatility_trader;  // 高波动率自动交易器

pub use check_table::{CheckTable, CheckEntry};
pub use types::{CheckChainContext, CheckSignal, CheckChainResult};

// 策略服务类型
pub use strategy_service::{
    StrategyService, StrategyServiceError, StrategyServiceRegistry,
    StrategyInfo, StrategyHealth, StrategyType, StrategySnapshot,
};
