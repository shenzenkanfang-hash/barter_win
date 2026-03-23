#![forbid(unsafe_code)]

//! 数据模型功能测试

use b_data_source::models::types::{KLine, Period, Tick};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_tick_creation() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let tick = Tick {
        symbol: "BTCUSDT".to_string(),
        price: dec!(100),
        qty: dec!(1),
        timestamp: ts,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    };

    assert_eq!(tick.symbol, "BTCUSDT");
    assert_eq!(tick.price, dec!(100));
    assert_eq!(tick.qty, dec!(1));
    assert!(tick.kline_1m.is_none());
}

#[test]
fn test_kline_creation() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(100),
        high: dec!(110),
        low: dec!(95),
        close: dec!(105),
        volume: dec!(10),
        timestamp: ts,
    };

    assert_eq!(kline.symbol, "BTCUSDT");
    assert_eq!(kline.period, Period::Minute(1));
    assert_eq!(kline.open, dec!(100));
    assert_eq!(kline.high, dec!(110));
    assert_eq!(kline.low, dec!(95));
    assert_eq!(kline.close, dec!(105));
    assert_eq!(kline.volume, dec!(10));
}

#[test]
fn test_period_minute() {
    let p1 = Period::Minute(1);
    let p5 = Period::Minute(5);
    let p15 = Period::Minute(15);
    let p60 = Period::Minute(60);

    assert_eq!(p1, Period::Minute(1));
    assert_eq!(p5, Period::Minute(5));
    assert_eq!(p15, Period::Minute(15));
    assert_eq!(p60, Period::Minute(60));
}

#[test]
fn test_period_day() {
    let day = Period::Day;
    assert_eq!(day, Period::Day);
}

#[test]
fn test_kline_serialization() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let kline = KLine {
        symbol: "ETHUSDT".to_string(),
        period: Period::Minute(5),
        open: dec!(2000),
        high: dec!(2100),
        low: dec!(1950),
        close: dec!(2050),
        volume: dec!(100),
        timestamp: ts,
    };

    let json = serde_json::to_string(&kline).unwrap();
    assert!(json.contains("ETHUSDT"));
    assert!(json.contains("2000"));
    assert!(json.contains("2100"));
    assert!(json.contains("1950"));
    assert!(json.contains("2050"));

    let restored: KLine = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.symbol, "ETHUSDT");
    assert_eq!(restored.open, dec!(2000));
    assert_eq!(restored.close, dec!(2050));
}

#[test]
fn test_tick_serialization() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 12, 30, 45).unwrap();
    let tick = Tick {
        symbol: "BTCUSDT".to_string(),
        price: dec!(50000.12345),
        qty: dec!(0.5678),
        timestamp: ts,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    };

    let json = serde_json::to_string(&tick).unwrap();
    let restored: Tick = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.symbol, "BTCUSDT");
    assert_eq!(restored.price, dec!(50000.12345));
    assert_eq!(restored.qty, dec!(0.5678));
}

#[test]
fn test_tick_with_klines() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(100),
        high: dec!(110),
        low: dec!(95),
        close: dec!(105),
        volume: dec!(10),
        timestamp: ts,
    };

    let tick = Tick {
        symbol: "BTCUSDT".to_string(),
        price: dec!(105),
        qty: dec!(1),
        timestamp: ts,
        kline_1m: Some(kline.clone()),
        kline_15m: None,
        kline_1d: None,
    };

    assert!(tick.kline_1m.is_some());
    let k = tick.kline_1m.unwrap();
    assert_eq!(k.close, dec!(105));
}

#[test]
fn test_kline_with_different_periods() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    let kline_1m = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(100),
        high: dec!(105),
        low: dec!(99),
        close: dec!(103),
        volume: dec!(50),
        timestamp: ts,
    };

    let kline_1d = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Day,
        open: dec!(100),
        high: dec!(110),
        low: dec!(95),
        close: dec!(105),
        volume: dec!(1000),
        timestamp: ts,
    };

    assert_eq!(kline_1m.period, Period::Minute(1));
    assert_eq!(kline_1d.period, Period::Day);
    assert!(kline_1d.volume > kline_1m.volume);
}

#[test]
fn test_tick_clone() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let tick = Tick {
        symbol: "BTCUSDT".to_string(),
        price: dec!(100),
        qty: dec!(1),
        timestamp: ts,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    };

    let cloned = tick.clone();
    assert_eq!(cloned.symbol, tick.symbol);
    assert_eq!(cloned.price, tick.price);
    assert_eq!(cloned.timestamp, tick.timestamp);
}

#[test]
fn test_kline_clone() {
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(100),
        high: dec!(110),
        low: dec!(95),
        close: dec!(105),
        volume: dec!(10),
        timestamp: ts,
    };

    let cloned = kline.clone();
    assert_eq!(cloned.symbol, kline.symbol);
    assert_eq!(cloned.period, kline.period);
    assert_eq!(cloned.close, kline.close);
}
