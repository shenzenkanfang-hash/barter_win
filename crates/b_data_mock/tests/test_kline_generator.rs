//! KlineStreamGenerator 测试 - K线转子K线流

use b_data_mock::{KLine, Period, KlineStreamGenerator, SimulatedKline};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn create_test_kline(symbol: &str, open: Decimal, close: Decimal) -> KLine {
    let (high, low) = if close > open {
        (high_max(open, close), low_min(open, close))
    } else {
        (high_max(close, open), low_min(close, open))
    };

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

fn high_max(a: Decimal, b: Decimal) -> Decimal {
    if a > b { a } else { b }
}

fn low_min(a: Decimal, b: Decimal) -> Decimal {
    if a < b { a } else { b }
}

fn create_generator(symbol: &str, klines: Vec<KLine>) -> KlineStreamGenerator {
    let boxed: Box<dyn Iterator<Item = KLine> + Send> = Box::new(klines.into_iter());
    KlineStreamGenerator::new(symbol.to_string(), boxed)
}

#[test]
fn test_generator_single_kline() {
    let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0));
    let g = create_generator("BTCUSDT", vec![kline]);

    let subs: Vec<SimulatedKline> = g.collect();

    // 每根 K线应生成 60 个子K线
    assert_eq!(subs.len(), 60);

    // 第一个子K线价格应接近开盘价
    assert!((subs[0].price - dec!(50000.0)).abs() < dec!(100.0));

    // 最后一个子K线价格应接近收盘价
    let last = subs.last().unwrap();
    assert!((last.price - dec!(51000.0)).abs() < dec!(100.0));
}

#[test]
fn test_generator_multi_kline() {
    let klines = vec![
        create_test_kline("ETHUSDT", dec!(50000.0), dec!(51000.0)),
        create_test_kline("ETHUSDT", dec!(51000.0), dec!(52000.0)),
        create_test_kline("ETHUSDT", dec!(52000.0), dec!(51500.0)),
    ];

    let g = create_generator("ETHUSDT", klines);
    let subs: Vec<SimulatedKline> = g.collect();

    // 3 根 K线 = 180 个子K线
    assert_eq!(subs.len(), 180);
}

#[test]
fn test_bullish_kline_path() {
    // 上涨 K线：O -> L -> H -> C
    let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0));
    let g = create_generator("BTCUSDT", vec![kline]);
    let subs: Vec<SimulatedKline> = g.collect();

    // 收盘价 > 开盘价
    let first_price = subs.first().unwrap().price;
    let last_price = subs.last().unwrap().price;
    assert!(last_price > first_price);
}

#[test]
fn test_bearish_kline_path() {
    // 下跌 K线：O -> H -> L -> C
    let kline = create_test_kline("BTCUSDT", dec!(51000.0), dec!(50000.0));
    let g = create_generator("BTCUSDT", vec![kline]);
    let subs: Vec<SimulatedKline> = g.collect();

    // 收盘价 < 开盘价
    let first_price = subs.first().unwrap().price;
    let last_price = subs.last().unwrap().price;
    assert!(last_price < first_price);
}

#[test]
fn test_kline_sequence_id() {
    let klines = vec![
        create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0)),
        create_test_kline("BTCUSDT", dec!(51000.0), dec!(52000.0)),
    ];

    let g = create_generator("BTCUSDT", klines);
    let subs: Vec<SimulatedKline> = g.collect();

    // 序列号应连续
    for (i, sub) in subs.iter().enumerate() {
        assert_eq!(sub.sequence_id, (i + 1) as u64);
    }
}

#[test]
fn test_kline_high_low_tracking() {
    let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0));
    let g = create_generator("BTCUSDT", vec![kline.clone()]);
    let subs: Vec<SimulatedKline> = g.collect();

    // 过程中记录的最高/最低价应正确
    let max_price = subs.iter().map(|t| t.high).max().unwrap();
    let min_price = subs.iter().map(|t| t.low).min().unwrap();

    // 最高价应 <= K线的 high
    assert!(max_price <= kline.high);
    // 最低价应 >= K线的 low
    assert!(min_price >= kline.low);
}

#[test]
fn test_last_sub_in_kline() {
    let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0));
    let g = create_generator("BTCUSDT", vec![kline]);
    let subs: Vec<SimulatedKline> = g.collect();

    // 前 59 个不应标记为 K线最后
    for sub in &subs[..59] {
        assert!(!sub.is_last_in_kline);
    }

    // 第 60 个应标记为 K线最后
    let last = subs.last().unwrap();
    assert!(last.is_last_in_kline);
}

#[test]
fn test_empty_klines() {
    let g = create_generator("BTCUSDT", vec![]);
    let subs: Vec<SimulatedKline> = g.collect();

    assert_eq!(subs.len(), 0);
}

#[test]
fn test_flat_kline() {
    // 开盘 = 收盘，震荡 K线
    let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(50000.0));
    let g = create_generator("BTCUSDT", vec![kline]);
    let subs: Vec<SimulatedKline> = g.collect();

    assert_eq!(subs.len(), 60);
}
