//! h_volatility_trader - 高波动率自动交易器
//!
//! 币安测试网自动交易策略：
//! - 监控全市场波动率
//! - 每15分钟选择波动率最高的品种
//! - 0.4% 止盈 / 0.1% 追踪止损
//!
//! # 策略逻辑
//! 1. 接收全市场 1m K线
//! 2. 计算每个品种的滚动波动率（20根K线）
//! 3. 选出波动率最高的品种
//! 4. 订阅该品种的 15m K线
//! 5. 入场后设置止盈止损
//! 6. 持仓监控：0.4% TP → 平仓，0.1% 回撤 → 平仓

#![forbid(unsafe_code)]

mod volatility_ranker;
mod simple_executor;

pub use volatility_ranker::{VolatilityRanker, SymbolVolatilityInfo};
pub use simple_executor::{SimpleExecutor, SimpleExecutorConfig, TradeResult};
