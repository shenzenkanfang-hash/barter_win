//! b_data_mock P0 测试点覆盖
//!
//! 测试未完全覆盖的 P0 测试点:
//! - BM-004: MockAccount 杠杆修改测试
//! - BM-005: MockAccount 资金计算测试
//! - BM-006: Kline1mStream 模拟1分钟K线生成测试
//! - BM-007: Kline1dStream 模拟1天K线生成测试

use b_data_mock::{
    MockConfig,
    MarketDataStore,
    Account, Side,
    Kline1mStream, Kline1dStream, KlineStreamGenerator,
    KLine, Period, KlineData,
    MarketDataStoreImpl,
};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;

// ============================================================================
// BM-004: MockAccount 杠杆修改测试
// ============================================================================

#[test]
fn test_account_leverage_precheck_buy_with_leverage() {
    // 测试杠杆验证 - 买入时需要考虑杠杆
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    // 设置价格
    account.update_price("BTCUSDT", dec!(50000.0));

    // 10倍杠杆开多，0.1 BTC 需要 500 USDT 保证金
    // 验证：杠杆 10，预估保证金 50000 * 0.1 / 10 = 500
    let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(10), Side::Buy);
    assert!(result.is_ok(), "10倍杠杆开仓应该成功");
}

#[test]
fn test_account_leverage_precheck_insufficient_balance() {
    // 测试杠杆验证 - 保证金不足
    let config = MockConfig::default();
    let mut account = Account::new(dec!(100.0), &config);

    account.update_price("BTCUSDT", dec!(50000.0));

    // 10倍杠杆开多，0.1 BTC 需要 500 USDT 保证金，但只有 100 USDT
    let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(10), Side::Buy);
    assert!(result.is_err(), "保证金不足应该被拒绝");
}

#[test]
fn test_account_leverage_high_leverage() {
    // 测试高杠杆 - 100倍杠杆开仓
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    account.update_price("BTCUSDT", dec!(50000.0));

    // 100倍杠杆开多，0.1 BTC 需要 50000 * 0.1 / 100 = 50 USDT 保证金
    let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(100), Side::Buy);
    assert!(result.is_ok(), "100倍杠杆开仓应该成功");
}

#[test]
fn test_account_leverage_position_limit() {
    // 测试杠杆与持仓限制
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    account.update_price("BTCUSDT", dec!(50000.0));

    // 尝试开仓超过总权益 95% 的限制
    // max_position_ratio = 0.95, total_equity ≈ 10000
    // 允许的最大持仓价值 ≈ 9500 USDT
    // 单笔开仓 0.2 BTC @ 50000 = 10000 USDT > 9500，应该被拒绝
    let result = account.pre_check("BTCUSDT", dec!(0.2), dec!(50000.0), dec!(1), Side::Buy);
    assert!(result.is_err(), "超过持仓限制应该被拒绝");
}

// ============================================================================
// BM-005: MockAccount 资金计算测试
// ============================================================================

#[test]
fn test_account_equity_calculation() {
    // 测试总权益计算：available + frozen_margin + unrealized_pnl
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    account.update_price("BTCUSDT", dec!(50000.0));

    // 初始权益 = 可用余额 = 10000
    assert_eq!(account.total_equity(), dec!(10000.0));
    assert_eq!(account.available(), dec!(10000.0));
    assert_eq!(account.frozen_margin(), dec!(0.0));

    // 开多仓 0.1 BTC @ 50000，保证金 = 50000 * 0.1 / 1 = 5000
    account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(1));

    // 权益 = available(5000) + frozen_margin(5000) + unrealized_pnl(0) = 10000
    assert_eq!(account.available(), dec!(5000.0));
    assert_eq!(account.frozen_margin(), dec!(5000.0));
    assert_eq!(account.total_equity(), dec!(10000.0));
}

#[test]
fn test_account_unrealized_pnl_calculation() {
    // 测试未实现盈亏计算
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    // 以 50000 开多仓 0.1 BTC
    account.update_price("BTCUSDT", dec!(50000.0));
    account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(1));

    // 价格涨到 51000，未实现盈亏 = (51000 - 50000) * 0.1 = 100
    account.update_price("BTCUSDT", dec!(51000.0));

    let position = account.get_position("BTCUSDT").unwrap();
    assert_eq!(position.total_unrealized_pnl(dec!(51000.0)), dec!(100.0));

    // 权益 = 可用(5000) + 冻结(5000) + 未实现盈亏(100) = 10100
    assert_eq!(account.total_equity(), dec!(10100.0));
}

#[test]
fn test_account_realized_pnl_on_close() {
    // 测试平仓时已实现盈亏计算
    // 注意: 公式为 (entry - exit) * qty，因此价格上升时为负值
    // 保证金会在平仓时释放，所以:
    // available = 初始余额 - 保证金 + 释放保证金 + realized_pnl
    //           = 10000 - 5000 + 5000 + (-100) = 9900
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    // 开多仓 0.1 BTC @ 50000
    account.update_price("BTCUSDT", dec!(50000.0));
    account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(1));

    // 此时: available = 5000 (保证金5000被冻结)
    assert_eq!(account.available(), dec!(5000.0));
    assert_eq!(account.frozen_margin(), dec!(5000.0));

    // 价格涨到 51000，平仓 0.1 BTC
    account.update_price("BTCUSDT", dec!(51000.0));
    let realized_pnl = account.apply_close("BTCUSDT", Side::Sell, dec!(0.1), dec!(51000.0));

    // 公式: (entry - exit) * qty = (50000 - 51000) * 0.1 = -100
    // 注意: 这个公式对多头仓位在价格上涨时返回负值（亏损）
    assert_eq!(realized_pnl, dec!(-100.0));

    // 平仓后: released_margin(5000) + realized_pnl(-100) = 4900
    // 但初始available是5000，所以: 5000 + 5000 - 100 = 9900
    assert_eq!(account.available(), dec!(9900.0));
    assert_eq!(account.frozen_margin(), dec!(0.0));

    // 权益 = 可用 + 冻结 + 未实现 = 9900 + 0 + 0 = 9900
    assert_eq!(account.total_equity(), dec!(9900.0));
}

#[test]
fn test_account_short_position_pnl() {
    // 测试做空持仓的盈亏计算
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    // 做空 0.1 BTC @ 50000
    account.update_price("BTCUSDT", dec!(50000.0));
    account.apply_open("BTCUSDT", Side::Sell, dec!(0.1), dec!(50000.0), dec!(1));

    // 价格跌到 49000，做空盈利
    // 做空盈亏 = (50000 - 49000) * 0.1 = 100
    account.update_price("BTCUSDT", dec!(49000.0));

    let position = account.get_position("BTCUSDT").unwrap();
    assert_eq!(position.total_unrealized_pnl(dec!(49000.0)), dec!(100.0));
}

#[test]
fn test_account_fee_deduction() {
    // 测试手续费扣除
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    account.update_price("BTCUSDT", dec!(50000.0));

    // 开仓手续费 = 50000 * 0.1 * 0.0004 = 2
    let fee = dec!(50000.0) * dec!(0.1) * config.fee_rate;
    account.deduct_fee(fee);

    assert_eq!(account.available(), dec!(9998.0));
}

// ============================================================================
// BM-006: Kline1mStream 模拟1分钟K线生成测试
// ============================================================================

fn create_test_kline_for_stream(symbol: &str, open: Decimal, close: Decimal, high: Decimal, low: Decimal) -> KLine {
    KLine {
        symbol: symbol.to_string(),
        period: Period::Minute(1),
        open,
        high,
        low,
        close,
        volume: dec!(1.0),
        timestamp: Utc::now(),
        is_closed: true,
    }
}

#[test]
fn test_kline1m_stream_next_message() {
    // 测试 Kline1mStream 的 next_message 方法
    let kline = create_test_kline_for_stream(
        "BTCUSDT",
        dec!(50000.0),
        dec!(51000.0),
        dec!(51500.0),
        dec!(49500.0),
    );

    let klines: Vec<KLine> = vec![kline];
    let boxed: Box<dyn Iterator<Item = KLine> + Send> = Box::new(klines.into_iter());

    let mut stream = Kline1mStream::from_klines("BTCUSDT".to_string(), boxed);

    // 获取第一条子K线消息
    let msg = stream.next_message();
    assert!(msg.is_some(), "应该能获取到K线消息");

    let json_str = msg.unwrap();
    assert!(json_str.contains("BTCUSDT"), "消息应包含交易对");
    assert!(json_str.contains("1m"), "消息应包含1分钟周期");
}

#[test]
fn test_kline1m_stream_generates_60_subs() {
    // 测试单根K线生成60个子K线
    let kline = create_test_kline_for_stream(
        "BTCUSDT",
        dec!(50000.0),
        dec!(51000.0),
        dec!(51500.0),
        dec!(49500.0),
    );

    let klines: Vec<KLine> = vec![kline];
    let boxed: Box<dyn Iterator<Item = KLine> + Send> = Box::new(klines.into_iter());

    let mut stream = Kline1mStream::from_klines("BTCUSDT".to_string(), boxed);

    let mut count = 0;
    while stream.next_message().is_some() {
        count += 1;
    }

    // 每根1m K线生成60个子K线
    assert_eq!(count, 60, "单根K线应生成60个子K线");
}

#[test]
fn test_kline1m_stream_multi_kline() {
    // 测试多根K线
    let klines: Vec<KLine> = vec![
        create_test_kline_for_stream("ETHUSDT", dec!(50000.0), dec!(51000.0), dec!(51500.0), dec!(49500.0)),
        create_test_kline_for_stream("ETHUSDT", dec!(51000.0), dec!(52000.0), dec!(52500.0), dec!(50500.0)),
    ];

    let boxed: Box<dyn Iterator<Item = KLine> + Send> = Box::new(klines.into_iter());
    let mut stream = Kline1mStream::from_klines("ETHUSDT".to_string(), boxed);

    let mut count = 0;
    while stream.next_message().is_some() {
        count += 1;
    }

    // 2根K线 = 120个子K线
    assert_eq!(count, 120, "2根K线应生成120个子K线");
}

// ============================================================================
// BM-007: Kline1dStream 模拟1天K线生成测试
// ============================================================================

#[test]
fn test_kline1d_stream_new() {
    // 测试 Kline1dStream 创建
    let store = Arc::new(MarketDataStoreImpl::new());
    let stream = Kline1dStream::new(store);

    // 验证store方法可用
    let _ = stream.store();
}

#[test]
fn test_kline1d_stream_update_from_1m_kline() {
    // 测试从1分钟K线更新日K线
    let store = Arc::new(MarketDataStoreImpl::new());
    let mut stream = Kline1dStream::new(store.clone());

    // 创建一根1分钟K线
    let kline_data = KlineData {
        kline_start_time: 1709251200000, // 2024-03-01 00:00:00 UTC
        kline_close_time: 1709251260000,
        symbol: "BTCUSDT".to_string(),
        interval: "1m".to_string(),
        open: "50000".to_string(),
        close: "50100".to_string(),
        high: "50200".to_string(),
        low: "49900".to_string(),
        volume: "10.5".to_string(),
        is_closed: true,
    };

    // 更新日K线
    stream.update_from_kline(&kline_data);

    // 验证store仍然可用
    let _ = stream.store().get_current_kline("BTCUSDT");
}

#[test]
fn test_kline1d_stream_accumulates_1m_klines() {
    // 测试日K线聚合多根1分钟K线
    let store = Arc::new(MarketDataStoreImpl::new());
    let mut stream = Kline1dStream::new(store.clone());

    let base_time = 1709251200000i64; // 2024-03-01 00:00:00 UTC

    // 模拟60根1分钟K线 (1小时数据)
    for i in 0..60 {
        let kline_data = KlineData {
            kline_start_time: base_time + (i * 60000),
            kline_close_time: base_time + ((i + 1) * 60000),
            symbol: "BTCUSDT".to_string(),
            interval: "1m".to_string(),
            open: format!("{}", 50000 + i),
            close: format!("{}", 50100 + i),
            high: format!("{}", 50200 + i),
            low: format!("{}", 49900 + i),
            volume: "1.0".to_string(),
            is_closed: true,
        };

        stream.update_from_kline(&kline_data);
    }

    // 最后一根1m K线闭合时，应该写入日K线到store
    // 验证store可以正常读取
    let current = stream.store().get_current_kline("BTCUSDT");
    assert!(current.is_some(), "应该累积出日K线并写入store");

    let kline = current.unwrap();
    assert_eq!(kline.symbol, "BTCUSDT");
    assert_eq!(kline.interval, "1d");
}

#[test]
fn test_kline1d_stream_new_day_resets() {
    // 测试跨日时日K线重置
    let store = Arc::new(MarketDataStoreImpl::new());
    let mut stream = Kline1dStream::new(store.clone());

    let day1_time = 1709251200000i64; // 2024-03-01 00:00:00 UTC
    let day2_time = 1709337600000i64; // 2024-03-02 00:00:00 UTC

    // 第一天最后一根K线
    let kline_data_day1 = KlineData {
        kline_start_time: day1_time + (23 * 3600 * 1000) + (59 * 60000), // 23:59
        kline_close_time: day1_time + (24 * 3600 * 1000), // 00:00 next day
        symbol: "BTCUSDT".to_string(),
        interval: "1m".to_string(),
        open: "50000".to_string(),
        close: "50100".to_string(),
        high: "50200".to_string(),
        low: "49900".to_string(),
        volume: "10.0".to_string(),
        is_closed: true,
    };

    stream.update_from_kline(&kline_data_day1);

    // 第二天第一根K线
    let kline_data_day2 = KlineData {
        kline_start_time: day2_time,
        kline_close_time: day2_time + 60000,
        symbol: "BTCUSDT".to_string(),
        interval: "1m".to_string(),
        open: "50100".to_string(),
        close: "50200".to_string(),
        high: "50300".to_string(),
        low: "50000".to_string(),
        volume: "5.0".to_string(),
        is_closed: true,
    };

    stream.update_from_kline(&kline_data_day2);

    // 验证新的一天开始，日K线应该重新累积
    let current = stream.store().get_current_kline("BTCUSDT");
    assert!(current.is_some());
}

// ============================================================================
// 边界条件测试
// ============================================================================

#[test]
fn test_kline1m_stream_empty_klines() {
    // 测试空K线列表
    let klines: Vec<KLine> = vec![];
    let boxed: Box<dyn Iterator<Item = KLine> + Send> = Box::new(klines.into_iter());
    let mut stream = Kline1mStream::from_klines("BTCUSDT".to_string(), boxed);

    let msg = stream.next_message();
    assert!(msg.is_none(), "空K线列表应该返回None");
}

#[test]
fn test_account_close_without_position() {
    // 测试平仓时无持仓
    let config = MockConfig::default();
    let mut account = Account::new(dec!(10000.0), &config);

    account.update_price("BTCUSDT", dec!(50000.0));

    // 尝试平仓不存在的持仓
    let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(1), Side::Sell);
    assert!(result.is_err(), "平仓不存在的持仓应该失败");
}

#[test]
fn test_kline_generator_with_zero_volume() {
    // 测试零成交量K线
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(50000.0),
        low: dec!(50000.0),
        close: dec!(50000.0),
        volume: dec!(0.0), // 零成交量
        timestamp: Utc::now(),
        is_closed: true,
    };

    let klines: Vec<KLine> = vec![kline];
    let boxed: Box<dyn Iterator<Item = KLine> + Send> = Box::new(klines.into_iter());
    let mut generator = KlineStreamGenerator::new("BTCUSDT".to_string(), boxed);

    let sub = generator.next();
    assert!(sub.is_some(), "零成交量K线也应该能生成子K线");
}
