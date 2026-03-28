//! DT-001: CheckTable 检查表注册与调度
//!
//! 测试 CheckTable 的核心功能：
//! - next_round_id: 轮次ID生成
//! - fill: 检查表填充
//! - get: 按品种/策略/周期查询
//! - get_by_strategy: 按策略查询
//! - get_high_risk: 获取高风险记录
//! - clear: 清空检查表
//! - current_round_id: 当前轮次ID查询

use chrono::Utc;
use rust_decimal_macros::dec;
use d_checktable::{CheckTable, CheckEntry};
use d_checktable::types::{CheckSignal, CheckChainResult};
use c_data_process::types::PineColor;
use c_data_process::Signal;

fn create_test_entry(
    symbol: &str,
    strategy_id: &str,
    period: &str,
    risk_flag: bool,
) -> CheckEntry {
    CheckEntry {
        symbol: symbol.to_string(),
        strategy_id: strategy_id.to_string(),
        period: period.to_string(),
        ema_signal: Signal::LongExit, // 默认使用 LongExit 表示无信号
        rsi_value: dec!(50),
        pine_color: PineColor::Neutral,
        price_position: dec!(50),
        final_signal: Signal::LongExit, // 默认使用 LongExit 表示无信号
        target_price: dec!(50000),
        quantity: dec!(0.01),
        risk_flag,
        timestamp: Utc::now(),
        round_id: 1,
        is_high_freq: true,
    }
}

#[test]
fn test_checktable_new() {
    let table = CheckTable::new();
    assert_eq!(table.current_round_id(), 0);
}

#[test]
fn test_checktable_next_round_id() {
    let table = CheckTable::new();

    let id1 = table.next_round_id();
    assert_eq!(id1, 1);

    let id2 = table.next_round_id();
    assert_eq!(id2, 2);

    assert_eq!(table.current_round_id(), 2);
}

#[test]
fn test_checktable_fill_and_get() {
    let table = CheckTable::new();

    let entry = create_test_entry("BTCUSDT", "pin", "15m", false);
    table.fill(entry.clone());

    let retrieved = table.get("BTCUSDT", "pin", "15m");
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.symbol, "BTCUSDT");
    assert_eq!(retrieved.strategy_id, "pin");
    assert_eq!(retrieved.period, "15m");
}

#[test]
fn test_checktable_get_nonexistent() {
    let table = CheckTable::new();

    let entry = create_test_entry("BTCUSDT", "pin", "15m", false);
    table.fill(entry);

    // 查询不存在的记录
    let retrieved = table.get("ETHUSDT", "pin", "15m");
    assert!(retrieved.is_none());

    // 查询存在但策略不同
    let retrieved = table.get("BTCUSDT", "trend", "15m");
    assert!(retrieved.is_none());
}

#[test]
fn test_checktable_get_by_strategy() {
    let table = CheckTable::new();

    // 填充多个品种的同一策略
    table.fill(create_test_entry("BTCUSDT", "pin", "15m", false));
    table.fill(create_test_entry("ETHUSDT", "pin", "15m", false));
    table.fill(create_test_entry("BTCUSDT", "trend", "15m", false));

    let pin_entries = table.get_by_strategy("pin");
    assert_eq!(pin_entries.len(), 2);

    let trend_entries = table.get_by_strategy("trend");
    assert_eq!(trend_entries.len(), 1);
}

#[test]
fn test_checktable_get_high_risk() {
    let table = CheckTable::new();

    // 填充高风险和低风险记录
    table.fill(create_test_entry("BTCUSDT", "pin", "15m", true));
    table.fill(create_test_entry("ETHUSDT", "pin", "15m", false));
    table.fill(create_test_entry("BNBUSDT", "pin", "15m", true));

    let high_risk = table.get_high_risk();
    assert_eq!(high_risk.len(), 2);

    // 验证都是高风险的
    for entry in high_risk {
        assert!(entry.risk_flag);
    }
}

#[test]
fn test_checktable_clear() {
    let table = CheckTable::new();

    table.fill(create_test_entry("BTCUSDT", "pin", "15m", false));
    table.fill(create_test_entry("ETHUSDT", "pin", "15m", false));

    assert!(table.get("BTCUSDT", "pin", "15m").is_some());

    table.clear();

    assert!(table.get("BTCUSDT", "pin", "15m").is_none());
    assert_eq!(table.current_round_id(), 0);
}

#[test]
fn test_checktable_overwrite() {
    let table = CheckTable::new();

    let entry1 = create_test_entry("BTCUSDT", "pin", "15m", false);
    table.fill(entry1);

    let entry2 = create_test_entry("BTCUSDT", "pin", "15m", true);
    table.fill(entry2);

    let retrieved = table.get("BTCUSDT", "pin", "15m").unwrap();
    assert!(retrieved.risk_flag); // 应该被覆盖
}

#[test]
fn test_check_chain_result() {
    let mut result = CheckChainResult::new();

    assert!(result.is_empty());

    result.add_signal(CheckSignal::Open);
    assert!(!result.is_empty());
    assert!(result.has(CheckSignal::Open));
    assert!(!result.has(CheckSignal::Close));

    result.add_signal(CheckSignal::Add);
    assert_eq!(result.signals.len(), 2);
}

#[test]
fn test_checktable_multiple_strategies() {
    let table = CheckTable::new();

    table.fill(create_test_entry("BTCUSDT", "pin", "15m", false));
    table.fill(create_test_entry("BTCUSDT", "pin", "1m", false));
    table.fill(create_test_entry("BTCUSDT", "trend", "15m", false));
    table.fill(create_test_entry("ETHUSDT", "pin", "15m", false));

    // 同一品种同一策略不同周期
    let retrieved_15m = table.get("BTCUSDT", "pin", "15m");
    let retrieved_1m = table.get("BTCUSDT", "pin", "1m");
    assert!(retrieved_15m.is_some());
    assert!(retrieved_1m.is_some());
    assert_ne!(retrieved_15m.unwrap().period, retrieved_1m.unwrap().period);
}
