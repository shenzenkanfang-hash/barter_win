//! 订单执行接口
//!
//! 定义订单执行的统一接口。
//! 确保交易所网关封装，其他模块不能直接访问网关内部。

use crate::interfaces::risk::{AccountInfo, OrderRequest, PositionInfo};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// 订单执行结果
#[derive(Debug, Clone)]
pub struct OrderResult {
    pub order_id: String,
    pub status: OrderStatus,
    pub executed_quantity: Decimal,
    pub executed_price: Decimal,
    pub commission: Decimal,
    pub message: String,
    pub reject_reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Submitted,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
}

/// 交易所网关接口
///
/// 封装所有交易所交互逻辑。
///
/// # 封装理由
/// 1. 解耦：引擎不依赖具体交易所实现
/// 2. 测试：可以注入 Mock 网关
/// 3. 切换：可以随时切换实盘/模拟/回测环境
///
/// # 接口契约
/// - 所有方法必须是线程安全的 (Send + Sync)
/// - 返回结果必须包含完整的状态信息
/// - 错误信息必须清晰可追溯
pub trait ExchangeGateway: Send + Sync {
    /// 下单
    fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExecutionError>;

    /// 取消订单
    fn cancel_order(&self, order_id: &str, symbol: &str) -> Result<(), ExecutionError>;

    /// 查询订单状态
    fn query_order(&self, order_id: &str, symbol: &str) -> Result<Option<OrderResult>, ExecutionError>;

    /// 获取账户信息
    fn get_account(&self) -> Result<AccountInfo, ExecutionError>;

    /// 获取持仓
    fn get_position(&self, symbol: &str) -> Result<Option<PositionInfo>, ExecutionError>;

    /// 获取所有持仓
    fn get_all_positions(&self) -> Result<Vec<PositionInfo>, ExecutionError>;
}

/// 执行错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExecutionError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),

    #[error("Position limit exceeded: {0}")]
    PositionLimitExceeded(String),

    #[error("Order rejected: {0}")]
    OrderRejected(String),

    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Gateway error: {0}")]
    Gateway(String),
}

/// 订单簿提供者接口
///
/// 用于获取订单簿数据（深度、流动性等）。
pub trait MarketDepthProvider: Send + Sync {
    /// 获取指定品种的买一/卖一
    fn best_bid_ask(&self, symbol: &str) -> Option<(Decimal, Decimal)>;

    /// 获取流动性（指定价格范围内的挂单总量）
    fn liquidity(&self, symbol: &str, depth: Decimal) -> (Decimal, Decimal);
}
