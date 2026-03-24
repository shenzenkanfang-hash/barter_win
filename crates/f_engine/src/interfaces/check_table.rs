//! CheckTable 检查层接口
//!
//! 定义 CheckTableProvider 接口，用于获取 CheckTable 实例。

use async_trait::async_trait;
use rust_decimal::Decimal;

/// CheckTable 检查结果
#[derive(Debug, Clone)]
pub struct CheckTableResult {
    /// 检查是否通过
    pub passed: bool,
    /// 拒绝原因（如果未通过）
    pub reject_reason: Option<String>,
    /// 检查执行时间（毫秒）
    pub execution_time_ms: u64,
}

/// CheckTable 配置
#[derive(Debug, Clone)]
pub struct CheckTableConfig {
    /// 检查表标识
    pub id: String,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
}

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
