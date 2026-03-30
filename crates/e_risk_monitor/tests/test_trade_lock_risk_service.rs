//! Integration tests for TradeLock + RiskService
//!
//! Tests the interaction between TradeLock and RiskService:
//! 1. TradeLock + RiskService PreCheck integration
//! 2. TradeLock blocking concurrent strategy execution
//! 3. Full flow: acquire lock → pre_check → re_check → release lock

use e_risk_monitor::{
    LockError, MockRiskService, PreCheckRequest, PreCheckResult,
    ReCheckRequest, ReCheckResult, RiskLevel, RiskService, RiskSide,
    RiskSnapshot, TradeLock,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use std::time::Duration;

/// Helper: create a standard PreCheckRequest
fn create_pre_check_request(order_id: &str, strategy_id: &str) -> PreCheckRequest {
    PreCheckRequest {
        order_id: order_id.to_string(),
        symbol: "BTCUSDT".to_string(),
        strategy_id: strategy_id.to_string(),
        side: RiskSide::Long,
        quantity: dec!(0.1),
        order_value: dec!(1000),
        available_balance: dec!(10000),
        current_position_qty: dec!(0),
        total_equity: dec!(10000),
    }
}

/// Helper: create a standard ReCheckRequest
fn create_re_check_request(order_id: &str, strategy_id: &str) -> ReCheckRequest {
    ReCheckRequest {
        order_id: order_id.to_string(),
        fill_id: "fill_1".to_string(),
        symbol: "BTCUSDT".to_string(),
        strategy_id: strategy_id.to_string(),
        side: RiskSide::Long,
        filled_qty: dec!(0.1),
        fill_price: dec!(10000),
        fill_value: dec!(1000),
        fill_time: chrono::Utc::now(),
        current_position_qty: dec!(0.1),
        available_balance: dec!(9000),
        total_equity: dec!(10000),
    }
}

// ============================================================================
// Test 1: TradeLock + RiskService PreCheck Integration
// ============================================================================

#[tokio::test]
async fn test_trade_lock_with_risk_service_precheck_pass() {
    let trade_lock = TradeLock::new_arc();
    let risk_service = Arc::new(MockRiskService::new().with_pre_check_pass(true));

    // Strategy 1 acquires lock and passes pre_check
    let guard = trade_lock.acquire("strategy_1").expect("Failed to acquire lock");

    let request = create_pre_check_request("order_1", "strategy_1");
    let result = risk_service.pre_check(request).await.expect("PreCheck failed");

    assert!(result.passed);
    assert_eq!(result.risk_level, RiskLevel::Low);
    assert!(trade_lock.is_held());

    drop(guard);
    assert!(!trade_lock.is_held());
}

#[tokio::test]
async fn test_trade_lock_with_risk_service_precheck_reject() {
    let trade_lock = TradeLock::new_arc();
    let risk_service = Arc::new(MockRiskService::new().with_pre_check_pass(false));

    let guard = trade_lock.acquire("strategy_1").expect("Failed to acquire lock");

    let request = create_pre_check_request("order_1", "strategy_1");
    let result = risk_service.pre_check(request).await.expect("PreCheck failed");

    assert!(!result.passed);
    assert_eq!(result.risk_level, RiskLevel::Rejected);
    assert!(result.reject_reason.is_some());

    // Lock still held even though pre_check rejected
    assert!(trade_lock.is_held());

    drop(guard);
    assert!(!trade_lock.is_held());
}

// ============================================================================
// Test 2: TradeLock Blocks Concurrent Strategy Execution
// ============================================================================

#[tokio::test]
async fn test_trade_lock_blocks_concurrent_strategy() {
    let trade_lock = TradeLock::new_arc();
    let risk_service = Arc::new(MockRiskService::new().with_pre_check_pass(true));

    // Strategy 1 acquires lock
    let guard1 = trade_lock.acquire("strategy_1").expect("Failed to acquire lock");
    assert_eq!(trade_lock.holder(), Some("strategy_1".to_string()));

    // Strategy 2 tries to acquire but should be blocked
    let guard2_result = trade_lock.acquire("strategy_2");
    assert!(guard2_result.is_err());

    if let Err(LockError::AlreadyHeld(holder)) = guard2_result {
        assert_eq!(holder, "strategy_1");
    } else {
        panic!("Expected AlreadyHeld error");
    }

    // Verify risk_service still works with blocked lock
    let request = create_pre_check_request("order_2", "strategy_2");
    let result = risk_service.pre_check(request).await.expect("PreCheck failed");
    assert!(result.passed); // RiskService itself doesn't check lock

    // Strategy 1 releases lock
    drop(guard1);
    assert!(!trade_lock.is_held());

    // Now Strategy 2 can acquire
    let guard2 = trade_lock.acquire("strategy_2").expect("Failed to acquire after release");
    assert_eq!(trade_lock.holder(), Some("strategy_2".to_string()));
    drop(guard2);
}

#[tokio::test]
async fn test_same_strategy_can_reacquire() {
    let trade_lock = TradeLock::new_arc();

    // Strategy 1 acquires lock
    let guard1 = trade_lock.acquire("strategy_1").expect("Failed to acquire lock");
    assert!(trade_lock.is_held());

    // Same strategy can acquire again (re-entrant - no count tracking)
    let guard2 = trade_lock.acquire("strategy_1").expect("Failed to re-acquire");
    assert!(trade_lock.is_held());

    // First guard released - lock is released because we don't track reentry count
    drop(guard1);
    // Lock is no longer held because guard1 dropped it
    // Note: TradeLock does NOT track reentry count, so this is expected
    assert!(!trade_lock.is_held());

    // guard2 is now orphaned (lock already released by guard1)
    drop(guard2);
    assert!(!trade_lock.is_held());
}

// ============================================================================
// Test 3: Full Flow - acquire lock → pre_check → re_check → release lock
// ============================================================================

#[tokio::test]
async fn test_full_flow_lock_precheck_reccheck_release() {
    let trade_lock = TradeLock::new_arc();
    let risk_service = Arc::new(MockRiskService::new()
        .with_pre_check_pass(true)
        .with_re_check_pass(true));

    let strategy_id = "strategy_1";

    // Step 1: Acquire lock
    let guard = trade_lock.acquire(strategy_id).expect("Failed to acquire lock");
    assert_eq!(trade_lock.holder(), Some(strategy_id.to_string()));

    // Step 2: PreCheck
    let pre_request = create_pre_check_request("order_1", strategy_id);
    let pre_result = risk_service.pre_check(pre_request).await.expect("PreCheck failed");
    assert!(pre_result.passed, "PreCheck should pass");

    // Step 3: Simulate order execution and ReCheck
    let re_request = create_re_check_request("order_1", strategy_id);
    let re_result = risk_service.re_check(re_request).await.expect("ReCheck failed");
    assert!(re_result.passed, "ReCheck should pass");
    assert!(!re_result.alert_flagged);

    // Step 4: Release lock
    assert!(trade_lock.is_held());
    drop(guard);
    assert!(!trade_lock.is_held());
}

#[tokio::test]
async fn test_full_flow_with_precheck_rejection() {
    let trade_lock = TradeLock::new_arc();
    let risk_service = Arc::new(MockRiskService::new()
        .with_pre_check_pass(false)
        .with_re_check_pass(true));

    let strategy_id = "strategy_1";

    // Step 1: Acquire lock
    let guard = trade_lock.acquire(strategy_id).expect("Failed to acquire lock");

    // Step 2: PreCheck fails - order rejected before sending
    let pre_request = create_pre_check_request("order_1", strategy_id);
    let pre_result = risk_service.pre_check(pre_request).await.expect("PreCheck failed");
    assert!(!pre_result.passed, "PreCheck should reject");

    // Step 3: No ReCheck needed since pre-check failed
    // Step 4: Release lock
    drop(guard);
    assert!(!trade_lock.is_held());
}

#[tokio::test]
async fn test_full_flow_with_reccheck_warning() {
    let trade_lock = TradeLock::new_arc();
    let risk_service = Arc::new(MockRiskService::new()
        .with_pre_check_pass(true)
        .with_re_check_pass(false)); // ReCheck will generate warning

    let strategy_id = "strategy_1";

    // Acquire lock
    let guard = trade_lock.acquire(strategy_id).expect("Failed to acquire lock");

    // PreCheck passes
    let pre_request = create_pre_check_request("order_1", strategy_id);
    let pre_result = risk_service.pre_check(pre_request).await.expect("PreCheck failed");
    assert!(pre_result.passed);

    // ReCheck generates warning
    let re_request = create_re_check_request("order_1", strategy_id);
    let re_result = risk_service.re_check(re_request).await.expect("ReCheck failed");
    assert!(!re_result.passed, "ReCheck should fail");
    assert!(re_result.alert_flagged, "Should be flagged for alert");
    assert!(!re_result.warnings.is_empty(), "Should have warnings");

    // Lock still released
    drop(guard);
    assert!(!trade_lock.is_held());
}

// ============================================================================
// Test 4: Concurrent multi-strategy scenario
// ============================================================================

#[tokio::test]
async fn test_concurrent_multi_strategy_scenario() {
    let trade_lock = Arc::new(TradeLock::new());

    // Use tokio::sync::RwLock for async compatibility
    let results = Arc::new(tokio::sync::RwLock::new(Vec::new()));

    let mut handles = vec![];

    for i in 1..=3 {
        let lock = trade_lock.clone();
        let res = results.clone();

        let handle = tokio::spawn(async move {
            let strategy_id = format!("strategy_{}", i);

            // Try to acquire lock
            match lock.acquire(&strategy_id) {
                Ok(guard) => {
                    let mut local_results = res.write().await;
                    local_results.push(format!(
                        "Strategy {}: lock acquired",
                        i
                    ));

                    // Simulate some work
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    drop(guard);
                }
                Err(e) => {
                    let mut local_results = res.write().await;
                    local_results.push(format!("Strategy {}: lock blocked - {:?}", i, e));
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task panicked");
    }

    let final_results = results.read().await.clone();

    // Only one strategy should have acquired the lock
    let acquired_count = final_results.iter()
        .filter(|r| r.contains("lock acquired"))
        .count();
    let blocked_count = final_results.iter()
        .filter(|r| r.contains("lock blocked"))
        .count();

    assert_eq!(acquired_count, 1, "Only one strategy should acquire lock");
    assert_eq!(blocked_count, 2, "Two strategies should be blocked");
}

// ============================================================================
// Test 5: RiskService methods - freeze/unfreeze/confirm
// ============================================================================

#[tokio::test]
async fn test_risk_service_freeze_unfreeze() {
    let risk_service = MockRiskService::new();

    // Freeze
    let result = risk_service.freeze("order_1", dec!(1000)).await;
    assert!(result.is_ok());

    // MockRiskService doesn't actually track frozen amounts, so unfreeze returns 0
    // This is expected behavior for the mock
    let unfrozen = risk_service.unfreeze("order_1").await;
    assert!(unfrozen.is_ok());
    assert_eq!(unfrozen.unwrap(), dec!(0));
}

#[tokio::test]
async fn test_risk_service_confirm() {
    let risk_service = MockRiskService::new();

    let result = risk_service.confirm("order_1", dec!(1000)).await;
    assert!(result.is_ok());
}

// ============================================================================
// Test 6: Health check and snapshot
// ============================================================================

#[tokio::test]
async fn test_risk_service_health_check() {
    let risk_service = MockRiskService::new();

    let health = risk_service.health_check().await;
    assert!(health.is_ok());
    assert!(health.unwrap());
}

#[tokio::test]
async fn test_risk_service_snapshot() {
    let risk_service = MockRiskService::new();

    let snapshot = risk_service.snapshot().await;
    assert!(snapshot.is_ok());

    let snap = snapshot.unwrap();
    // Note: RiskSnapshot default service_name is "RiskService"
    assert_eq!(snap.service_name, "RiskService");
    assert!(snap.available);
    assert_eq!(snap.total_pre_checks, 0);
    assert_eq!(snap.total_re_checks, 0);
}

#[tokio::test]
async fn test_risk_service_reset_stats() {
    let risk_service = MockRiskService::new();

    let result = risk_service.reset_stats().await;
    assert!(result.is_ok());
}
