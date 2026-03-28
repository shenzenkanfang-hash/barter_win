//! MarketDataStore 模块
//!
//! 与 b_data_source::store 对齐

pub mod store_trait;
pub mod memory_store;
pub mod history_store;
pub mod volatility;
pub mod store_impl;

// Re-exports
pub use store_trait::{MarketDataStore, OrderBookData, VolatilityData};
pub use store_impl::MarketDataStoreImpl;
