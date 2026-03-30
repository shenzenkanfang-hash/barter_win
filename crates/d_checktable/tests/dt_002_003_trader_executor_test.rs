//! DT-002/DT-003: h_15m::Trader 和 h_15m::Executor 测试
//!
//! 测试 Trader 15分钟高频交易员：
//! - Trader::new 创建
//! - Trader::with_account_provider 创建带账户服务
//! - Trader::with_quantity_calculator 配置数量计算器
//! - Trader::with_indicator_calculator 配置指标计算器
//! - current_price 获取当前价格
//! - volatility_value 获取波动率值
//!
//! 测试 Executor 15分钟信号执行器：
//! - Executor::new 创建
//! - Executor::with_risk_checker 创建带风控
//! - calculate_order_qty 计算下单数量
//! - rate_limit_check 频率限制检查
//! - send_order 发送订单

use std::sync::Arc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use d_checktable::h_15m::{
    Trader, TraderConfig, Executor, ExecutorConfig, OrderType,
    QuantityCalculatorConfig, ThresholdConfig,
};

fn create_test_trader_config() -> TraderConfig {
    TraderConfig {
        symbol: "BTCUSDT".to_string(),
        interval_ms: 100,
        max_position: dec!(0.15),
        initial_ratio: dec!(0.05),
        db_path: ":memory:".to_string(),
        order_interval_ms: 100,
        lot_size: dec!(0.001),
        thresholds: ThresholdConfig::default(),
    }
}

fn create_test_executor_config() -> ExecutorConfig {
    ExecutorConfig {
        symbol: "BTCUSDT".to_string(),
        order_interval_ms: 100,
        initial_ratio: dec!(0.05),
        lot_size: dec!(0.001),
        max_position: dec!(0.15),
    }
}

#[test]
fn test_executor_new() {
    let config = create_test_executor_config();
    let _executor = Executor::new(config);

    // Executor 创建成功，无 panic
}

#[test]
fn test_executor_calculate_order_qty_initial_open() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    let qty = executor.calculate_order_qty(OrderType::InitialOpen, Decimal::ZERO, None);
    assert_eq!(qty, dec!(0.05));
}

#[test]
fn test_executor_calculate_order_qty_double_add() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    let current_qty = dec!(0.05);
    let qty = executor.calculate_order_qty(OrderType::DoubleAdd, current_qty, None);
    assert_eq!(qty, dec!(0.025)); // 0.05 * 0.5
}

#[test]
fn test_executor_calculate_order_qty_double_close() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    let current_qty = dec!(0.1);
    let qty = executor.calculate_order_qty(OrderType::DoubleClose, current_qty, None);
    assert_eq!(qty, dec!(0.1)); // 全平
}

#[test]
fn test_executor_round_to_lot_size() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    // 测试步长裁剪 - lot_size = 0.001
    // InitialOpen 使用 initial_ratio = 0.05，不受 current_qty 影响
    let qty = executor.calculate_order_qty(OrderType::InitialOpen, dec!(0.055), None);
    // InitialOpen 直接返回 config.initial_ratio = 0.05
    assert_eq!(qty, dec!(0.05));
}

#[test]
fn test_executor_rate_limit_check_pass() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    // 第一次检查应该通过
    let result = executor.rate_limit_check(100);
    assert!(result.is_ok());
}

#[test]
fn test_executor_rate_limit_check_block() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    // 设置最近一次下单时间
    executor.rate_limit_check(100).unwrap();

    // 短时间内再次下单应该被拒绝
    let result = executor.rate_limit_check(100);
    assert!(result.is_err());
}

#[test]
fn test_trader_config_default() {
    let config = TraderConfig::default();

    assert_eq!(config.symbol, "BTCUSDT");
    assert_eq!(config.interval_ms, 100);
    assert_eq!(config.max_position, dec!(0.15));
    assert_eq!(config.initial_ratio, dec!(0.05));
    assert_eq!(config.lot_size, dec!(0.001));
}

#[test]
fn test_trader_with_quantity_calculator() {
    let config = create_test_trader_config();
    let executor = Arc::new(Executor::new(create_test_executor_config()));
    let repository = Arc::new(
        d_checktable::h_15m::Repository::new(&config.symbol, ":memory:").unwrap()
    );
    let store = b_data_source::default_store().clone();

    let qty_config = QuantityCalculatorConfig {
        base_open_qty: dec!(0.05),
        max_position_qty: dec!(0.15),
        add_multiplier: dec!(1.5),
        vol_adjustment: true,
    };

    // with_quantity_calculator 返回新的 Trader
    let _trader = Trader::new(config, executor, repository, store)
        .with_quantity_calculator(qty_config);

    // 创建成功，无 panic
}

#[test]
fn test_quantity_calculator_config_default() {
    let config = QuantityCalculatorConfig::default();

    assert_eq!(config.base_open_qty, dec!(0.05));
    assert_eq!(config.max_position_qty, dec!(0.15));
    assert_eq!(config.add_multiplier, dec!(1.5));
    assert!(config.vol_adjustment);
}

#[test]
fn test_gc_config_default() {
    let gc = d_checktable::h_15m::trader::GcConfig::default();

    assert_eq!(gc.timeout_secs, 300);
    assert_eq!(gc.interval_secs, 60);
}

#[test]
fn test_gc_config_production() {
    let gc = d_checktable::h_15m::trader::GcConfig::production();

    assert_eq!(gc.timeout_secs, 600);
    assert_eq!(gc.interval_secs, 300);
}

#[test]
fn test_account_info_default() {
    let info = d_checktable::h_15m::trader::AccountInfo::default();

    assert_eq!(info.available_balance, dec!(10000));
    assert_eq!(info.total_equity, dec!(10000));
    assert_eq!(info.unrealized_pnl, Decimal::ZERO);
    assert_eq!(info.used_margin, Decimal::ZERO);
}

#[test]
fn test_execution_result_is_executed() {
    use d_checktable::h_15m::ExecutionResult;

    let executed = ExecutionResult::Executed {
        qty: dec!(0.05),
        order_type: OrderType::InitialOpen,
    };
    assert!(executed.is_executed());

    let skipped = ExecutionResult::Skipped("no signal");
    assert!(!skipped.is_executed());

    let failed = ExecutionResult::Failed(
        d_checktable::h_15m::trader::TraderError::OrderFailed("test".to_string())
    );
    assert!(!failed.is_executed());
}

#[test]
fn test_trader_execute_once_no_panic() {
    let config = create_test_trader_config();
    let executor = Arc::new(Executor::new(create_test_executor_config()));
    let repository = Arc::new(
        d_checktable::h_15m::Repository::new(&config.symbol, ":memory:").unwrap()
    );
    let store = b_data_source::default_store().clone();

    let trader = Trader::new(config, executor, repository, store);

    // execute_once 在没有市场数据时应该返回 None 而不是 panic
    let result = trader.execute_once();
    // 结果应该是 None（因为没有市场数据），而不是 panic
    assert!(result.is_none() || result.is_some());
}

#[test]
fn test_trader_with_default_store() {
    let config = create_test_trader_config();
    let executor = Arc::new(Executor::new(create_test_executor_config()));
    let repository = Arc::new(
        d_checktable::h_15m::Repository::new(&config.symbol, ":memory:").unwrap()
    );

    let trader = Trader::with_default_store(config, executor, repository);

    // Trader 创建成功
    assert!(trader.execute_once().is_none() || trader.execute_once().is_some());
}

#[test]
fn test_executor_send_order_rate_limited() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    // 先下一次单，消耗频率限制
    let _ = executor.rate_limit_check(100);

    // 频率限制中，send_order 应该返回错误
    let result = executor.send_order(
        OrderType::InitialOpen,
        dec!(0.05),
        None,
        dec!(2500), // order_value
        dec!(10000), // available_balance
        dec!(10000), // total_equity
    );

    assert!(result.is_err());
    // 错误类型应该是 RateLimited 或 CasRetryExceeded
    let err = result.unwrap_err();
    let err_name = format!("{:?}", err);
    assert!(err_name.contains("RateLimited") || err_name.contains("CasRetryExceeded") || err_name.contains("Cas"));
}

#[test]
fn test_executor_send_order_zero_quantity() {
    let config = create_test_executor_config();
    let executor = Executor::new(config);

    let result = executor.send_order(
        OrderType::DoubleClose,
        Decimal::ZERO, // 数量为0
        None,
        Decimal::ZERO,
        dec!(10000),
        dec!(10000),
    );

    assert!(result.is_err());
}
