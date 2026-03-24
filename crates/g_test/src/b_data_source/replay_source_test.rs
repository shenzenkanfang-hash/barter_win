//! ReplaySource 黑盒测试
//!
//! 测试历史数据回放功能

#![forbid(unsafe_code)]

use b_data_source::models::{KLine, Period};
use chrono::Utc;
use rust_decimal_macros::dec;

#[allow(dead_code)]
fn create_sample_klines() -> Vec<KLine> {
    vec![
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(50000),
            high: dec!(50500),
            low: dec!(49500),
            close: dec!(50200),
            volume: dec!(100),
            timestamp: Utc::now(),
        },
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(50200),
            high: dec!(51000),
            low: dec!(50100),
            close: dec!(50800),
            volume: dec!(150),
            timestamp: Utc::now(),
        },
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(50800),
            high: dec!(51500),
            low: dec!(50700),
            close: dec!(51200),
            volume: dec!(120),
            timestamp: Utc::now(),
        },
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(51200),
            high: dec!(52000),
            low: dec!(51100),
            close: dec!(51800),
            volume: dec!(180),
            timestamp: Utc::now(),
        },
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(51800),
            high: dec!(52500),
            low: dec!(51700),
            close: dec!(52300),
            volume: dec!(200),
            timestamp: Utc::now(),
        },
    ]
}

#[allow(dead_code)]
fn create_multi_symbol_klines() -> Vec<KLine> {
    let base_time = Utc::now();
    vec![
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(50000),
            high: dec!(50500),
            low: dec!(49500),
            close: dec!(50200),
            volume: dec!(100),
            timestamp: base_time,
        },
        KLine {
            symbol: "ETHUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(3000),
            high: dec!(3050),
            low: dec!(2950),
            close: dec!(3020),
            volume: dec!(50),
            timestamp: base_time,
        },
        KLine {
            symbol: "BTCUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(50200),
            high: dec!(51000),
            low: dec!(50100),
            close: dec!(50800),
            volume: dec!(150),
            timestamp: base_time,
        },
        KLine {
            symbol: "ETHUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(3020),
            high: dec!(3080),
            low: dec!(3010),
            close: dec!(3050),
            volume: dec!(60),
            timestamp: base_time,
        },
    ]
}

#[test]
fn test_replay_source_from_data() {
    let klines = create_sample_klines();
    let replay = ReplaySource::from_data(klines.clone());

    assert_eq!(replay.len(), 5);
    assert!(!replay.is_empty());
}

#[test]
fn test_replay_source_next_kline() {
    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines);

    let kline = replay.next_kline();
    assert!(kline.is_some());

    let kline = kline.unwrap();
    assert_eq!(kline.symbol, "BTCUSDT");
    assert_eq!(kline.open, dec!(50000));
    assert_eq!(kline.close, dec!(50200));
}

#[test]
fn test_replay_source_next_kline_exhausted() {
    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines);

    // 消费所有数据
    for _ in 0..5 {
        assert!(replay.next_kline().is_some());
    }

    // 现在应该耗尽
    assert!(replay.next_kline().is_none());
    assert!(replay.is_exhausted());
}

#[test]
fn test_replay_source_reset() {
    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines);

    // 消费一些数据
    let _ = replay.next_kline();
    let _ = replay.next_kline();

    assert!(!replay.is_exhausted());

    // 重置
    replay.reset();

    // 应该回到开始
    let kline = replay.next_kline();
    assert!(kline.is_some());
    assert_eq!(kline.unwrap().open, dec!(50000));
}

#[test]
fn test_replay_source_multi_symbol() {
    let klines = create_multi_symbol_klines();
    let mut replay = ReplaySource::from_data(klines);

    // 验证两个品种都有数据
    let kline1 = replay.next_kline().unwrap();
    let kline2 = replay.next_kline().unwrap();

    // 两个 K 线的时间戳应该相同
    assert_eq!(kline1.timestamp, kline2.timestamp);
    assert_ne!(kline1.symbol, kline2.symbol);
}

#[test]
fn test_replay_source_len_and_is_empty() {
    let empty_replay = ReplaySource::from_data(Vec::new());
    assert!(empty_replay.is_empty());
    assert_eq!(empty_replay.len(), 0);

    let replay = ReplaySource::from_data(create_sample_klines());
    assert!(!replay.is_empty());
    assert_eq!(replay.len(), 5);
}

#[test]
fn test_replay_source_kline_content() {
    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines);

    let kline = replay.next_kline().unwrap();

    assert_eq!(kline.symbol, "BTCUSDT");
    assert_eq!(kline.open, dec!(50000));
    assert_eq!(kline.high, dec!(50500));
    assert_eq!(kline.low, dec!(49500));
    assert_eq!(kline.close, dec!(50200));
    assert_eq!(kline.volume, dec!(100));
    assert_eq!(kline.period, Period::Minute(1));
}

#[tokio::test]
async fn test_replay_source_from_csv() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // 创建临时 CSV 文件 (包含表头)
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "symbol,period,open,high,low,close,volume,timestamp").unwrap();
    writeln!(temp_file, "BTCUSDT,1m,50000,50500,49500,50200,100,2024-01-01T00:00:00Z").unwrap();
    writeln!(temp_file, "BTCUSDT,1m,50200,51000,50100,50800,150,2024-01-01T00:01:00Z").unwrap();
    writeln!(temp_file, "BTCUSDT,1m,50800,51500,50700,51200,120,2024-01-01T00:02:00Z").unwrap();

    let path = temp_file.path();
    let replay = ReplaySource::from_csv(path).await.unwrap();

    assert_eq!(replay.len(), 3);
    assert!(!replay.is_empty());
}

#[tokio::test]
async fn test_replay_source_from_csv_multi_line() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // 创建包含多品种的 CSV 文件 (包含表头)
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "symbol,period,open,high,low,close,volume,timestamp").unwrap();
    writeln!(temp_file, "BTCUSDT,1m,50000,50500,49500,50200,100,2024-01-01T00:00:00Z").unwrap();
    writeln!(temp_file, "ETHUSDT,1m,3000,3050,2950,3020,50,2024-01-01T00:00:00Z").unwrap();
    writeln!(temp_file, "BTCUSDT,1m,50200,51000,50100,50800,150,2024-01-01T00:01:00Z").unwrap();

    let path = temp_file.path();
    let replay = ReplaySource::from_csv(path).await.unwrap();

    assert_eq!(replay.len(), 3);
}

#[test]
fn test_replay_source_with_symbols_filter() {
    let klines = create_multi_symbol_klines();
    let mut replay = ReplaySource::from_data(klines).with_symbols(vec!["BTCUSDT".to_string()]);

    // 只应该返回 BTCUSDT 的数据
    while let Some(kline) = replay.next_kline() {
        assert_eq!(kline.symbol, "BTCUSDT");
    }
}

#[test]
fn test_replay_source_with_period_filter() {
    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines).with_period(Period::Minute(5));

    // 由于样本数据都是 1m，应该没有数据返回
    assert!(replay.next_kline().is_none());
}

#[test]
fn test_replay_source_iteration_order() {
    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines);

    let mut prev_close = Decimal::ZERO;
    let mut count = 0;

    while let Some(kline) = replay.next_kline() {
        // 验证数据按时间顺序返回
        if prev_close != Decimal::ZERO {
            assert!(kline.open >= prev_close || count == 0);
        }
        prev_close = kline.close;
        count += 1;
    }

    assert_eq!(count, 5);
}

#[test]
fn test_kline_source_trait() {
    use b_data_source::replay_source::KLineSource;

    let klines = create_sample_klines();
    let mut replay = ReplaySource::from_data(klines);

    // 测试 trait 方法
    assert!(!replay.is_exhausted());

    replay.reset();
    assert!(!replay.is_exhausted());

    // 消费所有
    while replay.next_kline().is_some() {}
    assert!(replay.is_exhausted());
}
