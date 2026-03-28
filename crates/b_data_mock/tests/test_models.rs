//! Models 测试 - KLine, Tick, Period

use b_data_mock::{KLine, Period, Tick};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_period_creation() {
    let m1 = Period::Minute(1);
    let m5 = Period::Minute(5);
    let m60 = Period::Minute(60);
    let day = Period::Day;

    assert_eq!(format!("{:?}", m1), "Minute(1)");
    assert_eq!(format!("{:?}", m5), "Minute(5)");
    assert_eq!(format!("{:?}", m60), "Minute(60)");
    assert_eq!(format!("{:?}", day), "Day");
}

#[test]
fn test_kline_creation() {
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(50500.0),
        low: dec!(49800.0),
        close: dec!(50200.0),
        volume: dec!(100.5),
        timestamp: Utc::now(),
        is_closed: false,
    };

    assert_eq!(kline.symbol, "BTCUSDT");
    assert_eq!(kline.period, Period::Minute(1));
    assert!(kline.high >= kline.low);
    assert!(kline.open <= kline.high);
    assert!(kline.open >= kline.low);
    assert!(kline.close <= kline.high);
    assert!(kline.close >= kline.low);
}

#[test]
fn test_kline_bullish() {
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(51000.0),
        low: dec!(49500.0),
        close: dec!(50500.0),  // 收盘 > 开盘，涨
        volume: dec!(100.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    assert!(kline.close > kline.open);
}

#[test]
fn test_kline_bearish() {
    let kline = KLine {
        symbol: "ETHUSDT".to_string(),
        period: Period::Minute(5),
        open: dec!(3000.0),
        high: dec!(3050.0),
        low: dec!(2900.0),
        close: dec!(2950.0),  // 收盘 < 开盘，跌
        volume: dec!(50.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    assert!(kline.close < kline.open);
}

#[test]
fn test_tick_creation() {
    let tick = Tick {
        symbol: "BTCUSDT".to_string(),
        price: dec!(50200.0),
        qty: dec!(0.5),
        timestamp: Utc::now(),
        sequence_id: 1,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    };

    assert_eq!(tick.symbol, "BTCUSDT");
    assert_eq!(tick.sequence_id, 1);
    assert!(tick.price > Decimal::ZERO);
    assert!(tick.qty > Decimal::ZERO);
}

#[test]
fn test_kline_clone() {
    let kline1 = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(50500.0),
        low: dec!(49800.0),
        close: dec!(50200.0),
        volume: dec!(100.0),
        timestamp: Utc::now(),
        is_closed: false,
    };

    let kline2 = kline1.clone();

    assert_eq!(kline1.symbol, kline2.symbol);
    assert_eq!(kline1.open, kline2.open);
    assert_eq!(kline1.close, kline2.close);
    assert_eq!(kline1.is_closed, kline2.is_closed);
}

#[test]
fn test_kline_serialization() {
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000.0),
        high: dec!(50500.0),
        low: dec!(49800.0),
        close: dec!(50200.0),
        volume: dec!(100.0),
        timestamp: Utc::now(),
        is_closed: true,
    };

    let json = serde_json::to_string(&kline).unwrap();
    let kline_back: KLine = serde_json::from_str(&json).unwrap();

    assert_eq!(kline.symbol, kline_back.symbol);
    assert_eq!(kline.open, kline_back.open);
    assert_eq!(kline.close, kline_back.close);
}
