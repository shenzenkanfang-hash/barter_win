#![forbid(unsafe_code)]

// 指标目录（保留）
pub mod real_time;   // 实时价格指标 (Tick触发)
pub mod trend;       // 历史趋势指标 (K线闭合触发)
pub mod position;    // 仓位相关指标

// 注意: signal_generator, market_status_generator, price_control_generator 已迁移到 d_checktable
// 注意: trading_trigger 依赖 day 生成器，暂保留在 c_data_process
