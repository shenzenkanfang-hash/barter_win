//! f_engine 核心模块测试
//!
//! 覆盖业务数据类型、触发器、执行流程、资金池、风控、监控、回滚。

#![cfg(test)]

use rust_decimal_macros::dec;

// 导入 business_types
use crate::core::business_types::{
    PositionSide, VolatilityTier, RiskState, ChannelType,
    StrategyQuery, StrategyResponse, RiskCheckResult, PriceControlOutput,
    OrderLifecycle, OrderInfo, FundPool, EngineErrorCode,
};

// 导入 triggers
use crate::core::triggers::{
    TriggerConfig, MinuteTrigger, DailyTrigger, TriggerManager,
};

// 导入 execution
use crate::core::execution::{
    ExecutionConfig, OrderExecutor, TradingPipeline, StateSyncer,
};

// 导入 fund_pool
use crate::core::fund_pool::FundPoolManager;

// 导入 risk_manager
use crate::core::risk_manager::{RiskConfig, RiskManager};

// 导入 monitoring
use crate::core::monitoring::{HealthChecker, TimeoutMonitor, TimeoutSeverity};

// 导入 rollback
use crate::core::rollback::{RollbackManager, OrderRollbackHelper};

// 导入 engine_state
use crate::core::engine_state::{EngineMode, EngineStateHandle};

// 导入 TradingAction
pub use crate::types::TradingAction;

#[cfg(test)]
mod business_types_tests {
    use super::*;

    #[test]
    fn test_position_side() {
        assert!(PositionSide::LONG.is_long());
        assert!(!PositionSide::LONG.is_short());
        assert!(!PositionSide::LONG.is_flat());

        assert!(PositionSide::SHORT.is_short());
        assert!(!PositionSide::SHORT.is_long());

        assert!(PositionSide::NONE.is_flat());
        assert!(!PositionSide::NONE.is_long());
    }

    #[test]
    fn test_volatility_tier() {
        assert_eq!(VolatilityTier::from_ratio(dec!(3)), VolatilityTier::Low);
        assert_eq!(VolatilityTier::from_ratio(dec!(7)), VolatilityTier::Medium);
        assert_eq!(VolatilityTier::from_ratio(dec!(15)), VolatilityTier::High);
        assert_eq!(VolatilityTier::from_ratio(dec!(25)), VolatilityTier::Extreme);
    }

    #[test]
    fn test_risk_state() {
        assert!(RiskState::Normal.can_trade());
        assert!(RiskState::Warning.can_trade());
        assert!(!RiskState::Risky.can_trade());
        assert!(!RiskState::Forbidden.can_trade());
    }

    #[test]
    fn test_channel_type() {
        assert!(ChannelType::HighSpeed.is_high_speed());
        assert!(!ChannelType::HighSpeed.is_low_speed());

        assert!(ChannelType::LowSpeed.is_low_speed());
        assert!(!ChannelType::LowSpeed.is_high_speed());
    }

    #[test]
    fn test_strategy_query() {
        let query = StrategyQuery::new(
            1234567890,
            dec!(10000),
            RiskState::Normal,
            dec!(50000),
            VolatilityTier::High,
            true,
            PositionSide::LONG,
            dec!(0.5),
            dec!(49000),
        );

        assert!(query.can_trade());
        assert!(query.has_valid_position());
    }

    #[test]
    fn test_strategy_response_execute() {
        let response = StrategyResponse::execute(
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
            "test",
        );

        assert!(response.should_execute);
        assert_eq!(response.action, TradingAction::Long);
        assert_eq!(response.quantity, dec!(0.1));
    }

    #[test]
    fn test_strategy_response_no_action() {
        let response = StrategyResponse::no_action("no signal");

        assert!(!response.should_execute);
        assert_eq!(response.action, TradingAction::Flat);
    }

    #[test]
    fn test_risk_check_result() {
        let result = RiskCheckResult::new(true, true);
        assert!(result.both_passed());
        assert!(!result.pre_failed());
        assert!(!result.lock_failed());

        let result2 = RiskCheckResult::new(false, false);
        assert!(!result2.both_passed());
        assert!(result2.pre_failed());

        let result3 = RiskCheckResult::new(true, false);
        assert!(!result3.both_passed());
        assert!(result3.lock_failed());
    }

    #[test]
    fn test_order_lifecycle() {
        let mut state = OrderLifecycle::Created;

        state = state.next();
        assert_eq!(state, OrderLifecycle::Sent);

        state = state.next();
        assert_eq!(state, OrderLifecycle::PartialFilled);

        state = state.next();
        assert_eq!(state, OrderLifecycle::Filled);

        assert!(OrderLifecycle::Filled.is_terminal());
        assert!(!OrderLifecycle::Sent.is_terminal());
        assert!(OrderLifecycle::Sent.is_active());
    }

    #[test]
    fn test_order_info() {
        let mut order = OrderInfo::new(
            "order_123".to_string(),
            "BTC-USDT".to_string(),
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
        );

        assert_eq!(order.lifecycle, OrderLifecycle::Created);
        assert_eq!(order.retry_count, 0);

        order.transition(OrderLifecycle::Sent);
        assert_eq!(order.lifecycle, OrderLifecycle::Sent);

        order.increment_retry();
        assert_eq!(order.retry_count, 1);
    }

    #[test]
    fn test_fund_pool() {
        let mut pool = FundPool::new("test", dec!(10000));

        assert_eq!(pool.available(), dec!(10000));
        assert!(!pool.is_full());

        // 冻结 3000
        assert!(pool.freeze(dec!(3000)));
        assert_eq!(pool.available(), dec!(7000));

        // 确认使用 2000
        pool.confirm_usage(dec!(2000));
        assert_eq!(pool.used, dec!(2000));
        assert_eq!(pool.frozen, dec!(1000));
        assert_eq!(pool.available(), dec!(7000));

        // 释放冻结 1000
        pool.release_frozen(dec!(1000));
        assert_eq!(pool.frozen, dec!(0));

        // 回滚（释放冻结）
        pool.rollback(dec!(500));
        // rollback 释放冻结，所以 frozen 从 0 变成 0（已为0）
        // used 保持不变
        assert_eq!(pool.used, dec!(2000));
    }

    #[test]
    fn test_price_control_output() {
        let output = PriceControlOutput {
            should_add: true,
            should_stop: false,
            should_take_profit: true,
            should_move_stop: false,
            profit_distance_pct: dec!(0.05),
            stop_distance_pct: dec!(0.02),
        };

        assert!(output.any_action());
    }

    #[test]
    fn test_engine_error_code() {
        assert_eq!(EngineErrorCode::SymbolExists.code(), 1001);
        assert_eq!(EngineErrorCode::InsufficientFunds.code(), 2001);
        assert_eq!(EngineErrorCode::RiskRejected.code(), 3001);
        assert_eq!(EngineErrorCode::Timeout.code(), 4001);
        assert_eq!(EngineErrorCode::StateInconsistent.code(), 5001);
    }
}

#[cfg(test)]
mod triggers_tests {
    use super::*;

    #[test]
    fn test_trigger_config_default() {
        let config = TriggerConfig::default();
        assert_eq!(config.minute_volatility_threshold, dec!(13));
        assert_eq!(config.daily_entry_low_threshold, dec!(30));
        assert_eq!(config.daily_entry_high_threshold, dec!(70));
        assert_eq!(config.max_running_symbols, 10);
    }

    #[test]
    fn test_minute_trigger_check() {
        let config = TriggerConfig::default();
        let trigger = MinuteTrigger::new(config);
        let engine_state = EngineStateHandle::new(EngineMode::Simulation);

        // 波动率未超过阈值
        let result = trigger.check("BTC-USDT", dec!(10), &engine_state);
        assert!(!result.precheck_passed);

        // 波动率超过阈值
        let result = trigger.check("BTC-USDT", dec!(15), &engine_state);
        assert!(result.precheck_passed);
    }

    #[test]
    fn test_daily_trigger_check() {
        let config = TriggerConfig::default();
        let trigger = DailyTrigger::new(config);
        let engine_state = EngineStateHandle::new(EngineMode::Simulation);

        // 低位转绿 -> 做多
        let result = trigger.check("BTC-USDT", dec!(25), true, false, &engine_state);
        assert!(result.precheck_passed);

        // 高位转红 -> 做空
        let result = trigger.check("BTC-USDT", dec!(75), false, true, &engine_state);
        assert!(result.precheck_passed);

        // 不满足条件
        let result = trigger.check("BTC-USDT", dec!(50), false, false, &engine_state);
        assert!(!result.precheck_passed);
    }

    #[test]
    fn test_trigger_manager() {
        let manager = TriggerManager::default();
        let config = manager.config();
        assert!(config.minute_volatility_threshold > dec!(0));
        assert!(config.daily_entry_low_threshold > dec!(0));
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;

    #[test]
    fn test_execution_config_default() {
        let config = ExecutionConfig::default();
        assert_eq!(config.strategy_query_timeout_secs, 2);
        assert_eq!(config.lock_timeout_secs, 1);
        assert_eq!(config.order_timeout_secs, 10);
        assert_eq!(config.max_retry_count, 2);
        assert_eq!(config.order_interval_ms, 1000); // 下单间隔 1 秒
    }

    #[test]
    fn test_order_executor_create_order() {
        let executor = OrderExecutor::default();

        let order = executor.create_order(
            "order_001".to_string(),
            "BTC-USDT".to_string(),
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
        );

        assert_eq!(order.order_id, "order_001");
        assert_eq!(order.symbol, "BTC-USDT");
        assert_eq!(order.lifecycle, OrderLifecycle::Created);
    }

    #[test]
    fn test_order_executor_transition() {
        let executor = OrderExecutor::default();
        let mut order = executor.create_order(
            "order_001".to_string(),
            "BTC-USDT".to_string(),
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
        );

        executor.transition(&mut order, OrderLifecycle::Sent);
        assert_eq!(order.lifecycle, OrderLifecycle::Sent);

        executor.transition(&mut order, OrderLifecycle::Filled);
        assert_eq!(order.lifecycle, OrderLifecycle::Filled);
    }

    #[test]
    fn test_trading_pipeline_build_query() {
        let pipeline = TradingPipeline::default();

        let query = pipeline.build_strategy_query(
            "BTC-USDT",
            dec!(10000),
            RiskState::Normal,
            dec!(50000),
            VolatilityTier::High,
            true,
            PositionSide::LONG,
            dec!(0.5),
            dec!(49000),
        );

        assert_eq!(query.current_price, dec!(50000));
        assert!(query.has_valid_position());
    }

    #[test]
    fn test_trading_pipeline_pre_check() {
        let pipeline = TradingPipeline::default();

        let response = StrategyResponse::execute(
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
            "test",
        );

        assert!(pipeline.pre_check(&response, dec!(10000)));

        // 账户不可交易
        assert!(!pipeline.pre_check(&response, dec!(0)));
    }

    #[test]
    fn test_trading_pipeline_lock_check() {
        let pipeline = TradingPipeline::default();

        let response = StrategyResponse::execute(
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
            "test",
        );

        // 价格偏差小 -> 通过
        assert!(pipeline.lock_check(&response, dec!(50000), dec!(50000)));

        // 价格偏差大 -> 不通过
        assert!(!pipeline.lock_check(&response, dec!(55000), dec!(50000)));
    }

    #[test]
    fn test_state_syncer_sync_position() {
        let syncer = StateSyncer::new();

        // 一致 -> 通过
        let result = syncer.sync_position(
            dec!(0.5), dec!(0.5),
            dec!(50000), dec!(50000),
        );
        assert!(result.is_ok());

        // 数量不一致 -> 失败
        let result = syncer.sync_position(
            dec!(0.5), dec!(0.6),
            dec!(50000), dec!(50000),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_state_syncer_sync_account() {
        let syncer = StateSyncer::new();

        // 一致 -> 通过
        let result = syncer.sync_account(dec!(10000), dec!(9990));
        assert!(result.is_ok());

        // 差异过大 -> 失败
        let result = syncer.sync_account(dec!(10000), dec!(9000));
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod fund_pool_tests {
    use super::*;

    #[test]
    fn test_fund_pool_manager() {
        let manager = FundPoolManager::new(dec!(10000), dec!(20000));

        // 检查初始状态
        assert_eq!(manager.available(ChannelType::HighSpeed), dec!(10000));
        assert_eq!(manager.available(ChannelType::LowSpeed), dec!(20000));

        // 冻结分钟级资金
        assert!(manager.freeze(ChannelType::HighSpeed, dec!(3000)));
        assert_eq!(manager.available(ChannelType::HighSpeed), dec!(7000));

        // 冻结日线级资金（用于回滚测试）
        assert!(manager.freeze(ChannelType::LowSpeed, dec!(5000)));
        assert_eq!(manager.available(ChannelType::LowSpeed), dec!(15000));

        // 确认使用
        manager.confirm_usage(ChannelType::HighSpeed, dec!(3000));
        assert_eq!(manager.available(ChannelType::HighSpeed), dec!(7000));

        // 回滚（释放冻结的日线级资金）
        manager.rollback(ChannelType::LowSpeed, dec!(5000));
        assert_eq!(manager.available(ChannelType::LowSpeed), dec!(20000));
    }

    #[test]
    fn test_fund_pool_manager_insufficient() {
        let manager = FundPoolManager::new(dec!(1000), dec!(2000));

        // 资金不足
        assert!(!manager.freeze(ChannelType::HighSpeed, dec!(2000)));
    }

    #[test]
    fn test_fund_pool_usage_rate() {
        let manager = FundPoolManager::new(dec!(10000), dec!(20000));

        manager.freeze(ChannelType::HighSpeed, dec!(5000));
        manager.confirm_usage(ChannelType::HighSpeed, dec!(5000));

        let rate = manager.minute_usage_rate();
        assert_eq!(rate, dec!(0.5)); // 50%
    }
}

#[cfg(test)]
mod risk_manager_tests {
    use super::*;

    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();
        assert_eq!(config.pool_usage_threshold, dec!(0.8));
        assert_eq!(config.max_running_symbols, 10);
    }

    #[test]
    fn test_risk_manager_pre_check() {
        let fund_pool = FundPoolManager::new(dec!(10000), dec!(20000));
        let risk_manager = RiskManager::new(RiskConfig::default(), fund_pool);

        let response = StrategyResponse::execute(
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
            "test",
        );

        // 正常情况 -> 通过
        let result = risk_manager.pre_check(&response, true, 5);
        assert!(result.pre_check_passed);

        // 账户不可交易 -> 不通过
        let result = risk_manager.pre_check(&response, false, 5);
        assert!(!result.pre_check_passed);

        // 品种数量超限 -> 不通过
        let result = risk_manager.pre_check(&response, true, 15);
        assert!(!result.pre_check_passed);
    }

    #[test]
    fn test_risk_manager_lock_check() {
        let fund_pool = FundPoolManager::new(dec!(10000), dec!(20000));
        let risk_manager = RiskManager::new(RiskConfig::default(), fund_pool);

        let response = StrategyResponse::execute(
            TradingAction::Long,
            dec!(0.1),
            dec!(50000),
            ChannelType::HighSpeed,
            "test",
        );

        // 正常情况 -> 通过
        let result = risk_manager.lock_check(
            &response,
            dec!(50000),
            dec!(0),
            dec!(10000),
        );
        assert!(result.lock_check_passed);

        // 价格偏差大 -> 不通过
        let result = risk_manager.lock_check(
            &response,
            dec!(55000),
            dec!(0),
            dec!(10000),
        );
        assert!(!result.lock_check_passed);
    }
}

#[cfg(test)]
mod monitoring_tests {
    use super::*;

    #[test]
    fn test_timeout_monitor() {
        let monitor = TimeoutMonitor::new(10);

        // 记录活跃
        monitor.record_activity("test_service");
        assert!(!monitor.is_timeout("test_service"));

        // 移除服务
        monitor.remove_service("test_service");
        assert!(monitor.is_timeout("test_service"));
    }

    #[test]
    fn test_health_checker() {
        let checker = HealthChecker::new(30, 180);

        assert_eq!(checker.check_interval_secs(), 30);
        assert_eq!(checker.service_timeout_secs(), 180);

        // 正常
        assert_eq!(
            checker.classify_timeout(100),
            TimeoutSeverity::Normal
        );

        // 轻微超时
        assert_eq!(
            checker.classify_timeout(200),
            TimeoutSeverity::Mild
        );

        // 严重超时
        assert_eq!(
            checker.classify_timeout(1000),
            TimeoutSeverity::Severe
        );
    }
}

#[cfg(test)]
mod rollback_tests {
    use super::*;

    #[test]
    fn test_rollback_manager_order_failed() {
        let fund_pool = FundPoolManager::new(dec!(10000), dec!(20000));
        fund_pool.freeze(ChannelType::HighSpeed, dec!(5000));

        let manager = RollbackManager::new(fund_pool.clone());

        let result = manager.rollback_order(ChannelType::HighSpeed, dec!(5000));
        assert!(result.success);

        // 验证资金已回滚
        assert_eq!(fund_pool.available(ChannelType::HighSpeed), dec!(10000));
    }

    #[test]
    fn test_rollback_manager_partial_fill() {
        let fund_pool = FundPoolManager::new(dec!(10000), dec!(20000));
        fund_pool.freeze(ChannelType::HighSpeed, dec!(5000));

        let manager = RollbackManager::new(fund_pool.clone());

        // 部分成交：成交 2000，回滚剩余 3000
        let result = manager.rollback_partial_fill(
            ChannelType::HighSpeed,
            dec!(2000),
            dec!(5000),
        );
        assert!(result.success);

        // 验证剩余冻结已回滚
        assert_eq!(fund_pool.available(ChannelType::HighSpeed), dec!(8000));
    }

    #[test]
    fn test_order_rollback_helper() {
        let helper = OrderRollbackHelper::new(
            "order_001".to_string(),
            ChannelType::HighSpeed,
            dec!(0.1),
            dec!(50000),
        );

        assert_eq!(helper.order_id(), "order_001");
        assert_eq!(helper.channel_type(), ChannelType::HighSpeed);
        assert_eq!(helper.order_value(), dec!(5000)); // 0.1 * 50000
    }
}

// ============================================================================
// TradeLock 测试
// ============================================================================

#[cfg(test)]
mod trade_lock_tests {
    use super::*;
    use crate::core::state::TradeLock;

    #[test]
    fn test_trade_lock_try_lock() {
        let mut lock = TradeLock::new();

        // 未锁定状态可以直接获取锁
        assert!(lock.try_lock(1)); // 1s 超时
        assert!(lock.is_locked());

        // 再次获取锁会失败（锁仍有效）
        assert!(!lock.try_lock(1));

        // 释放锁
        lock.unlock();
        assert!(!lock.is_locked());

        // 可以再次获取锁
        assert!(lock.try_lock(1));
    }

    #[test]
    fn test_trade_lock_is_stale() {
        let mut lock = TradeLock::new();

        // 未更新时，任何 tick 都是新的
        assert!(!lock.is_stale(100));

        // 更新锁状态
        lock.update(100, dec!(0.5), dec!(50000));

        // 相同或更早的 tick 被视为过期
        assert!(lock.is_stale(100));
        assert!(lock.is_stale(50));

        // 较新的 tick 不是过期
        assert!(!lock.is_stale(200));
    }

    #[test]
    fn test_trade_lock_position() {
        let mut lock = TradeLock::new();

        // 更新持仓信息
        lock.update(100, dec!(0.5), dec!(50000));

        assert_eq!(lock.position_qty(), dec!(0.5));
        assert_eq!(lock.position_price(), dec!(50000));
        assert_eq!(lock.position_value(), dec!(25000)); // 0.5 * 50000
    }
}
