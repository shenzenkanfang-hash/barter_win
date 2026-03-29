//! 拦截器模块测试
//!
//! 测试 Tick 和 Order 拦截器的功能

use b_data_mock::interceptor::{TickInterceptor, OrderInterceptor};
use b_data_mock::OrderInterceptorConfig;
use b_data_mock::MockApiGateway;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_tick_interceptor_latency() {
    let interceptor = TickInterceptor::new();
    let past = chrono::Utc::now() - chrono::Duration::milliseconds(100);
    let latency = interceptor.calc_latency_ms(past);
    assert!(latency >= 100, "Latency should be >= 100ms, got {}", latency);
}

#[test]
fn test_tick_interceptor_anormal_detection() {
    let interceptor = TickInterceptor::new();
    assert!(interceptor.is_latency_anormal(200, 100));
    assert!(!interceptor.is_latency_anormal(50, 100));
}

#[test]
fn test_order_interceptor_stats() {
    let gateway = MockApiGateway::with_default_config(dec!(10000));
    let interceptor = OrderInterceptor::with_default_config(gateway);

    let stats = interceptor.get_stats();
    assert_eq!(stats.total_orders, 0);
    assert_eq!(stats.successful_orders, 0);
    assert_eq!(stats.failed_orders, 0);
}

#[test]
fn test_order_interceptor_order_execution() {
    let gateway = MockApiGateway::with_default_config(dec!(10000));
    let interceptor = OrderInterceptor::with_default_config(gateway);

    // 下单
    let result = interceptor.place_order("BTCUSDT", b_data_mock::Side::Buy, dec!(0.01), Some(dec!(50000)));

    // 检查结果
    assert!(result.is_ok());
    let order = result.unwrap();
    assert_eq!(order.status, a_common::models::types::OrderStatus::Filled);

    // 检查统计
    let stats = interceptor.get_stats();
    assert_eq!(stats.total_orders, 1);
    assert_eq!(stats.successful_orders, 1);
    assert!(stats.avg_latency_ms >= 0);
}

#[test]
fn test_order_interceptor_custom_config() {
    let config = OrderInterceptorConfig {
        enable_heartbeat: true,
        latency_warning_ms: 50,
        latency_critical_ms: 200,
    };
    let gateway = MockApiGateway::with_default_config(dec!(10000));
    let interceptor = OrderInterceptor::new(gateway, config);

    let stats = interceptor.get_stats();
    assert_eq!(stats.total_orders, 0);
}
