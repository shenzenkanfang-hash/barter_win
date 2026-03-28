//! KLineSynthesizer 测试 - Tick 聚合为 K线
//!
//! 按模块组织的功能测试:
//! 模块: KLineSynthesizer (K线合成器)
//! 功能: 将 Tick 数据聚合为指定周期的 K线

use b_data_mock::{Period, Tick, KLineSynthesizer};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn create_tick(price: Decimal, timestamp: DateTime<Utc>) -> Tick {
    Tick {
        symbol: "BTCUSDT".to_string(),
        price,
        qty: dec!(0.1),
        timestamp,
        sequence_id: 0,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    }
}

fn minute_timestamp(minutes: i64) -> DateTime<Utc> {
    // 分钟数 -> 该分钟开始的时间戳（秒 = minutes * 60）
    // Period::Minute(1): 每 60 秒一个周期
    // ts=0 -> 00:00:00 (秒 0-59 属于分钟 0)
    // ts=1 -> 00:01:00 (秒 60-119 属于分钟 1)
    let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    base + chrono::Duration::minutes(minutes)
}

fn second_timestamp(seconds: i64) -> DateTime<Utc> {
    let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    base + chrono::Duration::seconds(seconds)
}

fn create_tick_secs(price: Decimal, seconds: i64) -> Tick {
    Tick {
        symbol: "BTCUSDT".to_string(),
        price,
        qty: dec!(0.1),
        timestamp: second_timestamp(seconds),
        sequence_id: 0,
        kline_1m: None,
        kline_15m: None,
        kline_1d: None,
    }
}

// ============================================================================
// 模块验证报告: KLineSynthesizer
// ============================================================================
// 职责: 将 Tick 数据聚合为指定周期的 K线
// 依赖: models::KLine, models::Period, models::Tick
// 前置: 无依赖，独立可测试
// ============================================================================

#[test]
fn test_synthesizer_new() {
    // 验证: new() 创建空合成器
    let synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    assert!(synth.current_kline().is_none());
}

#[test]
fn test_first_tick_creates_kline() {
    // 验证: 第一个 Tick 创建当前 K 线，不返回完成的 K线
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    let tick = create_tick_secs(dec!(50000.0), 0);
    let completed = synth.update(&tick);
    assert!(completed.is_none(), "第一个 Tick 不应返回已完成的 K线");
    let current = synth.current_kline().unwrap();
    assert_eq!(current.open, dec!(50000.0));
    assert_eq!(current.close, dec!(50000.0));
    assert_eq!(current.high, dec!(50000.0));
    assert_eq!(current.low, dec!(50000.0));
}

#[test]
fn test_tick_updates_current_kline() {
    // 验证: 同一周期内的多个 Tick 更新当前 K 线（不创建新 K 线）
    // Period::Minute(1): 秒 0-59 属于同一周期
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    // 第 0 秒 Tick
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    // 第 30 秒 Tick（仍在同一 1 分钟周期内）
    synth.update(&create_tick_secs(dec!(50100.0), 30));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.open, dec!(50000.0));
    assert_eq!(current.close, dec!(50100.0));
    assert_eq!(current.high, dec!(50100.0));  // 最高价更新
    assert_eq!(current.low, dec!(50000.0));   // 最低价不变
}

#[test]
fn test_new_period_completes_kline() {
    // 验证: 新周期开始时，完成当前 K 线并返回
    // Period::Minute(1): 秒 0-59 = 分钟 0，秒 60-119 = 分钟 1
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    // 第 0 秒 Tick（分钟 0）
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    // 第 60 秒 Tick（分钟 1，开始新周期）-> 触发前一 K 线完成
    let completed = synth.update(&create_tick_secs(dec!(50100.0), 60));

    assert!(completed.is_some(), "新周期开始应返回已完成的 K线");
    let completed_kline = completed.unwrap();
    assert_eq!(completed_kline.open, dec!(50000.0));
    assert_eq!(completed_kline.close, dec!(50000.0));  // 最后一个成交价来自第 0 秒
    assert_eq!(completed_kline.high, dec!(50000.0));
    assert_eq!(completed_kline.low, dec!(50000.0));

    // 当前 K 线重新创建（分钟 1）
    let current = synth.current_kline().unwrap();
    assert_eq!(current.open, dec!(50100.0));
    assert_eq!(current.close, dec!(50100.0));
}

#[test]
fn test_volume_accumulates() {
    // 验证: 同一周期内 Tick 成交量累加
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    synth.update(&create_tick_secs(dec!(50100.0), 30));

    let current = synth.current_kline().unwrap();
    // 每个 Tick qty = 0.1，两个 Tick = 0.2
    assert_eq!(current.volume, dec!(0.2), "同一周期内成交量应累加");
}

#[test]
fn test_high_low_tracking() {
    // 验证: 价格走势跟踪 high/low
    // 秒 0, 15, 30, 45 全部在同一 1 分钟周期内
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    synth.update(&create_tick_secs(dec!(50200.0), 15));
    synth.update(&create_tick_secs(dec!(49900.0), 30));
    synth.update(&create_tick_secs(dec!(50100.0), 45));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.high, dec!(50200.0), "最高价应为 50200");
    assert_eq!(current.low, dec!(49900.0), "最低价应为 49900");
    assert_eq!(current.close, dec!(50100.0), "收盘价应为最后成交价");
}

#[test]
fn test_5min_period() {
    // 验证: Period::Minute(5) 正确分组
    // 分钟 0-4 (秒 0-299) 属于同一 5 分钟周期
    // 分钟 5 (秒 300+) 开始新周期
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(5));
    // 分钟 0
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    // 分钟 4 (秒 240)
    synth.update(&create_tick_secs(dec!(50100.0), 240));

    assert!(synth.current_kline().is_some(), "分钟 0-4 应在同一个 5 分钟周期");

    // 分钟 5 开始（秒 300），触发前一 K 线完成
    let completed = synth.update(&create_tick_secs(dec!(50200.0), 300));
    assert!(completed.is_some(), "分钟 5 应开始新周期并完成前一 K 线");
}

#[test]
fn test_day_period() {
    // 验证: Period::Day 按日期分组
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Day);
    let day_start = Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap();  // 第1小时
    let midday = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();     // 第12小时
    synth.update(&create_tick(dec!(50000.0), day_start));
    synth.update(&create_tick(dec!(50100.0), midday));

    let current = synth.current_kline().unwrap();
    assert_eq!(current.high, dec!(50100.0));
    assert_eq!(current.low, dec!(50000.0));
}

#[test]
fn test_period_boundary_1min() {
    // 验证: Period::Minute(1) 周期边界（通过行为测试，非直接调用）
    // 分钟 0 (秒 0-59) 和 分钟 1 (秒 60+) 在不同周期
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    synth.update(&create_tick_secs(dec!(50200.0), 30)); // 同周期，high 更新
    // 秒 59 仍在分钟 0
    synth.update(&create_tick_secs(dec!(50300.0), 59));
    // 秒 60 开始分钟 1，新周期，完成前一 K 线
    let completed = synth.update(&create_tick_secs(dec!(50400.0), 60));

    assert!(completed.is_some(), "秒 60 应开始新周期，完成前一 K 线");
    assert_eq!(completed.unwrap().high, dec!(50300.0));
}

#[test]
fn test_period_boundary_5min() {
    // 验证: Period::Minute(5) 周期边界
    // 分钟 0-4 (秒 0-299) 同周期，分钟 5 (秒 300+) 新周期
    let mut synth = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(5));
    synth.update(&create_tick_secs(dec!(50000.0), 0));
    synth.update(&create_tick_secs(dec!(50200.0), 299)); // 分钟 4，最后一秒
    let completed = synth.update(&create_tick_secs(dec!(50300.0), 300)); // 分钟 5

    assert!(completed.is_some(), "秒 300 应开始新周期");
}
