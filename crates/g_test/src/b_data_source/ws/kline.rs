#![forbid(unsafe_code)]

//! K线合成器功能测试

use b_data_source::ws::kline_1m::KLineSynthesizer;
use b_data_source::models::types::{KLine, Period, Tick};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn create_tick(symbol: &str, price: Decimal, qty: Decimal, timestamp: chrono::DateTime<chrono::Utc>) -> Tick {
    Tick {
        symbol: symbol.to_string(),
        price,
        qty,
        timestamp,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    }
}

#[test]
fn test_kline_synthesizer_new() {
    let synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    assert_eq!(synth.symbol, "BTCUSDT");
    assert!(synth.current_kline().is_none());
}

#[test]
fn test_kline_synthesizer_first_tick_creates_kline() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let tick = create_tick("BTCUSDT", dec!(100), dec!(1), ts);

    let completed = synth.update(&tick);
    assert!(completed.is_none());

    let current = synth.current_kline().unwrap();
    assert_eq!(current.open, dec!(100));
    assert_eq!(current.high, dec!(100));
    assert_eq!(current.low, dec!(100));
    assert_eq!(current.close, dec!(100));
    assert_eq!(current.volume, dec!(1));
}

#[test]
fn test_kline_synthesizer_incremental_update() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    let ts1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let ts2 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 30).unwrap();

    // First tick
    synth.update(&create_tick("BTCUSDT", dec!(100), dec!(1), ts1));
    // Second tick - same period, should update
    synth.update(&create_tick("BTCUSDT", dec!(105), dec!(2), ts2));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.open, dec!(100));  // First price
    assert_eq!(current.high, dec!(105));  // Max seen
    assert_eq!(current.low, dec!(100));  // Min seen
    assert_eq!(current.close, dec!(105)); // Latest price
    assert_eq!(current.volume, dec!(3)); // Sum qty
}

#[test]
fn test_kline_synthesizer_new_period_completes_kline() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    let ts1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let ts2 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 1, 0).unwrap();  // Next minute

    // First tick at t=0
    synth.update(&create_tick("BTCUSDT", dec!(100), dec!(1), ts1));
    // Second tick at t=1min - completes first kline
    let completed = synth.update(&create_tick("BTCUSDT", dec!(105), dec!(1), ts2));

    // Should return completed kline
    let completed = completed.unwrap();
    assert_eq!(completed.open, dec!(100));
    assert_eq!(completed.close, dec!(100));
    assert_eq!(completed.high, dec!(100));
    assert_eq!(completed.low, dec!(100));

    // Current kline should be new one
    let current = synth.current_kline().unwrap();
    assert_eq!(current.open, dec!(105));
    assert_eq!(current.close, dec!(105));
}

#[test]
fn test_kline_synthesizer_high_low_tracking() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    synth.update(&create_tick("BTCUSDT", dec!(100), dec!(1), ts));
    synth.update(&create_tick("BTCUSDT", dec!(110), dec!(1), ts));
    synth.update(&create_tick("BTCUSDT", dec!(95), dec!(1), ts));
    synth.update(&create_tick("BTCUSDT", dec!(105), dec!(1), ts));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.high, dec!(110));
    assert_eq!(current.low, dec!(95));
    assert_eq!(current.close, dec!(105));
}

#[test]
fn test_kline_synthesizer_day_period() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Day);
    let ts1 = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
    let ts2 = Utc.with_ymd_and_hms(2024, 1, 1, 23, 0, 0).unwrap();
    let ts3 = Utc.with_ymd_and_hms(2024, 1, 2, 5, 0, 0).unwrap();

    // All same day - same kline
    synth.update(&create_tick("BTCUSDT", dec!(100), dec!(1), ts1));
    synth.update(&create_tick("BTCUSDT", dec!(110), dec!(1), ts2));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.high, dec!(110));
    assert_eq!(current.low, dec!(100));

    // Next day - completes kline
    let completed = synth.update(&create_tick("BTCUSDT", dec!(105), dec!(1), ts3));
    assert!(completed.is_some());
}

#[test]
fn test_kline_synthesizer_15min_period() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(15));
    let ts1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let ts2 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 14, 59).unwrap();
    let ts3 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 15, 0).unwrap();

    synth.update(&create_tick("BTCUSDT", dec!(100), dec!(1), ts1));
    synth.update(&create_tick("BTCUSDT", dec!(105), dec!(1), ts2));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.period, Period::Minute(15));

    // Next 15min period - completes
    let completed = synth.update(&create_tick("BTCUSDT", dec!(110), dec!(1), ts3));
    assert!(completed.is_some());
}

#[test]
fn test_kline_synthesizer_volume_accumulation() {
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    synth.update(&create_tick("BTCUSDT", dec!(100), dec!(10), ts));
    synth.update(&create_tick("BTCUSDT", dec!(105), dec!(5), ts));
    synth.update(&create_tick("BTCUSDT", dec!(110), dec!(3), ts));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.volume, dec!(18)); // 10 + 5 + 3
}

#[test]
fn test_kline_struct_serialization() {
    let kline = KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(100),
        high: dec!(110),
        low: dec!(95),
        close: dec!(105),
        volume: dec!(10),
        timestamp: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
    };

    let json = serde_json::to_string(&kline).unwrap();
    assert!(json.contains("BTCUSDT"));
    assert!(json.contains("100"));
    assert!(json.contains("110"));

    let restored: KLine = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.symbol, "BTCUSDT");
    assert_eq!(restored.open, dec!(100));
}

#[test]
fn test_period_enum() {
    assert_eq!(Period::Minute(1), Period::Minute(1));
    assert_eq!(Period::Day, Period::Day);
    assert_ne!(Period::Minute(1), Period::Minute(5));
}
