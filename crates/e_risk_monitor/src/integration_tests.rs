//! integration_tests.rs - TradeLock + RiskService 集成测试
//!
//! 测试两阶段风控流程的完整集成：
//! - TradeLock + RiskService PreCheck 集成
//! - TradeLock 防止并发策略执行
//! - 完整流程：acquire lock → pre_check → re_check → release lock

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use crate::trade_lock::{TradeLock, LockError};
    use crate::risk_service::{
        MockRiskService, RiskService,
        PreCheckRequest, ReCheckRequest,
        RiskSide, RiskLevel,
    };
    use rust_decimal_macros::dec;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    // ==================== TradeLock + RiskService PreCheck 集成测试 ====================

    #[tokio::test]
    async fn test_trade_lock_with_risk_service_pre_check_pass() {
        // 场景：单个策略获取锁后通过风控预检
        let lock = TradeLock::new_arc();
        let risk_service = Arc::new(MockRiskService::new().with_pre_check_pass(true));

        // 获取锁
        let guard = lock.acquire("strategy_trend").unwrap();
        assert!(lock.is_held());
        assert_eq!(lock.holder(), Some("strategy_trend".to_string()));

        // 执行风控预检
        let request = PreCheckRequest {
            order_id: "order_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "strategy_trend".to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        let result = risk_service.pre_check(request).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Low);

        // 释放锁
        drop(guard);
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_trade_lock_with_risk_service_pre_check_reject() {
        // 场景：获取锁后风控预检被拒绝
        let lock = TradeLock::new_arc();
        let risk_service = Arc::new(MockRiskService::new().with_pre_check_pass(false));

        // 获取锁
        let guard = lock.acquire("strategy_pin").unwrap();

        // 执行风控预检
        let request = PreCheckRequest {
            order_id: "order_2".to_string(),
            symbol: "ETHUSDT".to_string(),
            strategy_id: "strategy_pin".to_string(),
            side: RiskSide::Short,
            quantity: dec!(1.0),
            order_value: dec!(2000),
            available_balance: dec!(1000), // 余额不足
            current_position_qty: dec!(0),
            total_equity: dec!(1000),
        };

        let result = risk_service.pre_check(request).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.risk_level, RiskLevel::Rejected);
        assert!(result.reject_reason.is_some());

        // 释放锁（即使预检失败也要释放）
        drop(guard);
        assert!(!lock.is_held());
    }

    // ==================== TradeLock 防止并发策略执行测试 ====================

    #[tokio::test]
    async fn test_trade_lock_prevents_concurrent_strategy() {
        // 场景：strategy_a 获取锁后，strategy_b 无法获取
        let lock = TradeLock::new_arc();

        // strategy_a 获取锁
        let _guard_a = lock.acquire("strategy_a").unwrap();
        assert!(lock.is_held());
        assert_eq!(lock.holder(), Some("strategy_a".to_string()));

        // strategy_b 尝试获取锁应该失败
        let result_b = lock.acquire("strategy_b");
        assert!(result_b.is_err());
        let err = result_b.unwrap_err();
        assert!(matches!(err, LockError::AlreadyHeld(h) if h == "strategy_a"));

        // strategy_a 释放锁
        drop(_guard_a);
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_trade_lock_same_strategy_reentrant() {
        // 场景：同一策略可以重入（但无重入计数）
        // 注意：TradeLock 允许同一策略多次 acquire，但不跟踪重入次数
        // 每次 acquire 都会增加版本号，每次 drop 都会尝试释放锁
        let lock = TradeLock::new_arc();

        // 第一次获取
        let guard1 = lock.acquire("strategy_trend").unwrap();
        assert_eq!(lock.version(), 1);

        // 同一策略第二次获取（可重入）
        let guard2 = lock.acquire("strategy_trend").unwrap();
        assert_eq!(lock.version(), 2);

        // 锁仍被持有
        assert!(lock.is_held());
        assert_eq!(lock.holder(), Some("strategy_trend".to_string()));

        // 释放内层 guard - 注意：由于没有重入计数，锁会被释放
        drop(guard2);
        // 锁已不再被持有（guard1 也无法保护锁）
        assert!(!lock.is_held());

        // 释放外层 guard - 此时锁已经不归 guard1 持有
        drop(guard1);
        assert!(!lock.is_held());
    }

    // ==================== 完整流程测试：acquire → pre_check → re_check → release ====================

    #[tokio::test]
    async fn test_full_flow_pre_check_pass_re_check_pass() {
        // 完整流程：预检通过 → 成交 → 复核通过
        let lock = TradeLock::new_arc();
        let risk_service = Arc::new(
            MockRiskService::new()
                .with_pre_check_pass(true)
                .with_re_check_pass(true)
        );

        let strategy_id = "strategy_full_flow";

        // Step 1: 获取锁
        let _guard = lock.acquire(strategy_id).unwrap();
        assert!(lock.is_held());

        // Step 2: PreCheck
        let pre_request = PreCheckRequest {
            order_id: "order_full_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: strategy_id.to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        let pre_result = risk_service.pre_check(pre_request).await.unwrap();
        assert!(pre_result.passed);

        // Step 3: 冻结保证金
        risk_service.freeze("order_full_1", dec!(100)).await.unwrap();

        // Step 4: 模拟成交
        sleep(Duration::from_millis(10)).await;

        // Step 5: ReCheck
        let re_request = ReCheckRequest {
            order_id: "order_full_1".to_string(),
            fill_id: "fill_full_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: strategy_id.to_string(),
            side: RiskSide::Long,
            filled_qty: dec!(0.1),
            fill_price: dec!(10000),
            fill_value: dec!(1000),
            fill_time: chrono::Utc::now(),
            current_position_qty: dec!(0.1),
            available_balance: dec!(9900),
            total_equity: dec!(10000),
        };

        let re_result = risk_service.re_check(re_request).await.unwrap();
        assert!(re_result.passed);
        assert!(!re_result.alert_flagged);

        // Step 6: 确认保证金
        risk_service.confirm("order_full_1", dec!(100)).await.unwrap();

        // Step 7: 释放锁
        drop(_guard);
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_full_flow_pre_check_reject() {
        // 完整流程：预检拒绝 → 不下单 → 释放锁
        let lock = TradeLock::new_arc();
        let risk_service = Arc::new(
            MockRiskService::new()
                .with_pre_check_pass(false)
                .with_re_check_pass(true)
        );

        let strategy_id = "strategy_reject";

        // Step 1: 获取锁
        let _guard = lock.acquire(strategy_id).unwrap();

        // Step 2: PreCheck（拒绝）
        let pre_request = PreCheckRequest {
            order_id: "order_reject_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: strategy_id.to_string(),
            side: RiskSide::Long,
            quantity: dec!(100), // 数量过大
            order_value: dec!(1000000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        let pre_result = risk_service.pre_check(pre_request).await.unwrap();
        assert!(!pre_result.passed); // 预检被拒

        // Step 3: 不冻结、不成交，直接释放锁
        drop(_guard);
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_full_flow_pre_check_pass_re_check_reject() {
        // 完整流程：预检通过 → 成交 → 复核拒绝（触发告警）
        let lock = TradeLock::new_arc();
        let risk_service = Arc::new(
            MockRiskService::new()
                .with_pre_check_pass(true)
                .with_re_check_pass(false) // 复核拒绝
        );

        let strategy_id = "strategy_alert";

        // Step 1: 获取锁
        let _guard = lock.acquire(strategy_id).unwrap();

        // Step 2: PreCheck
        let pre_request = PreCheckRequest {
            order_id: "order_alert_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: strategy_id.to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        let pre_result = risk_service.pre_check(pre_request).await.unwrap();
        assert!(pre_result.passed);

        // Step 3: 冻结保证金
        risk_service.freeze("order_alert_1", dec!(100)).await.unwrap();

        // Step 4: 模拟异常成交（可能导致复核拒绝）
        let re_request = ReCheckRequest {
            order_id: "order_alert_1".to_string(),
            fill_id: "fill_alert_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: strategy_id.to_string(),
            side: RiskSide::Long,
            filled_qty: dec!(0.1),
            fill_price: dec!(10000),
            fill_value: dec!(1000),
            fill_time: chrono::Utc::now(),
            current_position_qty: dec!(10), // 异常：持仓突增
            available_balance: dec!(9900),
            total_equity: dec!(10000),
        };

        let re_result = risk_service.re_check(re_request).await.unwrap();
        assert!(!re_result.passed); // 复核被拒
        assert!(re_result.alert_flagged); // 触发告警
        assert!(!re_result.warnings.is_empty());

        // Step 5: 释放锁（需要人工介入处理告警）
        drop(_guard);
        assert!(!lock.is_held());
    }

    // ==================== 并发场景测试 ====================

    #[tokio::test]
    async fn test_concurrent_strategies_only_one_wins() {
        // 场景：多个策略竞争锁，只有一个能获得
        let lock = Arc::new(TradeLock::new());

        let lock1 = lock.clone();
        let lock2 = lock.clone();
        let lock3 = lock.clone();

        // 三个策略同时尝试获取锁
        let (result1, result2, result3) = tokio::join! {
            async {
                lock1.acquire("strategy_1")
            },
            async {
                lock2.acquire("strategy_2")
            },
            async {
                lock3.acquire("strategy_3")
            },
        };

        // 只有一个能获取锁，其他两个会返回 AlreadyHeld 错误
        let winners: Vec<String> = vec![result1, result2, result3]
            .into_iter()
            .filter_map(|r| {
                match r {
                    Ok(guard) => {
                        let id = guard.strategy_id().to_string();
                        drop(guard);
                        Some(id)
                    }
                    Err(_) => None,
                }
            })
            .collect();

        // 只有一个赢家
        assert_eq!(winners.len(), 1);

        // 锁最终应该是空闲的（所有 guard 都已 drop）
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_risk_service_concurrent_requests() {
        // 场景：多个策略同时发起风控请求
        // MockRiskService 默认 pre_check_pass=false，需要显式设置
        let risk_service = Arc::new(MockRiskService::new().with_pre_check_pass(true));

        let request = PreCheckRequest {
            order_id: "order_concurrent".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "strategy_concurrent".to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        // 并发发起 10 个请求
        let mut handles = vec![];
        for i in 0..10 {
            let service = risk_service.clone();
            let req = PreCheckRequest {
                order_id: format!("order_concurrent_{}", i),
                ..request.clone()
            };
            handles.push(tokio::spawn(async move {
                service.pre_check(req).await.unwrap()
            }));
        }

        // 等待所有请求完成
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // 所有请求都应该成功
        assert_eq!(results.len(), 10);
        for result in results {
            assert!(result.passed);
        }
    }

    // ==================== RAII 自动释放测试 ====================

    #[tokio::test]
    async fn test_guard_auto_release_on_scope_exit() {
        // 场景：Guard 在作用域结束时自动释放锁
        let lock = TradeLock::new_arc();

        {
            let _guard = lock.acquire("strategy_auto").unwrap();
            assert!(lock.is_held());
        } // Guard 在这里自动 drop，锁被释放

        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_guard_auto_release_on_error() {
        // 场景：发生错误时 Guard 仍能正确释放锁
        let lock = TradeLock::new_arc();
        let risk_service = Arc::new(MockRiskService::new());

        let guard_result = lock.acquire("strategy_error");
        assert!(guard_result.is_ok());

        // 模拟预检失败
        let request = PreCheckRequest {
            order_id: "order_error".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "strategy_error".to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(0), // 余额为0
            current_position_qty: dec!(0),
            total_equity: dec!(0),
        };

        let result = risk_service.pre_check(request).await.unwrap();
        assert!(!result.passed);

        // 即使预检失败，guard 也会在 drop 时正确释放锁
        drop(guard_result.unwrap());

        assert!(!lock.is_held());
    }

    // ==================== 健康检查和快照测试 ====================

    #[tokio::test]
    async fn test_risk_service_health_check() {
        let risk_service = MockRiskService::new();

        let is_healthy = risk_service.health_check().await.unwrap();
        assert!(is_healthy);
    }

    #[tokio::test]
    async fn test_risk_service_snapshot() {
        let risk_service = MockRiskService::new();

        let snapshot = risk_service.snapshot().await.unwrap();
        // MockRiskService 使用默认 snapshot，默认 service_name 为 "RiskService"
        assert_eq!(snapshot.service_name, "RiskService");
        assert!(snapshot.available);
        assert_eq!(snapshot.total_pre_checks, 0);
        assert_eq!(snapshot.total_re_checks, 0);
    }

    // ==================== TradeLockGuard 方法测试 ====================

    #[tokio::test]
    async fn test_trade_lock_guard_methods() {
        let lock = TradeLock::new_arc();

        let guard = lock.acquire("strategy_test").unwrap();

        // 测试 strategy_id 方法
        assert_eq!(guard.strategy_id(), "strategy_test");

        // 测试 version 方法
        assert_eq!(guard.version(), 1);

        drop(guard);
    }

    // ==================== AsyncTradeLockGuard 测试 ====================

    #[tokio::test]
    async fn test_async_trade_lock_guard() {
        use crate::trade_lock::AsyncTradeLockGuard;

        let lock = TradeLock::new_arc();
        let guard = lock.acquire("strategy_async").unwrap();

        // 转换为异步 guard
        let async_guard = AsyncTradeLockGuard::from_sync(guard);

        // 测试 strategy_id 方法
        assert_eq!(async_guard.strategy_id(), Some("strategy_async"));

        // 锁仍被持有
        assert!(lock.is_held());

        // 显式释放
        async_guard.release();

        // 锁已释放
        assert!(!lock.is_held());
    }
}
