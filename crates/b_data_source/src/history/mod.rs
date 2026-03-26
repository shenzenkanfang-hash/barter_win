//! History Data Manager - 历史数据管理层
//!
//! 提供三层历史数据管理：内存 -> 磁盘 -> API
//! 支持数据自愈、实时K线更新、并发安全
//!
//! # 核心特性
//! - RingBuffer存储闭合K线，current存储未闭合K线
//! - 内存->磁盘自动同步（差异化：1分钟5秒批量，日线立即）
//! - 数据异常自愈机制
//! - 线程安全设计（RwLock/Arc）
//! - API调用支持指数退避+jitter重试
//! - 多品种并发限制（5并发+队列）
//!
//! # 使用方式
//! ```rust,ignore
//! use b_data_source::history::HistoryDataManager;
//! use b_data_source::history::HistoryDataProvider;
//!
//! let manager = HistoryDataManager::new();
//!
//! // 更新实时K线
//! manager.update_realtime_kline("BTCUSDT", "1m", kline, is_closed).await;
//!
//! // 查询历史数据
//! let response = manager.query_history("BTCUSDT", "1m", end_time, 1000).await;
//! ```

pub mod api;
pub mod manager;
pub mod provider;
pub mod types;

pub use api::{HistoryApiClient, ApiClientConfig};
pub use manager::{HistoryDataManager, MAX_KLINE_ENTRIES};
pub use provider::HistoryDataProvider;
pub use types::{
    DataIssue, DataSource, HistoryError, HistoryRequest, HistoryResponse, KLine, KlineMetadata,
};
