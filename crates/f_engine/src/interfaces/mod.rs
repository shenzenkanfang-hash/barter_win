//! 接口层模块
//!
//! 定义所有跨模块交互的接口，确保模块间通过接口通信而非直接访问。
//!
//! 核心原则：
//! - 禁止模块间直接访问内部数据
//! - 所有跨模块调用必须通过 Trait 接口
//! - 接口只暴露必要的方法，不暴露内部结构
//! - 所有异步接口统一使用 #[async_trait] + Send + Sync

pub mod market_data;
pub mod strategy;
pub mod risk;
pub mod execution;
pub mod check_table;

pub use market_data::*;
pub use strategy::*;
pub use risk::*;
pub use execution::*;
pub use check_table::*;
