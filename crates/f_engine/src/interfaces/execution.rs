//! 订单执行接口
//!
//! 定义订单执行的统一接口。
//! 确保交易所网关封装，其他模块不能直接访问网关内部。

use rust_decimal::Decimal;

// Re-export ExchangeGateway from order module
pub use crate::order::ExchangeGateway;

// Re-export ExecutionError from a_common
pub use a_common::models::dto::ExecutionError;

/// 订单簿提供者接口
pub trait MarketDepthProvider: Send + Sync {
    /// 获取指定品种的买一/卖一
    fn best_bid_ask(&self, symbol: &str) -> Option<(Decimal, Decimal)>;

    /// 获取流动性
    fn liquidity(&self, symbol: &str, depth: Decimal) -> (Decimal, Decimal);
}
