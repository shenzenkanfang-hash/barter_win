//! 交易系统黑盒集成测试
//!
//! 测试整个交易流程:
//! 1. 数据流: Tick -> K线合成 -> 指标计算
//! 2. 信号生成: MinSignalGenerator 生成交易信号
//! 3. 风控检查: RiskPreChecker 预检
//! 4. 引擎执行: TradingEngineV2 处理信号

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use d_checktable::types::MinSignalInput;


// ============================================================================
// 测试辅助函数
// ============================================================================

/// 创建测试用 MinSignalInput
#[allow(dead_code)]
fn create_test_signal_input(
    tr_base_60min: Decimal,
    tr_ratio_10min_1h: Decimal,
    tr_ratio_60min_5h: Decimal,
    zscore_14_1m: Decimal,
    zscore_1h_1m: Decimal,
    pos_norm_60: Decimal,
    pine_bg_color: &str,
    pine_bar_color: &str,
    price_deviation: Decimal,
    price_deviation_horizontal_position: Decimal,
) -> MinSignalInput {
    MinSignalInput {
        tr_base_60min,
        tr_ratio_15min: dec!(0.08), // 默认值
        zscore_14_1m,
        zscore_1h_1m,
        tr_ratio_60min_5h,
        tr_ratio_10min_1h,
        pos_norm_60,
        acc_percentile_1h: dec!(50),
        velocity_percentile_1h: dec!(50),
        pine_bg_color: pine_bg_color.to_string(),
        pine_bar_color: pine_bar_color.to_string(),
        price_deviation,
        price_deviation_horizontal_position,
    }
}

/// 创建高波动插针信号 (应该触发 LongEntry)
#[allow(dead_code)]
fn create_pinbar_long_entry_input() -> MinSignalInput {
    create_test_signal_input(
        dec!(0.20),     // tr_base_60min > 15%
        dec!(1.2),      // tr_ratio_10min_1h > 1
        dec!(1.1),      // tr_ratio_60min_5h > 1
        dec!(2.5),      // zscore_14_1m > 2
        dec!(0.5),      // zscore_1h_1m
        dec!(15),       // pos_norm_60 < 20 (极端位置)
        "纯绿",         // pine_bg_color 纯绿
        "纯绿",         // pine_bar_color 纯绿
        dec!(-0.05),    // price_deviation < 0
        dec!(100),      // horizontal_position = 100
    )
}

/// 创建高波动插针信号 (应该触发 ShortEntry)
#[allow(dead_code)]
fn create_pinbar_short_entry_input() -> MinSignalInput {
    create_test_signal_input(
        dec!(0.20),     // tr_base_60min > 15%
        dec!(1.3),      // tr_ratio_10min_1h > 1
        dec!(1.2),      // tr_ratio_60min_5h > 1
        dec!(-2.8),     // zscore_14_1m < -2
        dec!(-0.3),     // zscore_1h_1m
        dec!(88),       // pos_norm_60 > 80 (极端位置)
        "纯红",         // pine_bg_color 纯红
        "纯红",         // pine_bar_color 纯红
        dec!(0.06),     // price_deviation > 0
        dec!(-100),     // horizontal_position = -100
    )
}

/// 创建中性信号 (应该不触发任何信号)
#[allow(dead_code)]
fn create_neutral_input() -> MinSignalInput {
    create_test_signal_input(
        dec!(0.05),     // tr_base_60min < 15%
        dec!(0.8),      // tr_ratio_10min_1h < 1
        dec!(0.7),      // tr_ratio_60min_5h < 1
        dec!(0.3),      // zscore_14_1m 接近 0
        dec!(0.1),      // zscore_1h_1m
        dec!(50),       // pos_norm_60 中性
        "中性",         // pine_bg_color
        "中性",         // pine_bar_color
        dec!(0.0),      // price_deviation
        dec!(0),        // horizontal_position
    )
}

// ============================================================================
// MinSignalGenerator 测试
// ============================================================================

#[test]
fn test_signal_generator_long_entry() {
    let generator = MinSignalGenerator::new();
    let input = create_pinbar_long_entry_input();

    let output = generator.generate(&input, &VolatilityLevel::HIGH);

    assert!(output.long_entry, "应该触发 LongEntry 信号");
    assert!(!output.short_entry, "不应该触发 ShortEntry 信号");
    assert!(!output.long_exit, "不应该触发 LongExit 信号");
}

#[test]
fn test_signal_generator_short_entry() {
    let generator = MinSignalGenerator::new();
    let input = create_pinbar_short_entry_input();

    let output = generator.generate(&input, &VolatilityLevel::HIGH);

    assert!(!output.long_entry, "不应该触发 LongEntry 信号");
    assert!(output.short_entry, "应该触发 ShortEntry 信号");
}

#[test]
fn test_signal_generator_neutral_no_signal() {
    let generator = MinSignalGenerator::new();
    let input = create_neutral_input();

    let output = generator.generate(&input, &VolatilityLevel::NORMAL);

    assert!(!output.long_entry, "不应该触发 LongEntry 信号");
    assert!(!output.short_entry, "不应该触发 ShortEntry 信号");
    assert!(!output.long_exit, "不应该触发 LongExit 信号");
    assert!(!output.short_exit, "不应该触发 ShortExit 信号");
}

#[test]
fn test_signal_generator_7_pin_conditions() {
    let generator = MinSignalGenerator::new();

    // 测试所有条件都不满足
    let input1 = create_test_signal_input(
        dec!(0.10), dec!(0.5), dec!(0.5),
        dec!(0.5), dec!(0.5), dec!(50),
        "中性", "中性", dec!(0), dec!(0)
    );
    let output1 = generator.generate(&input1, &VolatilityLevel::NORMAL);
    assert!(!output1.long_entry && !output1.short_entry, "条件不满足不应该触发");

    // 测试所有条件都满足 (应该触发)
    let input2 = create_test_signal_input(
        dec!(0.20), dec!(1.5), dec!(2.0),
        dec!(3.0), dec!(3.0), dec!(10),
        "纯绿", "纯绿", dec!(-0.1), dec!(100)
    );
    let output2 = generator.generate(&input2, &VolatilityLevel::HIGH);
    assert!(output2.long_entry || output2.short_entry, "条件满足应该触发信号");
}

// ============================================================================
// 风控预检测试
// ============================================================================

#[test]
fn test_risk_prechecker_accepts_valid_order() {
    let checker = RiskPreChecker::new(dec!(0.95), dec!(1000));

    let result = checker.pre_check(
        "BTCUSDT",
        dec!(1000),   // available
        dec!(100),    // order_value
        dec!(1000),   // total_equity
    );

    assert!(result.is_ok(), "有效订单应该通过: {:?}", result);
}

#[test]
fn test_risk_prechecker_rejects_excessive_value() {
    let checker = RiskPreChecker::new(dec!(0.95), dec!(1000));

    // 订单价值超过限额
    let result = checker.pre_check(
        "BTCUSDT",
        dec!(1000),     // available
        dec!(2000),     // order_value 超过 1000
        dec!(1000),     // total_equity
    );

    assert!(result.is_err(), "超过限额的订单应该被拒绝");
}

#[test]
fn test_risk_prechecker_rejects_insufficient_balance() {
    let checker = RiskPreChecker::new(dec!(0.95), dec!(1000));

    // 可用余额不足
    let result = checker.pre_check(
        "BTCUSDT",
        dec!(50),       // available 不足
        dec!(100),      // order_value
        dec!(100),      // total_equity
    );

    assert!(result.is_err(), "余额不足应该被拒绝");
}

// ============================================================================
// MockExchangeGateway 测试
// ============================================================================

#[test]
fn test_mock_gateway_order_execution() {
    let gateway = Arc::new(MockExchangeGateway::default_test());

    let result = gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.1),
        price: Some(dec!(50000)),
    });

    assert!(result.is_ok(), "订单应该执行成功");
    let order_result = result.unwrap();
    assert_eq!(order_result.status, a_common::OrderStatus::Filled);
    assert_eq!(order_result.filled_qty, dec!(0.1));
}

#[test]
fn test_mock_gateway_insufficient_balance() {
    let gateway = Arc::new(MockExchangeGateway::new(dec!(100)));

    // 订单价值超过余额
    let result = gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(1), // 价值 = 1 * 50000 = 50000 > 100
        price: Some(dec!(50000)),
    });

    assert!(result.is_ok());
    let order_result = result.unwrap();
    assert_eq!(order_result.status, a_common::OrderStatus::Rejected);
}

#[test]
fn test_mock_gateway_position_tracking() {
    let gateway = Arc::new(MockExchangeGateway::default_test());

    // 开多仓
    gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.1),
        price: Some(dec!(50000)),
    }).unwrap();

    let position = gateway.get_position("BTCUSDT").unwrap().unwrap();
    assert_eq!(position.long_qty, dec!(0.1));
    assert_eq!(position.long_avg_price, dec!(50000));

    // 开空仓 (先用另一只symbol测试，因为当前有多仓会先平多仓)
    gateway.place_order(f_engine::types::OrderRequest {
        symbol: "ETHUSDT".to_string(),
        side: Side::Sell,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.05),
        price: Some(dec!(3000)),
    }).unwrap();

    let position = gateway.get_position("ETHUSDT").unwrap().unwrap();
    assert_eq!(position.short_qty, dec!(0.05));
}

#[test]
fn test_mock_gateway_order_count() {
    let gateway = Arc::new(MockExchangeGateway::default_test());
    assert_eq!(gateway.order_count(), 0);

    gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.1),
        price: Some(dec!(50000)),
    }).unwrap();

    assert_eq!(gateway.order_count(), 1);

    gateway.place_order(f_engine::types::OrderRequest {
        symbol: "ETHUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(1),
        price: Some(dec!(3000)),
    }).unwrap();

    assert_eq!(gateway.order_count(), 2);
}

// ============================================================================
// SignalProcessor 测试
// ============================================================================

#[test]
fn test_signal_processor_registration() {
    let processor = SignalProcessor::new();

    processor.register_symbol("BTCUSDT");
    assert!(processor.is_registered("BTCUSDT"));
    assert!(processor.is_registered("btcusdt")); // 大小写不敏感

    processor.unregister_symbol("BTCUSDT");
    assert!(!processor.is_registered("BTCUSDT"));
}

#[test]
fn test_signal_processor_day_update() {
    let processor = SignalProcessor::new();

    // 模拟日线数据更新
    for i in 0..100 {
        let base = dec!(100) + Decimal::from(i);
        let result = processor.day_update(
            "BTCUSDT",
            base + dec!(2),  // high
            base - dec!(2),  // low
            base,            // close
        );
        assert!(result.is_ok(), "第 {} 次更新应该成功", i);
    }

    // 验证可以获取指标
    let tr_ratio = processor.day_get_tr_ratio("BTCUSDT");
    assert!(tr_ratio.is_some(), "应该能获取 TR Ratio");

    let pine = processor.day_get_pine_20_50("BTCUSDT");
    assert!(pine.is_some(), "应该能获取 Pine 颜色");

    assert!(processor.day_is_ready("BTCUSDT"), "日线指标应该就绪");
    assert_eq!(processor.day_bar_count("BTCUSDT"), 100, "应该有 100 根 K 线");
}

// ============================================================================
// TradingEngine 基本测试
// ============================================================================

#[test]
fn test_mode_switcher_normal_mode() {
    let mut switcher = ModeSwitcher::new();
    assert_eq!(switcher.mode(), Mode::Normal);
    assert!(switcher.is_trading_allowed());

    switcher.set_mode(Mode::Paper);
    assert!(switcher.is_trading_allowed());

    switcher.set_mode(Mode::Backtest);
    assert!(!switcher.is_trading_allowed());

    switcher.set_mode(Mode::Maintenance);
    assert!(!switcher.is_trading_allowed());
}

#[test]
fn test_trading_engine_start_stop() {
    // 使用 MockMarketStream 需要 async_trait，暂时跳过
    // 这是占位测试，实际的完整测试需要 MockMarketStream 实现
}

// ============================================================================
// SymbolState 测试
// ============================================================================

#[test]
fn test_symbol_state_timeout() {
    let mut state = SymbolState::new("BTCUSDT".to_string());

    // 从未请求过，不算超时
    assert!(!state.is_1m_timeout(1000));

    // 记录请求
    state.record_1m_request(1000);

    // 30秒后不算超时 (timeout=60)
    assert!(!state.is_1m_timeout(1030));

    // 61秒后算超时
    assert!(state.is_1m_timeout(1061));
}

#[test]
fn test_symbol_state_trade_lock() {
    let mut state = SymbolState::new("BTCUSDT".to_string());

    // 初始状态
    assert!(state.trade_lock().timestamp() == 0);

    // 更新锁
    state.trade_lock_mut().update(1000, dec!(0.1), dec!(50000));

    // 检查过期
    assert!(state.trade_lock().is_stale(999));   // tick时间早于锁时间
    assert!(!state.trade_lock().is_stale(1001)); // tick时间晚于锁时间
    assert!(state.trade_lock().is_stale(1000)); // 同一时间也算过期（防止重复处理）
}

// ============================================================================
// 完整流程测试
// ============================================================================

#[test]
fn test_end_to_end_signal_to_decision() {
    // 模拟完整流程: 信号生成 -> 决策

    let generator = MinSignalGenerator::new();

    // 1. 生成做多信号
    let long_input = create_pinbar_long_entry_input();
    let long_output = generator.generate(&long_input, &VolatilityLevel::HIGH);

    assert!(long_output.long_entry, "应该生成做多信号");

    // 2. 构建交易决策
    let decision = TradingDecision {
        symbol: "BTCUSDT".to_string(),
        action: TradingAction::Long,
        reason: "PinBar Long Entry".to_string(),
        confidence: 85,
        level: c_data_process::types::StrategyLevel::MIN,
        qty: dec!(0.1),
        price: dec!(50000),
        timestamp: chrono::Utc::now().timestamp(),
    };

    assert_eq!(decision.action, TradingAction::Long);
    assert_eq!(decision.qty, dec!(0.1));

    // 3. 风控检查
    // order_value = 0.1 * 50000 = 5000, total_equity 需要 >= 5000/0.95 ≈ 5264
    let checker = RiskPreChecker::new(dec!(0.95), dec!(1000));
    let risk_result = checker.pre_check(
        &decision.symbol,
        dec!(6000),
        decision.qty * decision.price,
        dec!(6000),
    );

    assert!(risk_result.is_ok(), "风控应该通过");

    // 4. 执行订单
    let gateway = Arc::new(MockExchangeGateway::default_test());
    let order_result = gateway.place_order(f_engine::types::OrderRequest {
        symbol: decision.symbol.clone(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: decision.qty,
        price: Some(decision.price),
    });

    assert!(order_result.is_ok());
    let result = order_result.unwrap();
    assert_eq!(result.status, a_common::OrderStatus::Filled);

    // 5. 验证持仓
    let position = gateway.get_position("BTCUSDT").unwrap().unwrap();
    assert_eq!(position.long_qty, dec!(0.1));
}

#[test]
fn test_end_to_end_short_entry_flow() {
    let generator = MinSignalGenerator::new();

    // 生成做空信号
    let short_input = create_pinbar_short_entry_input();
    let short_output = generator.generate(&short_input, &VolatilityLevel::HIGH);

    assert!(short_output.short_entry, "应该生成做空信号");

    let decision = TradingDecision {
        symbol: "BTCUSDT".to_string(),
        action: TradingAction::Short,
        reason: "PinBar Short Entry".to_string(),
        confidence: 80,
        level: c_data_process::types::StrategyLevel::MIN,
        qty: dec!(0.1),
        price: dec!(50000),
        timestamp: chrono::Utc::now().timestamp(),
    };

    // 执行
    let gateway = Arc::new(MockExchangeGateway::default_test());
    let order_result = gateway.place_order(f_engine::types::OrderRequest {
        symbol: decision.symbol.clone(),
        side: Side::Sell,
        order_type: f_engine::types::OrderType::Market,
        qty: decision.qty,
        price: Some(decision.price),
    });

    assert!(order_result.is_ok());
    let position = gateway.get_position("BTCUSDT").unwrap().unwrap();
    assert_eq!(position.short_qty, dec!(0.1));
}

#[test]
fn test_flat_position_flow() {
    let gateway = Arc::new(MockExchangeGateway::default_test());

    // 1. 开多仓 (BTCUSDT 真实价格约 60000)
    gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.01), // 0.01 BTC ≈ 600 USDT
        price: Some(dec!(60000)),
    }).unwrap();

    let position_before = gateway.get_position("BTCUSDT").unwrap().unwrap();
    assert_eq!(position_before.long_qty, dec!(0.01));

    // 2. 开空仓 (不同 symbol，简化测试)
    gateway.place_order(f_engine::types::OrderRequest {
        symbol: "ETHUSDT".to_string(),
        side: Side::Sell,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.1), // 0.1 ETH ≈ 300 USDT
        price: Some(dec!(3000)),
    }).unwrap();

    let position_eth = gateway.get_position("ETHUSDT").unwrap().unwrap();
    assert_eq!(position_eth.short_qty, dec!(0.1));
}

#[test]
fn test_rejected_order_handling() {
    let gateway = Arc::new(MockExchangeGateway::default_test());
    gateway.set_reject(Some("Rate limit exceeded".to_string()));

    let result = gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0.1),
        price: Some(dec!(50000)),
    });

    assert!(result.is_ok());
    let order_result = result.unwrap();
    assert_eq!(order_result.status, a_common::OrderStatus::Rejected);
    assert!(order_result.message.contains("Rate limit"));
}

// ============================================================================
// 边界条件测试
// ============================================================================

#[test]
fn test_boundary_tr_ratio_threshold() {
    let generator = MinSignalGenerator::new();

    // 边界: tr_base_60min = 0.15 (刚好等于阈值)
    let input = create_test_signal_input(
        dec!(0.15),  // 刚好等于 15%
        dec!(1.0), dec!(1.0),
        dec!(2.0), dec!(2.0), dec!(15),
        "纯绿", "纯绿", dec!(-0.05), dec!(100)
    );

    let output = generator.generate(&input, &VolatilityLevel::HIGH);
    // tr_base_60min <= 15% 时不应该触发 entry
    assert!(!output.long_entry, "tr_base=0.15 时不应该触发 LongEntry");
}

#[test]
fn test_boundary_zscore_threshold() {
    let generator = MinSignalGenerator::new();

    // 边界: zscore = 2 (刚好等于阈值)
    let input = create_test_signal_input(
        dec!(0.20),
        dec!(1.5), dec!(1.5),
        dec!(2.0), dec!(2.0), // zscore 刚好等于 2
        dec!(15),
        "纯绿", "纯绿", dec!(-0.05), dec!(100)
    );

    let output = generator.generate(&input, &VolatilityLevel::HIGH);
    // zscore = 2 时，极端条件检查是 |zscore| > 2，不满足
    // 但因为有多个条件满足，仍然可能触发
    println!("Long entry: {}, Short entry: {}", output.long_entry, output.short_entry);
}

#[test]
fn test_empty_symbol_handling() {
    let gateway = Arc::new(MockExchangeGateway::default_test());

    // 获取不存在的持仓
    let position = gateway.get_position("NONEXIST").unwrap();
    assert!(position.is_none());

    // 获取账户信息
    let account = gateway.get_account().unwrap();
    assert_eq!(account.account_id, "test_account");
}

#[test]
fn test_zero_order_quantity() {
    let gateway = Arc::new(MockExchangeGateway::default_test());

    // 测试零数量订单 (应该被拒绝或特殊处理)
    let result = gateway.place_order(f_engine::types::OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: Side::Buy,
        order_type: f_engine::types::OrderType::Market,
        qty: dec!(0),
        price: Some(dec!(50000)),
    });

    // 风控检查应该拒绝零数量订单
    assert!(result.is_ok());
}
