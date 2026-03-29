//! x_data 模块功能测试
//!
//! 测试 x_data 业务数据抽象层的核心数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use chrono::Utc;
use std::str::FromStr;

// Helper function to create Decimal from string
fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

// 导入被测试的类型
use x_data::{
    position::{LocalPosition, PositionDirection, PositionSide},
    market::{Tick, KLine, Period},
    trading::{
        SymbolRulesData, ParsedSymbolRules, OrderResult, OrderRejectReason,
        FuturesPosition, FuturesAccount,
    },
    trading::signal::{StrategySignal, TradeCommand, StrategyId, StrategyType, StrategyLevel, PositionRef},
};

// ============================================================================
// 测试 XD-001: LocalPosition 本地持仓数据结构
// ============================================================================

#[test]
fn test_local_position_creation() {
    let position = LocalPosition::new(
        "BTCUSDT".to_string(),
        PositionDirection::Long,
        d("1.5"),
        d("45000.0"),
        "strategy_001".to_string(),
    );

    assert_eq!(position.symbol, "BTCUSDT");
    assert_eq!(position.direction, PositionDirection::Long);
    assert_eq!(position.qty, d("1.5"));
    assert_eq!(position.avg_price, d("45000.0"));
    assert_eq!(position.strategy_instance_id, "strategy_001");
    assert!(!position.position_id.is_empty());
    assert!(position.position_cost == d("0"));
}

#[test]
fn test_local_position_unrealized_pnl() {
    let mut position = LocalPosition::new(
        "BTCUSDT".to_string(),
        PositionDirection::Long,
        d("1.0"),
        d("45000.0"),
        "strategy_001".to_string(),
    );

    // 多头持仓，价格上涨
    let pnl_long = position.unrealized_pnl(d("46000.0"));
    assert_eq!(pnl_long, d("1000.0")); // (46000 - 45000) * 1.0

    // 多头持仓，价格下跌
    let pnl_long_loss = position.unrealized_pnl(d("44000.0"));
    assert_eq!(pnl_long_loss, d("-1000.0"));

    // 切换为空头
    position.direction = PositionDirection::Short;
    let pnl_short = position.unrealized_pnl(d("44000.0"));
    assert_eq!(pnl_short, d("1000.0")); // (45000 - 44000) * 1.0
}

#[test]
fn test_local_position_notional_value() {
    let position = LocalPosition::new(
        "BTCUSDT".to_string(),
        PositionDirection::Long,
        d("2.0"),
        d("50000.0"),
        "strategy_001".to_string(),
    );

    let notional = position.notional_value(d("51000.0"));
    assert_eq!(notional, d("102000.0")); // 2.0 * 51000
}

// ============================================================================
// 测试 XD-002: PositionDirection 持仓方向
// ============================================================================

#[test]
fn test_position_direction_is_long() {
    assert!(PositionDirection::Long.is_long());
    assert!(PositionDirection::NetLong.is_long());
    assert!(!PositionDirection::Short.is_long());
    assert!(!PositionDirection::NetShort.is_long());
    assert!(!PositionDirection::Flat.is_long());
}

#[test]
fn test_position_direction_is_short() {
    assert!(PositionDirection::Short.is_short());
    assert!(PositionDirection::NetShort.is_short());
    assert!(!PositionDirection::Long.is_short());
    assert!(!PositionDirection::NetLong.is_short());
    assert!(!PositionDirection::Flat.is_short());
}

#[test]
fn test_position_direction_is_flat() {
    assert!(PositionDirection::Flat.is_flat());
    assert!(!PositionDirection::Long.is_flat());
    assert!(!PositionDirection::Short.is_flat());
}

// ============================================================================
// 测试 XD-003: PositionSide 持仓边
// ============================================================================

#[test]
fn test_position_side_is_long() {
    assert!(PositionSide::Long.is_long());
    assert!(PositionSide::Both.is_long());
    assert!(!PositionSide::Short.is_long());
    assert!(!PositionSide::None.is_long());
}

#[test]
fn test_position_side_is_short() {
    assert!(PositionSide::Short.is_short());
    assert!(PositionSide::Both.is_short());
    assert!(!PositionSide::Long.is_short());
    assert!(!PositionSide::None.is_short());
}

#[test]
fn test_position_side_is_flat() {
    assert!(PositionSide::None.is_flat());
    assert!(!PositionSide::Long.is_flat());
    assert!(!PositionSide::Both.is_flat());
}

// ============================================================================
// 测试 XD-006: KLine K线数据结构
// ============================================================================

#[test]
fn test_kline_creation() {
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: d("50000.0"),
        high: d("51000.0"),
        low: d("49500.0"),
        close: d("50500.0"),
        volume: d("100.0"),
        timestamp: Utc::now(),
    };

    assert_eq!(kline.symbol, "BTCUSDT");
    assert_eq!(kline.open, d("50000.0"));
    assert_eq!(kline.high, d("51000.0"));
    assert_eq!(kline.low, d("49500.0"));
    assert_eq!(kline.close, d("50500.0"));
    assert_eq!(kline.volume, d("100.0"));
}

#[test]
fn test_kline_period() {
    let kline_1m = KLine {
        symbol: "ETHUSDT".to_string(),
        period: Period::Minute(1),
        open: d("3000.0"),
        high: d("3100.0"),
        low: d("2900.0"),
        close: d("3050.0"),
        volume: d("50.0"),
        timestamp: Utc::now(),
    };

    let kline_1d = KLine {
        symbol: "ETHUSDT".to_string(),
        period: Period::Day,
        open: d("3000.0"),
        high: d("3200.0"),
        low: d("2800.0"),
        close: d("3100.0"),
        volume: d("1000.0"),
        timestamp: Utc::now(),
    };

    assert!(matches!(kline_1m.period, Period::Minute(1)));
    assert!(matches!(kline_1d.period, Period::Day));
}

// ============================================================================
// 测试 XD-007: Tick Tick数据结构
// ============================================================================

#[test]
fn test_tick_creation() {
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: d("50000.0"),
        high: d("51000.0"),
        low: d("49500.0"),
        close: d("50500.0"),
        volume: d("10.0"),
        timestamp: Utc::now(),
    };

    let tick = Tick {
        symbol: "BTCUSDT".to_string(),
        price: d("50500.0"),
        qty: d("0.5"),
        timestamp: Utc::now(),
        kline_1m: Some(kline.clone()),
        kline_15m: None,
        kline_1d: None,
    };

    assert_eq!(tick.symbol, "BTCUSDT");
    assert_eq!(tick.price, d("50500.0"));
    assert_eq!(tick.qty, d("0.5"));
    assert!(tick.kline_1m.is_some());
    assert!(tick.kline_15m.is_none());
    assert!(tick.kline_1d.is_none());
}

// ============================================================================
// 测试 XD-011: SymbolRulesData 交易对规则
// ============================================================================

#[test]
fn test_symbol_rules_data_creation() {
    let rules = SymbolRulesData {
        symbol: "BTCUSDT".to_string(),
        price_precision: 2,
        quantity_precision: 3,
        tick_size: d("0.01"),
        min_qty: d("0.001"),
        step_size: d("0.001"),
        min_notional: d("10.0"),
        max_notional: d("1000000.0"),
        leverage: 10,
        max_leverage: 125,
        maker_fee: d("0.0002"),
        taker_fee: d("0.0004"),
    };

    assert_eq!(rules.symbol, "BTCUSDT");
    assert_eq!(rules.price_precision, 2);
    assert_eq!(rules.quantity_precision, 3);
    assert_eq!(rules.tick_size, d("0.01"));
    assert_eq!(rules.min_qty, d("0.001"));
    assert_eq!(rules.max_leverage, 125);
}

// ============================================================================
// 测试 XD-012: ParsedSymbolRules 解析后规则
// ============================================================================

#[test]
fn test_parsed_symbol_rules_creation() {
    let rules = ParsedSymbolRules {
        symbol: "BTCUSDT".to_string(),
        price_precision: 2,
        quantity_precision: 3,
        tick_size: d("0.01"),
        min_qty: d("0.001"),
        step_size: d("0.001"),
        min_notional: d("10.0"),
        max_notional: d("1000000.0"),
        leverage: 10,
        maker_fee: d("0.0002"),
        taker_fee: d("0.0004"),
        close_min_ratio: d("2.0"),
        min_value_threshold: d("5.0"),
        update_ts: 1700000000,
    };

    assert_eq!(rules.symbol, "BTCUSDT");
    assert_eq!(rules.close_min_ratio, d("2.0"));
}

#[test]
fn test_parsed_symbol_rules_effective_min_qty() {
    let rules = ParsedSymbolRules {
        symbol: "BTCUSDT".to_string(),
        price_precision: 2,
        quantity_precision: 3,
        tick_size: d("0.01"),          // 价格步长 0.01
        min_qty: d("0.001"),            // 最小数量 0.001
        step_size: d("0.001"),
        min_notional: d("10.0"),        // 最小名义价值 10
        max_notional: d("1000000.0"),
        leverage: 10,
        maker_fee: d("0.0002"),
        taker_fee: d("0.0004"),
        close_min_ratio: d("2.0"),
        min_value_threshold: d("5.0"),
        update_ts: 1700000000,
    };

    // effective_min_qty = ceil(10 / 0.01) = 1000, 但 min_qty = 0.001
    // 所以返回 max(1000, 0.001) = 1000
    let effective = rules.effective_min_qty();
    assert_eq!(effective, d("1000"));
}

// ============================================================================
// 测试 XD-013: OrderResult 订单结果
// ============================================================================

#[test]
fn test_order_result_success() {
    let result = OrderResult {
        order_id: "123456".to_string(),
        status: "FILLED".to_string(),
        filled_qty: d("1.0"),
        filled_price: d("50000.0"),
        commission: d("20.0"),
        reject_reason: None,
        message: "Order filled successfully".to_string(),
    };

    assert_eq!(result.order_id, "123456");
    assert_eq!(result.status, "FILLED");
    assert_eq!(result.filled_qty, d("1.0"));
    assert_eq!(result.filled_price, d("50000.0"));
    assert!(result.reject_reason.is_none());
}

#[test]
fn test_order_result_rejected() {
    let result = OrderResult {
        order_id: "789012".to_string(),
        status: "REJECTED".to_string(),
        filled_qty: d("0.0"),
        filled_price: d("0.0"),
        commission: d("0.0"),
        reject_reason: Some(OrderRejectReason::InsufficientBalance),
        message: "Insufficient balance".to_string(),
    };

    assert_eq!(result.status, "REJECTED");
    assert!(result.reject_reason.is_some());
    assert_eq!(result.reject_reason.unwrap(), OrderRejectReason::InsufficientBalance);
}

#[test]
fn test_order_reject_reason_variants() {
    assert!(matches!(OrderRejectReason::InsufficientBalance, OrderRejectReason::InsufficientBalance));
    assert!(matches!(OrderRejectReason::PositionLimitExceeded, OrderRejectReason::PositionLimitExceeded));
    assert!(matches!(OrderRejectReason::MarginInsufficient, OrderRejectReason::MarginInsufficient));
    assert!(matches!(OrderRejectReason::PriceDeviationExceeded, OrderRejectReason::PriceDeviationExceeded));
    assert!(matches!(OrderRejectReason::SymbolNotTradable, OrderRejectReason::SymbolNotTradable));
    assert!(matches!(OrderRejectReason::OrderFrequencyExceeded, OrderRejectReason::OrderFrequencyExceeded));
    assert!(matches!(OrderRejectReason::SystemError, OrderRejectReason::SystemError));
    assert!(matches!(OrderRejectReason::Unknown, OrderRejectReason::Unknown));
}

// ============================================================================
// 测试 XD-014: FuturesPosition 期货持仓
// ============================================================================

#[test]
fn test_futures_position_creation() {
    let position = FuturesPosition {
        symbol: "BTCUSDT".to_string(),
        side: "LONG".to_string(),
        qty: d("1.5"),
        entry_price: d("45000.0"),
        mark_price: d("46000.0"),
        unrealized_pnl: d("1500.0"),
        leverage: 10,
    };

    assert_eq!(position.symbol, "BTCUSDT");
    assert_eq!(position.side, "LONG");
    assert_eq!(position.qty, d("1.5"));
    assert_eq!(position.entry_price, d("45000.0"));
    assert_eq!(position.mark_price, d("46000.0"));
    assert_eq!(position.unrealized_pnl, d("1500.0"));
    assert_eq!(position.leverage, 10);
}

// ============================================================================
// 测试 XD-015: FuturesAccount 期货账户
// ============================================================================

#[test]
fn test_futures_account_creation() {
    let account = FuturesAccount {
        account_id: "acc_001".to_string(),
        total_assets: d("100000.0"),
        available: d("50000.0"),
        margin_balance: d("60000.0"),
        unrealized_pnl: d("5000.0"),
        account_tier: "VIP1".to_string(),
    };

    assert_eq!(account.account_id, "acc_001");
    assert_eq!(account.total_assets, d("100000.0"));
    assert_eq!(account.available, d("50000.0"));
    assert_eq!(account.margin_balance, d("60000.0"));
    assert_eq!(account.unrealized_pnl, d("5000.0"));
    assert_eq!(account.account_tier, "VIP1");
}

// ============================================================================
// 测试 XD-016: StrategySignal 策略信号
// ============================================================================

#[test]
fn test_strategy_signal_open() {
    let strategy_id = StrategyId::new_trend_minute("strategy_001");
    let signal = StrategySignal::open(
        PositionSide::Long,
        d("1.0"),
        d("50000.0"),
        strategy_id.clone(),
        "Breakout detected",
    );

    assert_eq!(signal.command, TradeCommand::Open);
    assert_eq!(signal.direction, PositionSide::Long);
    assert_eq!(signal.quantity, d("1.0"));
    assert_eq!(signal.target_price, d("50000.0"));
    assert!(signal.position_ref.is_none());
    assert!(!signal.full_close);
    assert_eq!(signal.reason, "Breakout detected");
    assert_eq!(signal.confidence, 80);
}

#[test]
fn test_strategy_signal_add() {
    let strategy_id = StrategyId::new_trend_minute("strategy_001");
    let position_ref = PositionRef {
        position_id: "pos_123".to_string(),
        strategy_instance_id: "strategy_001".to_string(),
        side: PositionSide::Long,
    };

    let signal = StrategySignal::add(
        PositionSide::Long,
        d("0.5"),
        d("51000.0"),
        strategy_id.clone(),
        position_ref.clone(),
        "Add to position",
    );

    assert_eq!(signal.command, TradeCommand::Add);
    assert_eq!(signal.quantity, d("0.5"));
    assert!(signal.position_ref.is_some());
}

#[test]
fn test_strategy_signal_flat_all() {
    let strategy_id = StrategyId::new_trend_day("strategy_002");
    let position_ref = PositionRef {
        position_id: "pos_456".to_string(),
        strategy_instance_id: "strategy_002".to_string(),
        side: PositionSide::Short,
    };

    let signal = StrategySignal::flat_all(
        strategy_id.clone(),
        position_ref.clone(),
        "Take profit",
    );

    assert_eq!(signal.command, TradeCommand::FlatAll);
    assert!(signal.full_close);
    assert_eq!(signal.confidence, 90);
}

// ============================================================================
// 测试 XD-017: TradeCommand 交易命令
// ============================================================================

#[test]
fn test_trade_command_variants() {
    assert!(matches!(TradeCommand::Open, TradeCommand::Open));
    assert!(matches!(TradeCommand::Add, TradeCommand::Add));
    assert!(matches!(TradeCommand::Reduce, TradeCommand::Reduce));
    assert!(matches!(TradeCommand::FlatAll, TradeCommand::FlatAll));
    assert!(matches!(TradeCommand::FlatPosition, TradeCommand::FlatPosition));
    assert!(matches!(TradeCommand::HedgeOpen, TradeCommand::HedgeOpen));
    assert!(matches!(TradeCommand::HedgeClose, TradeCommand::HedgeClose));
}

// ============================================================================
// 测试 StrategyId 策略标识
// ============================================================================

#[test]
fn test_strategy_id_trend_minute() {
    let id = StrategyId::new_trend_minute("inst_001");
    assert_eq!(id.strategy_type, StrategyType::Trend);
    assert_eq!(id.level, StrategyLevel::Minute);
    assert_eq!(id.instance_id, "inst_001");
}

#[test]
fn test_strategy_id_trend_day() {
    let id = StrategyId::new_trend_day("inst_002");
    assert_eq!(id.strategy_type, StrategyType::Trend);
    assert_eq!(id.level, StrategyLevel::Day);
}

#[test]
fn test_strategy_id_pin_minute() {
    let id = StrategyId::new_pin_minute("inst_003");
    assert_eq!(id.strategy_type, StrategyType::Pin);
    assert_eq!(id.level, StrategyLevel::Minute);
}

#[test]
fn test_strategy_id_pin_day() {
    let id = StrategyId::new_pin_day("inst_004");
    assert_eq!(id.strategy_type, StrategyType::Pin);
    assert_eq!(id.level, StrategyLevel::Day);
}

// ============================================================================
// 边界条件测试
// ============================================================================

#[test]
fn test_local_position_zero_qty() {
    let position = LocalPosition::new(
        "BTCUSDT".to_string(),
        PositionDirection::Long,
        d("0.0"),
        d("50000.0"),
        "strategy_001".to_string(),
    );

    // 零数量时未实现盈亏应为 0
    let pnl = position.unrealized_pnl(d("51000.0"));
    assert_eq!(pnl, d("0.0"));
}

#[test]
fn test_local_position_flat_direction() {
    let position = LocalPosition::new(
        "BTCUSDT".to_string(),
        PositionDirection::Flat,
        d("1.0"),
        d("50000.0"),
        "strategy_001".to_string(),
    );

    // Flat 方向的未实现盈亏应为 0
    let pnl = position.unrealized_pnl(d("51000.0"));
    assert_eq!(pnl, d("0.0"));
}

#[test]
fn test_symbol_rules_zero_values() {
    let rules = ParsedSymbolRules {
        symbol: "TEST".to_string(),
        price_precision: 0,
        quantity_precision: 0,
        tick_size: d("0.0"),        // 零值
        min_qty: d("0.0"),          // 零值
        step_size: d("0.0"),
        min_notional: d("0.0"),     // 零值
        max_notional: d("1000.0"),
        leverage: 1,
        maker_fee: d("0.0"),
        taker_fee: d("0.0"),
        close_min_ratio: d("0.0"),
        min_value_threshold: d("0.0"),
        update_ts: 0,
    };

    // 零值情况下 effective_min_qty 应返回 min_qty (0)
    let effective = rules.effective_min_qty();
    assert_eq!(effective, d("0.0"));
}

#[test]
fn test_order_result_all_reject_reasons() {
    let reasons = vec![
        OrderRejectReason::InsufficientBalance,
        OrderRejectReason::PositionLimitExceeded,
        OrderRejectReason::MarginInsufficient,
        OrderRejectReason::PriceDeviationExceeded,
        OrderRejectReason::SymbolNotTradable,
        OrderRejectReason::OrderFrequencyExceeded,
        OrderRejectReason::SystemError,
        OrderRejectReason::Unknown,
    ];

    for reason in reasons {
        let result = OrderResult {
            order_id: "test".to_string(),
            status: "REJECTED".to_string(),
            filled_qty: d("0.0"),
            filled_price: d("0.0"),
            commission: d("0.0"),
            reject_reason: Some(reason),
            message: "".to_string(),
        };
        assert!(result.reject_reason.is_some());
    }
}
