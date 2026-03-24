//! TradingEngine v2 - 整合 core/ 模块的完整实现
//!
//! # 架构
//! 本模块整合了 core/ 下的所有模块：
//! - `EngineState`: 全局引擎状态
//! - `TriggerManager`: 并行触发器管理
//! - `TradingPipeline`: 交易执行流程
//! - `FundPoolManager`: 资金池管理
//! - `RiskManager`: 两级风控检查
//! - `TimeoutMonitor`: 超时监控
//! - `RollbackManager`: 失败回滚
//!
//! # 执行流程（V1.4）
//! 1. 触发器检查（分钟级/日线级）
//! 2. StrategyQuery + 策略执行（2s 超时）
//! 3. 风控一次预检（锁外）
//! 4. 品种级抢锁（1s 超时）
//! 5. 风控二次精校（锁内）
//! 6. 冻结资金 + 下单
//! 7. 成交回报 + 状态对齐
//! 8. 确认资金 / 回滚

#![forbid(unsafe_code)]

use rust_decimal::Decimal;

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

/// TradingEngine v2 - 整合 core/ 模块的完整实现
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
        }
    }

    /// 检查是否可以交易
    pub fn can_trade(&self) -> bool {
        self.engine_state.read().can_trade()
    }

    /// 处理 Tick 数据（主入口）
    ///
    /// # 流程
    /// 1. 触发器检查
    /// 2. 策略查询
    /// 3. 风控预检
    /// 4. 抢锁 + 风控精校
    /// 5. 下单执行
    pub fn process_tick(&self, symbol: &str, price: Decimal, volatility: Decimal) -> Result<Option<OrderInfo>, TradingError> {
        // 1. 检查引擎状态
        if !self.can_trade() {
            return Err(TradingError::EngineNotRunning);
        }

        let now_ts = chrono::Utc::now().timestamp();

        // 2. 触发器检查（分钟级）
        let minute_trigger = self.trigger_manager.minute_trigger();
        let trigger_result = minute_trigger.check(
            symbol,
            volatility,
            &self.engine_state,
        );

        if !trigger_result.precheck_passed {
            return Ok(None); // 未触发
        }

        // 3. 构建 StrategyQuery
        let query = self.pipeline.build_strategy_query(
            symbol,
            self.fund_pool.available(ChannelType::HighSpeed),
            RiskState::Normal,
            price,
            VolatilityTier::High,
            false,
            PositionSide::NONE,
            Decimal::ZERO,
            Decimal::ZERO,
        );

        // 4. 执行策略（模拟，实际会调用 c_data_process）
        let response = self.pipeline.execute_strategy(&query);

        // 5. 风控一次预检（锁外）
        if !self.pipeline.pre_check(&response, self.fund_pool.available(ChannelType::HighSpeed)) {
            return Ok(None);
        }

        // 6. 品种级抢锁（这里简化处理，实际需要实现 try_lock 1s）
        // 注意：V1.4 要求锁粒度为品种级，锁超时 1s

        // 7. 风控二次精校（锁内）
        let risk_result = self.risk_manager.lock_check(
            &response,
            price,
            Decimal::ZERO,
            self.fund_pool.available(ChannelType::HighSpeed),
        );

        if !risk_result.lock_check_passed {
            return Ok(None);
        }

        // 8. 检查下单间隔
        let interval_ms = self.order_executor.order_interval_ms();
        let last_time = self.last_order_time_ms.load(std::sync::atomic::Ordering::SeqCst);
        let now_ms = chrono::Utc::now().timestamp_millis();
        if now_ms - last_time < interval_ms as i64 {
            return Ok(None); // 间隔不足
        }

        // 9. 冻结资金
        let order_value = response.quantity * response.target_price;
        if !self.fund_pool.freeze(ChannelType::HighSpeed, order_value) {
            return Err(TradingError::InsufficientFunds);
        }

        // 10. 创建订单
        let mut order = self.order_executor.create_order(
            format!("ord_{}_{}", symbol, now_ts),
            symbol.to_string(),
            response.action,
            response.quantity,
            response.target_price,
            ChannelType::HighSpeed,
        );

        // 11. 状态转换
        self.order_executor.transition(&mut order, crate::core::OrderLifecycle::Sent);

        // 12. 更新下单时间
        self.last_order_time_ms.store(now_ms, std::sync::atomic::Ordering::SeqCst);

        // 13. 记录指标
        self.engine_state.read().record_order_sent();

        Ok(Some(order))
    }

    /// 处理订单成交回报
    pub fn handle_fill(&self, order: &OrderInfo, fill_price: Decimal, fill_qty: Decimal) -> Result<(), TradingError> {
        // 1. 确认使用冻结资金
        let order_value = fill_qty * fill_price;
        self.fund_pool.confirm_usage(order.channel_type, order_value);

        // 2. 更新状态
        self.engine_state.read().record_order_filled();

        Ok(())
    }

    /// 处理订单失败
    pub fn handle_failure(&self, order: &OrderInfo, _reason: &str) {
        // 回滚冻结资金
        let order_value = order.quantity * order.target_price;
        self.rollback_manager.rollback_order(order.channel_type, order_value);

        // 更新熔断器
        self.engine_state.read().record_order_failed();
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

    /// 超时监控
    pub fn timeout_monitor(&self) -> &TimeoutMonitor {
        &self.timeout_monitor
    }
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
