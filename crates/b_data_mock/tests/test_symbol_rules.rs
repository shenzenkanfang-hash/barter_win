//! SymbolRuleService 测试 - 交易对规则

use b_data_mock::{SymbolRuleService, ParsedSymbolRules};
use rust_decimal_macros::dec;

#[test]
fn test_get_btcusdt_rules() {
    let service = SymbolRuleService::new();

    let rules = service.get_rules("BTCUSDT").unwrap();

    assert_eq!(rules.symbol, "BTCUSDT");
    assert_eq!(rules.price_precision, 2);
    assert_eq!(rules.quantity_precision, 3);
    assert_eq!(rules.tick_size, dec!(0.01));
    assert_eq!(rules.min_qty, dec!(0.001));
    assert_eq!(rules.leverage, 20);
}

#[test]
fn test_get_ethusdt_rules() {
    let service = SymbolRuleService::new();

    let rules = service.get_rules("ETHUSDT").unwrap();

    assert_eq!(rules.symbol, "ETHUSDT");
    assert_eq!(rules.quantity_precision, 3);
}

#[test]
fn test_get_rules_case_insensitive() {
    let service = SymbolRuleService::new();

    let rules_upper = service.get_rules("BTCUSDT").unwrap();
    let rules_lower = service.get_rules("btcusdt").unwrap();
    let rules_mixed = service.get_rules("BtcUsdt").unwrap();

    assert_eq!(rules_upper.symbol, rules_lower.symbol);
    assert_eq!(rules_upper.symbol, rules_mixed.symbol);
}

#[test]
fn test_get_nonexistent_symbol() {
    let service = SymbolRuleService::new();

    let rules = service.get_rules("NONEXIST");
    assert!(rules.is_none());
}

#[test]
fn test_round_price() {
    let service = SymbolRuleService::new();
    let rules = service.get_rules("BTCUSDT").unwrap();

    // BTC tick_size = 0.01
    let price = dec!(50000.123);
    let rounded = rules.round_price(price);

    assert_eq!(rounded, dec!(50000.12));
}

#[test]
fn test_round_qty() {
    let service = SymbolRuleService::new();
    let rules = service.get_rules("BTCUSDT").unwrap();

    // BTC step_size = 0.001
    let qty = dec!(0.123456);
    let rounded = rules.round_qty(qty);

    assert_eq!(rounded, dec!(0.123));
}

#[test]
fn test_effective_min_qty() {
    let service = SymbolRuleService::new();
    let rules = service.get_rules("BTCUSDT").unwrap();

    let min_qty = rules.effective_min_qty();

    // effective_min_qty = max(min_qty, 0.001)
    assert!(min_qty >= dec!(0.001));
}

#[test]
fn test_validate_order_valid() {
    let service = SymbolRuleService::new();
    let rules = service.get_rules("BTCUSDT").unwrap();

    let valid = rules.validate_order(dec!(50000.0), dec!(0.1));
    assert!(valid);
}

#[test]
fn test_validate_order_below_min_qty() {
    let service = SymbolRuleService::new();
    let rules = service.get_rules("BTCUSDT").unwrap();

    // 数量太小
    let valid = rules.validate_order(dec!(50000.0), dec!(0.0001));
    assert!(!valid);
}

#[test]
fn test_validate_order_negative_price() {
    let service = SymbolRuleService::new();
    let rules = service.get_rules("BTCUSDT").unwrap();

    let valid = rules.validate_order(dec!(-100.0), dec!(0.1));
    assert!(!valid);
}

#[test]
fn test_register_new_rules() {
    let service = SymbolRuleService::new();

    let new_rules = ParsedSymbolRules {
        symbol: "BNBUSDT".to_string(),
        price_precision: 2,
        quantity_precision: 2,
        tick_size: dec!(0.01),
        min_qty: dec!(0.01),
        step_size: dec!(0.01),
        min_notional: dec!(10.0),
        max_notional: dec!(100000.0),
        leverage: 20,
        maker_fee: dec!(0.001),
        taker_fee: dec!(0.001),
        close_min_ratio: dec!(0.01),
        min_value_threshold: dec!(10.0),
        update_ts: 0,
    };

    service.register_rules(new_rules);

    let rules = service.get_rules("BNBUSDT").unwrap();
    assert_eq!(rules.symbol, "BNBUSDT");
    assert_eq!(rules.price_precision, 2);
}
