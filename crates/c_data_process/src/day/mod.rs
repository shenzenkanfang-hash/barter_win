#![forbid(unsafe_code)]

// 新架构目录
pub mod real_time;   // 实时价格指标 (分钟级触发)
pub mod trend;       // 历史趋势指标 (小时级触发)
pub mod position;    // 仓位相关指标

// 保留旧模块（待迁移）
pub mod market_status_generator;
pub mod signal_generator;
pub mod price_control_generator;
pub mod trend;

pub use market_status_generator::DayMarketStatusGenerator;
pub use signal_generator::DaySignalGenerator;
pub use price_control_generator::DayPriceControlGenerator;
pub use trend::{BigCycleCalculator, BigCycleIndicators, PineColorBig, TRRatioSignal};
