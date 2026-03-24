//! 交易执行流程模块
//!
//! 实现 CheckTables 双通道执行流程。

#![forbid(unsafe_code)]

use rust_decimal::Decimal;

use crate::core::{
    StrategyQuery, StrategyResponse,
    ChannelType, OrderLifecycle, OrderInfo,
    VolatilityTier, RiskState, PositionSide,
};
pub use crate::types::TradingAction;

/// 执行配置
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// 策略查询超时（秒）
    pub strategy_query_timeout_secs: u64,
    /// 锁获取超时（秒）
    pub lock_timeout_secs: u64,
    /// 订单超时（秒）
    pub order_timeout_secs: u64,
    /// 最大重试次数
    pub max_retry_count: u8,
    /// 重试间隔（毫秒）
    pub retry_interval_ms: u64,
    /// 下单间隔时间（毫秒）
    pub order_interval_ms: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            strategy_query_timeout_secs: 2,
            lock_timeout_secs: 1,
            order_timeout_secs: 10,
            max_retry_count: 2,
            retry_interval_ms: 100,
            order_interval_ms: 1000, // 默认 1 秒
        }
    }
}

impl ExecutionConfig {
    pub fn production() -> Self {
        Self::default()
    }

    pub fn backtest() -> Self {
        Self {
            strategy_query_timeout_secs: 5,
            lock_timeout_secs: 1,
            order_timeout_secs: 30,
            max_retry_count: 0,
            retry_interval_ms: 0,
            order_interval_ms: 1000, // 回测也默认 1 秒
        }
    }
}

// ============================================================================
// 订单执行器
// ============================================================================

/// 订单执行器
///
/// 负责订单生命周期管理。
pub struct OrderExecutor {
    config: ExecutionConfig,
}

impl OrderExecutor {
    pub fn new(config: ExecutionConfig) -> Self {
        Self { config }
    }

    /// 创建新订单
    pub fn create_order(
        &self,
        order_id: String,
        symbol: String,
        action: TradingAction,
        quantity: Decimal,
        target_price: Decimal,
        channel_type: ChannelType,
    ) -> OrderInfo {
        OrderInfo::new(
            order_id,
            symbol,
            action,
            quantity,
            target_price,
            channel_type,
        )
    }

    /// 更新订单状态
    pub fn transition(&self, order: &mut OrderInfo, new_state: OrderLifecycle) {
        order.transition(new_state);
    }

    /// 检查订单是否超时
    pub fn is_timeout(&self, order: &OrderInfo, now_ts: i64) -> bool {
        let elapsed = now_ts - order.created_at;
        elapsed as u64 > self.config.order_timeout_secs
    }

    /// 检查是否可以重试
    pub fn can_retry(&self, order: &OrderInfo) -> bool {
        order.retry_count < self.config.max_retry_count
    }

    /// 增加重试次数
    pub fn increment_retry(&self, order: &mut OrderInfo) {
        order.increment_retry();
    }

    /// 获取配置
    pub fn config(&self) -> &ExecutionConfig {
        &self.config
    }

    /// 获取下单间隔时间（毫秒）
    pub fn order_interval_ms(&self) -> u64 {
        self.config.order_interval_ms
    }
}

impl Default for OrderExecutor {
    fn default() -> Self {
        Self::new(ExecutionConfig::default())
    }
}

// ============================================================================
// 交易流程
// ============================================================================

/// 交易流程结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// 是否成功
    pub success: bool,
    /// 订单ID
    pub order_id: Option<String>,
    /// 消息
    pub message: String,
    /// 错误码
    pub error_code: Option<u16>,
}

impl ExecutionResult {
    pub fn success(order_id: String, message: impl Into<String>) -> Self {
        Self {
            success: true,
            order_id: Some(order_id),
            message: message.into(),
            error_code: None,
        }
    }

    pub fn failure(message: impl Into<String>, error_code: u16) -> Self {
        Self {
            success: false,
            order_id: None,
            message: message.into(),
            error_code: Some(error_code),
        }
    }
}

/// 交易流程执行器
pub struct TradingPipeline {
    execution_config: ExecutionConfig,
}

impl TradingPipeline {
    pub fn new(execution_config: ExecutionConfig) -> Self {
        Self { execution_config }
    }

    /// 构建 StrategyQuery
    pub fn build_strategy_query(
        &self,
        symbol: &str,
        account_available: Decimal,
        account_risk_state: RiskState,
        current_price: Decimal,
        volatility_tier: VolatilityTier,
        position_exists: bool,
        position_direction: PositionSide,
        position_qty: Decimal,
        position_entry_price: Decimal,
    ) -> StrategyQuery {
        StrategyQuery::new(
            chrono::Utc::now().timestamp(),
            account_available,
            account_risk_state,
            current_price,
            volatility_tier,
            position_exists,
            position_direction,
            position_qty,
            position_entry_price,
        )
    }

    /// 执行策略（模拟）
    ///
    /// 在实际实现中，这里会调用策略模块。
    pub fn execute_strategy(
        &self,
        _query: &StrategyQuery,
    ) -> StrategyResponse {
        // 实际实现中，这里会调用策略模块获取响应
        // 这里返回默认的无操作响应
        StrategyResponse::no_action("策略未实现")
    }

    /// 风控一次预检（锁外）
    pub fn pre_check(
        &self,
        response: &StrategyResponse,
        account_available: Decimal,
    ) -> bool {
        if !response.should_execute {
            return false;
        }

        // 检查账户是否允许交易
        if account_available <= Decimal::ZERO {
            return false;
        }

        true
    }

    /// 风控二次检查（加锁后）
    ///
    /// 在实际实现中，这里会比对实时数据。
    pub fn lock_check(
        &self,
        response: &StrategyResponse,
        current_price: Decimal,
        _target_price: Decimal,
    ) -> bool {
        if !response.should_execute {
            return false;
        }

        // 价格偏差检查
        if response.target_price > Decimal::ZERO {
            let deviation = (current_price - response.target_price).abs() / current_price;
            if deviation > Decimal::from(5) / Decimal::from(100) {
                // 价格偏差超过5%
                return false;
            }
        }

        true
    }

    /// 获取配置
    pub fn config(&self) -> &ExecutionConfig {
        &self.execution_config
    }

    /// 获取状态同步器
    pub fn state_syncer(&self) -> StateSyncer {
        StateSyncer::new()
    }
}

impl Default for TradingPipeline {
    fn default() -> Self {
        Self::new(ExecutionConfig::default())
    }
}

// ============================================================================
// 状态同步
// ============================================================================

/// 状态同步器
///
/// 在交易锁内同步本地状态与交易所状态。
pub struct StateSyncer;

impl StateSyncer {
    /// 创建新的状态同步器
    pub fn new() -> Self {
        Self
    }

    /// 同步持仓状态
    ///
    /// # 参数
    /// - local_qty: 本地持仓数量
    /// - exchange_qty: 交易所持仓数量
    /// - local_price: 本地持仓均价
    /// - exchange_price: 交易所持仓均价
    ///
    /// # 返回
    /// - `Ok((qty, price))` 同步后的持仓信息
    /// - `Err(String)` 不一致原因
    pub fn sync_position(
        &self,
        local_qty: Decimal,
        exchange_qty: Decimal,
        local_price: Decimal,
        exchange_price: Decimal,
    ) -> Result<(Decimal, Decimal), String> {
        // 数量不一致
        if local_qty != exchange_qty {
            return Err(format!(
                "持仓数量不一致: 本地={}, 交易所={}",
                local_qty, exchange_qty
            ));
        }

        // 价格偏差过大
        if local_price > Decimal::ZERO && exchange_price > Decimal::ZERO {
            let deviation = (local_price - exchange_price).abs() / local_price;
            if deviation > Decimal::from(1) / Decimal::from(100) {
                return Err(format!(
                    "持仓均价偏差过大: 本地={}, 交易所={}",
                    local_price, exchange_price
                ));
            }
        }

        // 强制以交易所为准
        Ok((exchange_qty, exchange_price))
    }

    /// 同步账户状态
    ///
    /// # 返回
    /// - `Ok(available)` 同步后的可用资金
    /// - `Err(String)` 不一致原因
    pub fn sync_account(
        &self,
        local_available: Decimal,
        exchange_available: Decimal,
    ) -> Result<Decimal, String> {
        let diff = (local_available - exchange_available).abs();

        // 差异超过1%认为是异常
        if local_available > Decimal::ZERO {
            let ratio = diff / local_available;
            if ratio > Decimal::from(1) / Decimal::from(100) {
                return Err(format!(
                    "账户可用资金不一致: 本地={}, 交易所={}",
                    local_available, exchange_available
                ));
            }
        }

        // 强制以交易所为准
        Ok(exchange_available)
    }
}

impl Default for StateSyncer {
    fn default() -> Self {
        Self::new()
    }
}
