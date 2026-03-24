//! 策略层黑盒测试
//!
//! 完整测试整个交易流程:
//! - 数据层 -> 指标层 -> 信号层 -> 风控层 -> 引擎层

#![forbid(unsafe_code)]

pub mod mock_gateway;
pub mod strategy_executor_test;
pub mod trading_integration_test;

pub use mock_gateway::MockExchangeGateway;
pub use trading_integration_test::*;
