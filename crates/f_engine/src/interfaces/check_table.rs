//! CheckTable 检查层接口
//!
//! 定义 CheckTableProvider 接口，用于获取 CheckTable 实例。

use async_trait::async_trait;
use rust_decimal::Decimal;

// Re-export DTO from a_common
pub use a_common::models::dto::{CheckTableResult, CheckTableConfig};

/// CheckTableProvider 接口
///
/// 提供 CheckTable 实例的工厂接口。
/// 由 d_checktable 模块实现。
#[async_trait]
pub trait CheckTableProvider: Send + Sync {
    /// 获取分钟级 CheckTable
    async fn get_minute_check_table(&self, symbol: &str) -> Option<Box<dyn CheckTable + '_>>;

    /// 获取日线级 CheckTable
    async fn get_daily_check_table(&self, symbol: &str) -> Option<Box<dyn CheckTable + '_>>;
}

/// CheckTable 执行接口
///
/// 定义 CheckTable 的执行行为。
#[async_trait]
pub trait CheckTable: Send + Sync {
    /// 获取配置
    fn config(&self) -> &CheckTableConfig;

    /// 执行检查
    async fn check(&self, price: Decimal, position: Decimal) -> CheckTableResult;
}
