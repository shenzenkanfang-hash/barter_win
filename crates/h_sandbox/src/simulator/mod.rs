//! Simulator - 模拟器模块
//!
//! 提供账户模拟和订单执行功能

pub mod account;
pub mod order;
pub mod risk_checker;

pub use account::{Account, Position, Side};
pub use order::{OrderEngine, OrderRequest};
pub use risk_checker::ShadowRiskChecker;
