//! MarketDataStore 模块
//!
//! 统一数据存储接口，支持 WS 和模拟器共用。
//!
//! # 模块结构
//! - store_trait.rs      - MarketDataStore trait 定义
//! - memory_store.rs     - 实时分区实现
//! - history_store.rs     - 历史分区实现
//! - volatility.rs       - 波动率计算
//! - store_impl.rs       - 默认实现
//! - pipeline_state.rs   - 流水线观测表（v4.0 新增）

pub mod store_trait;
pub mod memory_store;
pub mod history_store;
pub mod volatility;
pub mod store_impl;
pub mod pipeline_state;

// Re-exports
pub use store_trait::{MarketDataStore, OrderBookData, VolatilityData};
pub use store_impl::MarketDataStoreImpl;
pub use pipeline_state::{
    PipelineStage, PipelineEvent, PipelineState, PipelineStateSnapshot,
    VersionTracker, VersionSnapshot, PipelineStore,
};
