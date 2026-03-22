use a_common::{EngineError, ExchangeAccount, ExchangePosition, OrderResult};
use crate::strategy::types::OrderRequest;

/// 交易所网关 trait
///
/// 定义订单执行的统一接口，支持：
/// - 市价单和限价单执行
/// - 订单状态查询
/// - 账户和持仓查询
///
/// 所有实现必须是线程安全的 (Send + Sync)。
pub trait ExchangeGateway: Send + Sync {
    /// 下单
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError>;

    /// 获取账户信息
    fn get_account(&self) -> Result<ExchangeAccount, EngineError>;

    /// 获取持仓
    fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError>;
}
