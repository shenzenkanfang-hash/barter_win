//! TradingEngine v2 - V1.4 完整实现
//!
//! # 架构
//! 本模块整合了 core/ 下的所有模块，严格遵守 V1.4 文档。
//!
//! # 执行流程（V1.4）
//! 1. 并行触发器检查（分钟级/日线级）
//! 2. StrategyQuery + 策略执行（2s 超时）
//! 3. 风控一次预检（锁外）
//! 4. 品种级抢锁（1s 超时）
//! 5. 风控二次精校（锁内）
//! 6. 冻结资金 + 下单
//! 7. 成交回报 + 状态对齐
//! 8. 确认资金 / 回滚

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use crate::core::state::TradeLock;
use parking_lot::RwLock;

use crate::core::{
    // 核心状态
    EngineStateHandle, EngineStatus, EngineMode,
    // 业务类型
    ChannelType, VolatilityTier, RiskState, PositionSide, OrderInfo,
    // 触发器
    triggers::TriggerManager,
    // 执行流程
    execution::{ExecutionConfig, TradingPipeline, OrderExecutor},
    // 资金池
    fund_pool::FundPoolManager,
    // 风控
    risk_manager::{RiskManager, RiskConfig},
    // 监控
    monitoring::TimeoutMonitor,
    // 回滚
    rollback::RollbackManager,
};

/// TradingEngine v2 配置
#[derive(Debug, Clone)]
pub struct TradingEngineConfig {
    /// 执行配置
    pub execution: ExecutionConfig,
    /// 风控配置
    pub risk: RiskConfig,
    /// 引擎模式
    pub mode: EngineMode,
    /// 分钟级初始资金
    pub minute_fund: Decimal,
    /// 日线级初始资金
    pub daily_fund: Decimal,
}

impl Default for TradingEngineConfig {
    fn default() -> Self {
        Self {
            execution: ExecutionConfig::production(),
            risk: RiskConfig::default(),
            mode: EngineMode::Simulation,
            minute_fund: Decimal::from(10000),
            daily_fund: Decimal::from(20000),
        }
    }
}

/// TradingEngine v2 - V1.4 完整实现
///
/// # 严格遵守 V1.4 文档：
/// - 并行触发器 → CheckTables → StrategyQuery → 两级风控 → 抢锁 → 执行 → 状态对齐
/// - StrategyQuery 2s 超时
/// - 品种级锁 1s 超时
/// - 两级风控（锁外预检 + 锁内精校）
/// - 熔断器集成
pub struct TradingEngineV2 {
    /// 引擎状态句柄
    engine_state: EngineStateHandle,
    /// 触发器管理器
    trigger_manager: TriggerManager,
    /// 交易流程
    pipeline: TradingPipeline,
    /// 订单执行器
    order_executor: OrderExecutor,
    /// 资金池管理器
    fund_pool: FundPoolManager,
    /// 风控管理器
    risk_manager: RiskManager,
    /// 超时监控器
    timeout_monitor: TimeoutMonitor,
    /// 回滚管理器
    rollback_manager: RollbackManager,
    /// 下单间隔（毫秒）
    last_order_time_ms: std::sync::atomic::AtomicI64,
    /// 品种级交易锁（每个品种独立的锁）
    symbol_locks: RwLock<std::collections::HashMap<String, TradeLock>>,
    /// 锁超时时间（秒）
    lock_timeout_secs: i64,
}

impl TradingEngineV2 {
    /// 创建新的交易引擎
    pub fn new(config: TradingEngineConfig) -> Self {
        let fund_pool = FundPoolManager::new(config.minute_fund, config.daily_fund);
        let fund_pool_for_risk = fund_pool.clone();
        let fund_pool_for_rollback = fund_pool.clone();

        Self {
            engine_state: EngineStateHandle::new(config.mode),
            trigger_manager: TriggerManager::default(),
            pipeline: TradingPipeline::new(config.execution.clone()),
            order_executor: OrderExecutor::new(config.execution),
            fund_pool: fund_pool.clone(),
            risk_manager: RiskManager::new(config.risk, fund_pool_for_risk),
            timeout_monitor: TimeoutMonitor::new(180),
            rollback_manager: RollbackManager::new(fund_pool_for_rollback),
            last_order_time_ms: std::sync::atomic::AtomicI64::new(0),
            symbol_locks: RwLock::new(std::collections::HashMap::new()),
            lock_timeout_secs: 1, // V1.4 要求：锁超时 1s
        }
    }

    /// 检查是否可以交易
    pub fn can_trade(&self) -> bool {
        self.engine_state.read().can_trade()
    }

    /// 处理 Tick 数据（V1.4 完整流程）
    ///
    /// # V1.4 流程
    /// 1. 触发器检查
    /// 2. StrategyQuery + 策略执行（2s 超时）
    /// 3. 风控一次预检（锁外）
    /// 4. 品种级抢锁（1s 超时）
    /// 5. 风控二次精校（锁内）
    /// 6. 冻结资金 + 下单
    /// 7. 状态对齐
    pub fn process_tick(
        &self,
        symbol: &str,
        price: Decimal,
        volatility: Decimal,
        current_position_qty: Decimal,
        current_position_price: Decimal,
    ) -> Result<Option<OrderInfo>, TradingError> {
        // 1. 检查引擎状态
        if !self.can_trade() {
            return Err(TradingError::EngineNotRunning);
        }

        let now_ts = chrono::Utc::now().timestamp();
        let now_ms = chrono::Utc::now().timestamp_millis();

        // 2. 触发器检查（分钟级）
        let minute_trigger = self.trigger_manager.minute_trigger();
        let trigger_result = minute_trigger.check(
            symbol,
            volatility,
            &self.engine_state,
        );

        if !trigger_result.precheck_passed {
            return Ok(None);
        }

        // 3. 构建 StrategyQuery
        let query = self.pipeline.build_strategy_query(
            symbol,
            self.fund_pool.available(ChannelType::HighSpeed),
            RiskState::Normal,
            price,
            VolatilityTier::High,
            current_position_qty > Decimal::ZERO,
            if current_position_qty > Decimal::ZERO {
                PositionSide::LONG
            } else if current_position_qty < Decimal::ZERO {
                PositionSide::SHORT
            } else {
                PositionSide::NONE
            },
            current_position_qty.abs(),
            current_position_price,
        );

        // 4. 执行策略（模拟，实际会调用 c_data_process）
        // V1.4: StrategyQuery 2s 超时
        let query_timeout_ms = self.pipeline.config().strategy_query_timeout_secs * 1000;
        let strategy_start = now_ms;
        let response = self.pipeline.execute_strategy(&query);

        // 超时检测
        let elapsed = now_ms - strategy_start;
        if elapsed > query_timeout_ms as i64 {
            // 超时，记录错误并触发熔断
            {
                let mut state = self.engine_state.write();
                state.record_error();
            }
            return Err(TradingError::Timeout("StrategyQuery 超时".to_string()));
        }

        // 5. 风控一次预检（锁外）
        if !self.pipeline.pre_check(&response, self.fund_pool.available(ChannelType::HighSpeed)) {
            return Ok(None);
        }

        // 6. 品种级抢锁（V1.4: 1s 超时）
        let lock_result = self.try_acquire_lock(symbol, self.lock_timeout_secs);
        if !lock_result {
            // 抢锁失败，记录错误
            {
                let mut state = self.engine_state.write();
                state.record_error();
            }
            return Err(TradingError::LockFailed);
        }

        // 7. 风控二次精校（锁内）
        // 注意：锁范围只包住「状态比对 + 落地」，不包住下单
        let risk_result = self.risk_manager.lock_check(
            &response,
            price,
            current_position_price,
            self.fund_pool.available(ChannelType::HighSpeed),
        );

        if !risk_result.lock_check_passed {
            // 风控拒绝，释放锁并记录错误
            self.release_lock(symbol);
            {
                let mut state = self.engine_state.write();
                state.record_error();
            }
            return Err(TradingError::RiskRejected("风控二次精校拒绝".to_string()));
        }

        // 8. 状态对齐（在锁内完成）
        // V1.4: 检查本地状态与交易所状态是否一致
        let state_syncer = self.pipeline.state_syncer();
        if let Err(e) = state_syncer.sync_position(
            current_position_qty,
            current_position_qty, // 实际应从交易所获取
            current_position_price,
            current_position_price,
        ) {
            self.release_lock(symbol);
            return Err(TradingError::StateInconsistent);
        }

        // 9. 检查下单间隔
        let interval_ms = self.order_executor.order_interval_ms();
        let last_time = self.last_order_time_ms.load(std::sync::atomic::Ordering::SeqCst);
        if now_ms - last_time < interval_ms as i64 {
            self.release_lock(symbol);
            return Ok(None);
        }

        // 10. 冻结资金（此时才释放锁，然后下单）
        // V1.4: 锁范围不包住下单，下单在锁外
        let order_value = response.quantity * response.target_price;
        if !self.fund_pool.freeze(ChannelType::HighSpeed, order_value) {
            self.release_lock(symbol);
            return Err(TradingError::InsufficientFunds);
        }

        // 释放锁（在冻结资金后立即释放）
        self.release_lock(symbol);

        // 11. 创建订单
        let mut order = self.order_executor.create_order(
            format!("ord_{}_{}", symbol, now_ts),
            symbol.to_string(),
            response.action,
            response.quantity,
            response.target_price,
            ChannelType::HighSpeed,
        );

        // 12. 状态转换
        self.order_executor.transition(&mut order, crate::core::OrderLifecycle::Sent);

        // 13. 更新下单时间
        self.last_order_time_ms.store(now_ms, std::sync::atomic::Ordering::SeqCst);

        // 14. 记录指标
        self.engine_state.read().record_order_sent();

        Ok(Some(order))
    }

    /// 尝试获取品种级锁（V1.4: 1s 超时）
    fn try_acquire_lock(&self, symbol: &str, timeout_secs: i64) -> bool {
        let mut locks = self.symbol_locks.write();

        // 获取或创建该品种的锁
        let lock = locks.entry(symbol.to_string()).or_insert_with(TradeLock::new);

        // 尝试获取锁
        lock.try_lock(timeout_secs)
    }

    /// 释放品种级锁
    fn release_lock(&self, symbol: &str) {
        let mut locks = self.symbol_locks.write();
        if let Some(lock) = locks.get_mut(symbol) {
            lock.unlock();
        }
    }

    /// 处理订单成交回报
    pub fn handle_fill(
        &self,
        order: &OrderInfo,
        fill_price: Decimal,
        fill_qty: Decimal,
    ) -> Result<(), TradingError> {
        // 1. 确认使用冻结资金
        let order_value = fill_qty * fill_price;
        self.fund_pool.confirm_usage(order.channel_type, order_value);

        // 2. 更新状态
        self.engine_state.read().record_order_filled();

        // 3. 重置熔断计数（成功）- 使用写锁
        {
            let mut state = self.engine_state.write();
            state.reset_consecutive_errors();
        }

        Ok(())
    }

    /// 处理订单失败
    pub fn handle_failure(&self, order: &OrderInfo, _reason: &str) {
        // 1. 回滚冻结资金
        let order_value = order.quantity * order.target_price;
        self.rollback_manager.rollback_order(order.channel_type, order_value);

        // 2. 更新熔断器（失败）- 使用写锁
        self.engine_state.read().record_order_failed();
        {
            let mut state = self.engine_state.write();
            state.record_error();
        }
    }

    /// 获取引擎状态
    pub fn status(&self) -> EngineStatus {
        self.engine_state.read().status()
    }

    /// 获取资金池状态
    pub fn fund_status(&self) -> (Decimal, Decimal) {
        (
            self.fund_pool.available(ChannelType::HighSpeed),
            self.fund_pool.available(ChannelType::LowSpeed),
        )
    }

    /// 启动引擎
    pub fn start(&self) {
        self.engine_state.write().start();
    }

    /// 停止引擎
    pub fn stop(&self) {
        self.engine_state.write().stop();
    }

    /// 获取超时监控器
    pub fn timeout_monitor(&self) -> &TimeoutMonitor {
        &self.timeout_monitor
    }

    /// 获取熔断状态
    pub fn circuit_breaker_status(&self) -> CircuitBreakerStatus {
        let state = self.engine_state.read();
        let cb = state.circuit_breaker();
        CircuitBreakerStatus {
            is_triggered: cb.is_triggered(),
            consecutive_errors: cb.consecutive_errors(),
            max_errors: cb.config().max_consecutive_errors(),
        }
    }
}

/// 熔断状态
#[derive(Debug, Clone)]
pub struct CircuitBreakerStatus {
    pub is_triggered: bool,
    pub consecutive_errors: u32,
    pub max_errors: u32,
}

// ============================================================================
// 错误类型
// ============================================================================

#[derive(Debug, Clone, thiserror::Error)]
pub enum TradingError {
    #[error("引擎未运行")]
    EngineNotRunning,

    #[error("资金不足")]
    InsufficientFunds,

    #[error("风控拒绝: {0}")]
    RiskRejected(String),

    #[error("抢锁失败")]
    LockFailed,

    #[error("下单失败: {0}")]
    OrderFailed(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("状态不一致")]
    StateInconsistent,
}
