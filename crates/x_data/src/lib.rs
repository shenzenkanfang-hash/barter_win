#![forbid(unsafe_code)]

//! x_data - 业务数据抽象层
//!
//! 提供统一的业务数据类型定义，消除跨模块重复定义。
//!
//! # 架构
//! - a_common (纯基础设施) <- x_data (业务数据) <- 业务层
//!
//! # 子模块
//! - position: 持仓数据类型
//! - account: 账户数据类型
//! - market: 市场数据类型
//! - trading: 交易数据类型
//! - state: 状态管理 trait

pub mod position;
pub mod account;
pub mod market;
pub mod trading;
pub mod state;
pub mod error;

// Re-export 常用类型
pub use position::{LocalPosition, PositionDirection, PositionSide, PositionSnapshot, Positions, UnifiedPositionSnapshot};
pub use account::{FundPool, FundPoolManager, AccountSnapshot};
pub use market::{Tick, KLine, KlineData, DepthData, OrderBook, OrderBookLevel, OrderBookSnapshot, SymbolVolatility, VolatilitySummary};
pub use trading::{SymbolRulesData, ParsedSymbolRules, OrderRejectReason, OrderResult, OrderRecord, FuturesPosition, FuturesAccount};
pub use trading::signal::{StrategySignal, TradeCommand, StrategyId, StrategyType, StrategyLevel, PositionRef};
pub use state::{StateViewer, StateManager, UnifiedStateView, SystemSnapshot};
pub use state::{StateCenter, StateCenterImpl};
