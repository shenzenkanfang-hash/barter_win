#![forbid(unsafe_code)]

// 新架构目录
pub mod real_time;   // 实时价格指标 (Tick触发)
pub mod trend;       // 历史趋势指标 (K线闭合触发)
pub mod position;    // 仓位相关指标

// 保留旧模块（待迁移）
pub mod market_status_generator;
pub mod price_control_generator;
pub mod signal_generator;

// 最终信号状态机
pub mod trading_trigger;

pub use market_status_generator::MinMarketStatusGenerator;
pub use price_control_generator::MinPriceControlGenerator;
pub use signal_generator::MinSignalGenerator;

pub use trading_trigger::TradingTrigger;
