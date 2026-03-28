//! DT-011: CheckChainContext 检查链上下文传递测试
//!
//! 测试检查链上下文传递：
//! - CheckChainContext 结构体
//! - CheckSignal 枚举
//! - CheckChainResult 结果处理

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use d_checktable::types::{CheckChainContext, CheckSignal, CheckChainResult};
use x_data::trading::signal::{StrategyId, PositionRef, PositionSide};

#[test]
fn test_check_chain_context_new() {
    let ctx = CheckChainContext {
        current_position_qty: dec!(0.05),
        strategy_id: StrategyId::new_pin_minute("BTCUSDT"),
        position_ref: None,
    };

    assert_eq!(ctx.current_position_qty, dec!(0.05));
}

#[test]
fn test_check_chain_context_with_position_ref() {
    let position_ref = PositionRef {
        position_id: "pos_123".to_string(),
        strategy_instance_id: "BTCUSDT_pin_001".to_string(),
        side: PositionSide::Long,
    };

    let ctx = CheckChainContext {
        current_position_qty: dec!(0.05),
        strategy_id: StrategyId::new_pin_minute("BTCUSDT"),
        position_ref: Some(position_ref),
    };

    assert!(ctx.position_ref.is_some());
    let ref_val = ctx.position_ref.unwrap();
    assert_eq!(ref_val.side, PositionSide::Long);
}

#[test]
fn test_check_signal_variants() {
    // 测试所有 CheckSignal 变体
    let signals = vec![
        CheckSignal::Exit,
        CheckSignal::Close,
        CheckSignal::Hedge,
        CheckSignal::Add,
        CheckSignal::Open,
    ];

    for signal in signals {
        let result = CheckChainResult::new();
        assert!(!result.has(signal));
    }
}

#[test]
fn test_check_chain_result_add_signal() {
    let mut result = CheckChainResult::new();

    result.add_signal(CheckSignal::Open);
    result.add_signal(CheckSignal::Add);

    assert_eq!(result.signals.len(), 2);
    assert!(result.has(CheckSignal::Open));
    assert!(result.has(CheckSignal::Add));
    assert!(!result.has(CheckSignal::Exit));
}

#[test]
fn test_check_chain_result_has() {
    let mut result = CheckChainResult::new();

    result.add_signal(CheckSignal::Close);

    assert!(result.has(CheckSignal::Close));
    assert!(!result.has(CheckSignal::Open));
    assert!(!result.has(CheckSignal::Add));
    assert!(!result.has(CheckSignal::Hedge));
    assert!(!result.has(CheckSignal::Exit));
}

#[test]
fn test_check_chain_result_is_empty() {
    let mut result = CheckChainResult::new();

    assert!(result.is_empty());

    result.add_signal(CheckSignal::Open);
    assert!(!result.is_empty());
}

#[test]
fn test_check_chain_result_multiple_signals() {
    let mut result = CheckChainResult::new();

    result.add_signal(CheckSignal::Open);
    result.add_signal(CheckSignal::Add);
    result.add_signal(CheckSignal::Exit);

    assert_eq!(result.signals.len(), 3);
    assert!(result.has(CheckSignal::Open));
    assert!(result.has(CheckSignal::Add));
    assert!(result.has(CheckSignal::Exit));
}

#[test]
fn test_check_chain_result_duplicate_signals() {
    let mut result = CheckChainResult::new();

    result.add_signal(CheckSignal::Open);
    result.add_signal(CheckSignal::Open); // 重复添加

    // CheckChainResult 使用 Vec::push，允许重复
    assert_eq!(result.signals.len(), 2);
    assert!(result.has(CheckSignal::Open));
}

#[test]
fn test_check_chain_context_default_position() {
    let ctx = CheckChainContext {
        current_position_qty: Decimal::ZERO,
        strategy_id: StrategyId::new_pin_minute("ETHUSDT"),
        position_ref: None,
    };

    assert_eq!(ctx.current_position_qty, Decimal::ZERO);
    assert!(ctx.position_ref.is_none());
}

#[test]
fn test_strategy_id_pin_minute() {
    let id = StrategyId::new_pin_minute("BTCUSDT");

    // StrategyId 有 instance_id 字段
    assert!(id.instance_id.contains("BTCUSDT"));
}

#[test]
fn test_check_chain_result_new_has_no_signals() {
    let result = CheckChainResult::new();

    assert!(result.signals.is_empty());
    assert!(result.is_empty());
}

#[test]
fn test_check_chain_context_all_fields() {
    let position_ref = PositionRef {
        position_id: "pos_456".to_string(),
        strategy_instance_id: "ETHUSDT_pin_002".to_string(),
        side: PositionSide::Short,
    };

    let ctx = CheckChainContext {
        current_position_qty: dec!(0.1),
        strategy_id: StrategyId::new_pin_minute("ETHUSDT"),
        position_ref: Some(position_ref),
    };

    assert_eq!(ctx.current_position_qty, dec!(0.1));
    assert!(ctx.position_ref.is_some());

    let ref_val = ctx.position_ref.unwrap();
    assert_eq!(ref_val.side, PositionSide::Short);
}
