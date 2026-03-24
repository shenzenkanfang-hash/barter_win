//! 市场数据接口
//!
//! 定义市场数据访问的统一接口。

use async_trait::async_trait;
use rust_decimal::Decimal;

// Re-export market data types from a_common
pub use a_common::models::market_data::{
    MarketKLine, MarketTick, VolatilityInfo, VolatilityLevel,
    OrderBookLevel, OrderBookSnapshot,
};

/// 市场数据提供者接口
///
/// 封装所有市场数据访问，其他模块不能直接访问 b_data_source 内部。
///
/// 实现者可以是：
/// - 实时市场数据源 (WebSocket)
/// - 历史数据回放源 (CSV/数据库)
/// - 模拟数据源 (Mock)
///
/// # 封装理由
/// 1. 解耦：引擎不依赖具体数据源实现
/// 2. 测试：可以注入 Mock 数据源
/// 3. 替换：可以随时切换数据源而不影响业务逻辑
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    /// 获取下一个 Tick（阻塞等待）
    async fn next_tick(&self) -> Option<MarketTick>;

    /// 获取下一个 K 线（如果已完成）
    async fn next_completed_kline(&self) -> Option<MarketTick>;

    /// 获取当前价格
    fn current_price(&self, symbol: &str) -> Option<Decimal>;

    /// 获取 K 线序列（用于回放/回测）
    async fn get_klines(&self, symbol: &str, period: &str) -> Vec<MarketKLine>;

    /// 获取品种列表
    fn symbols(&self) -> Vec<String>;
}

/// 波动率检测器接口
pub trait VolatilityDetector: Send + Sync {
    /// 检测当前波动率级别
    fn detect_level(&self, symbol: &str) -> Option<VolatilityInfo>;

    /// 更新波动率（每次 Tick 调用）
    fn update(&self, symbol: &str, kline: &MarketKLine);

    /// 获取历史波动率
    fn history(&self, symbol: &str) -> Vec<VolatilityInfo>;
}

/// 订单簿接口
pub trait OrderBookProvider: Send + Sync {
    /// 获取当前订单簿快照
    fn snapshot(&self, symbol: &str) -> Option<OrderBookSnapshot>;

    /// 获取买卖价差
    fn spread(&self, symbol: &str) -> Option<Decimal>;
}
