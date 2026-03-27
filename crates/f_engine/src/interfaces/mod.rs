//! Interfaces 模块 - 最小化保留
//!
//! 仅保留沙盒测试需要的类型。

#![forbid(unsafe_code)]

pub mod risk;

pub use risk::{RiskChecker, RiskCheckResult};
pub use risk::{ExecutedOrder, PositionInfo, RiskThresholds, RiskWarning};
