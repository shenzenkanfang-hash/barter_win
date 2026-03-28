//! ReplaySource 测试 - 历史数据回放

use b_data_mock::{ReplaySource, KLine, Period};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn create_kline(close: Decimal) -> KLine {
    KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: close - dec!(100),
        high: close + dec!(50),
        low: close - dec!(150),
        close,
        volume: dec!(10.0),
        timestamp: Utc::now(),
        is_closed: true,
    }
}

#[test]
fn test_replay_from_data() {
    let klines = vec![
        create_kline(dec!(50000.0)),
        create_kline(dec!(50100.0)),
        create_kline(dec!(50200.0)),
    ];

    let mut source = ReplaySource::from_data(klines);

    assert_eq!(source.len(), 3);
    assert!(!source.is_exhausted());
}

#[test]
fn test_replay_iteration() {
    let klines = vec![
        create_kline(dec!(50000.0)),
        create_kline(dec!(50100.0)),
        create_kline(dec!(50200.0)),
    ];

    let mut source = ReplaySource::from_data(klines);

    let k1 = source.next_kline().unwrap();
    assert_eq!(k1.close, dec!(50000.0));

    let k2 = source.next_kline().unwrap();
    assert_eq!(k2.close, dec!(50100.0));

    let k3 = source.next_kline().unwrap();
    assert_eq!(k3.close, dec!(50200.0));
}

#[test]
fn test_replay_exhausted() {
    let klines = vec![create_kline(dec!(50000.0))];

    let mut source = ReplaySource::from_data(klines);

    assert!(source.next_kline().is_some());
    assert!(source.next_kline().is_none());
    assert!(source.is_exhausted());
}

#[test]
fn test_replay_iterator_trait() {
    let klines = vec![
        create_kline(dec!(50000.0)),
        create_kline(dec!(50100.0)),
    ];

    let source = ReplaySource::from_data(klines);

    let collected: Vec<KLine> = source.collect();
    assert_eq!(collected.len(), 2);
}

#[test]
fn test_replay_reset() {
    let klines = vec![
        create_kline(dec!(50000.0)),
        create_kline(dec!(50100.0)),
    ];

    let mut source = ReplaySource::from_data(klines);

    // 消费全部
    source.next_kline();
    source.next_kline();
    assert!(source.is_exhausted());

    // 重置
    source.reset();
    assert!(!source.is_exhausted());

    // 再次迭代
    let k = source.next_kline().unwrap();
    assert_eq!(k.close, dec!(50000.0));
}

#[test]
fn test_replay_with_symbols_filter() {
    let btc = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(50500.0),
        low: dec!(49500.0),
        close: dec!(50000.0),
        volume: dec!(10.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    let eth = KLine {
        symbol: "ETHUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(3000.0),
        high: dec!(3050.0),
        low: dec!(2950.0),
        close: dec!(3000.0),
        volume: dec!(10.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    let mut source = ReplaySource::from_data(vec![btc, eth])
        .with_symbols(vec!["BTCUSDT".to_string()]);

    let k = source.next_kline().unwrap();
    assert_eq!(k.symbol, "BTCUSDT");

    // 下一个应该是 ETH，但被过滤了，所以直接结束
    assert!(source.next_kline().is_none());
}

#[test]
fn test_replay_with_period_filter() {
    let m1 = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(50500.0),
        low: dec!(49500.0),
        close: dec!(50000.0),
        volume: dec!(10.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    let m5 = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(5),
        open: dec!(50000.0),
        high: dec!(50500.0),
        low: dec!(49500.0),
        close: dec!(50000.0),
        volume: dec!(10.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    let mut source = ReplaySource::from_data(vec![m1.clone(), m5.clone()])
        .with_period(Period::Minute(1));

    let k = source.next_kline().unwrap();
    assert_eq!(k.period, Period::Minute(1));

    // 5min 被过滤了
    assert!(source.next_kline().is_none());
}

#[test]
fn test_replay_empty() {
    let mut source = ReplaySource::from_data(vec![]);

    assert!(source.is_empty());
    assert!(source.next_kline().is_none());
}
