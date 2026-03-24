//! 市场数据接口
//!
//! 定义所有市场数据访问的统一接口。
//! 任何模块需要获取市场数据时，必须通过此接口。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// K线数据（接口契约）
///
/// 注意：这是接口契约，不是具体实现。
/// 任何市场数据源（实时/回放/模拟）都必须返回符合此结构的数据。
#[derive(Debug, Clone)]
pub struct MarketKLine {
    pub symbol: String,
    pub period: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    pub is_closed: bool,
}

/// Tick 数据（接口契约）
#[derive(Debug, Clone)]
pub struct MarketTick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
}

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
pub trait MarketDataProvider: Send + Sync {
    /// 获取下一个 Tick（阻塞等待）
    fn next_tick(&self) -> impl std::future::Future<Output = Option<MarketTick>> + Send;

    /// 获取下一个 K 线（如果已完成）
    fn next_completed_kline(&self) -> impl std::future::Future<Output = Option<MarketTick>> + Send;

    /// 获取当前价格
    fn current_price(&self, symbol: &str) -> Option<Decimal>;

    /// 获取 K 线序列（用于回放/回测）
    fn get_klines(
        &self,
        symbol: &str,
        period: &str,
    ) -> impl std::future::Future<Output = Vec<MarketKLine>> + Send;

    /// 获取品种列表
    fn symbols(&self) -> Vec<String>;
}

/// 波动率信息（接口契约）
#[derive(Debug, Clone)]
pub struct VolatilityInfo {
    pub symbol: String,
    pub level: VolatilityLevel,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolatilityLevel {
    High,
    Normal,
    Low,
}

/// 波动率检测器接口
///
/// 注意：封装 e_risk_monitor::shared::market_status::MarketStatusDetector
/// 其他模块不能直接访问风控模块内部。
pub trait VolatilityDetector: Send + Sync {
    /// 检测当前波动率级别
    fn detect_level(&self, symbol: &str) -> Option<VolatilityInfo>;

    /// 更新波动率（每次 Tick 调用）
    fn update(&self, symbol: &str, kline: &MarketKLine);

    /// 获取历史波动率
    fn history(&self, symbol: &str) -> Vec<VolatilityInfo>;
}

/// 订单簿数据（接口契约）
#[derive(Debug, Clone)]
pub struct OrderBookLevel {
    pub price: Decimal,
    pub qty: Decimal,
}

#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp: DateTime<Utc>,
}

/// 订单簿接口
pub trait OrderBookProvider: Send + Sync {
    /// 获取当前订单簿快照
    fn snapshot(&self, symbol: &str) -> Option<OrderBookSnapshot>;

    /// 获取买卖价差
    fn spread(&self, symbol: &str) -> Option<Decimal>;
}
