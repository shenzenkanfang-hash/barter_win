//! 模拟 1d K线 WebSocket
//!
//! 从 1m K线聚合生成日K线

use std::collections::HashMap;
use std::sync::Arc;

use crate::store::MarketDataStoreImpl;
use crate::store::store_trait::MarketDataStore;
use crate::ws::kline_1m::KlineData;
use crate::models::{KLine, Period};

/// 模拟日K线流管理器
pub struct Kline1dStream {
    /// 共享存储
    store: Arc<MarketDataStoreImpl>,
    /// 日K线合成器
    synthesizers: HashMap<String, KLineSynthesizerDay>,
}

/// 日K线合成器
struct KLineSynthesizerDay {
    symbol: String,
    current_open: Option<rust_decimal::Decimal>,
    current_high: rust_decimal::Decimal,
    current_low: rust_decimal::Decimal,
    current_close: rust_decimal::Decimal,
    volume: rust_decimal::Decimal,
    day_start_ms: i64,
    last_kline_end: i64,
}

impl Kline1dStream {
    pub fn new(store: Arc<MarketDataStoreImpl>) -> Self {
        Self {
            store,
            synthesizers: HashMap::new(),
        }
    }

    /// 从 1m K线更新日K线
    pub fn update_from_kline(&mut self, kline: &KlineData) {
        use rust_decimal::Decimal;

        let symbol = kline.symbol.clone();

        let open = kline.open.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let high = kline.high.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let low = kline.low.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let close = kline.close.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let volume = kline.volume.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let start_time = kline.kline_start_time;
        let close_time = kline.kline_close_time;

        let synth = self.synthesizers
            .entry(symbol.clone())
            .or_insert_with(|| KLineSynthesizerDay {
                symbol: symbol.clone(),
                current_open: None,
                current_high: Decimal::ZERO,
                current_low: Decimal::MAX,
                current_close: Decimal::ZERO,
                volume: Decimal::ZERO,
                day_start_ms: start_time,
                last_kline_end: 0,
            });

        // 检查是否是新的交易日
        let day_start = start_time - (start_time % (24 * 60 * 60 * 1000));

        if synth.day_start_ms != day_start {
            // 新的一天，重置
            synth.current_open = Some(open);
            synth.current_high = high;
            synth.current_low = low;
            synth.current_close = close;
            synth.volume = volume;
            synth.day_start_ms = day_start;
        } else {
            // 继续累积
            if synth.current_open.is_none() {
                synth.current_open = Some(open);
            }
            synth.current_high = synth.current_high.max(high);
            synth.current_low = synth.current_low.min(low);
            synth.current_close = close;
            synth.volume += volume;
        }

        synth.last_kline_end = close_time;

        // K线闭合时，写入日K线
        if kline.is_closed {
            if let (Some(o), Some(c)) = (synth.current_open, Some(synth.current_close)) {
                let _day_kline = KLine {
                    symbol: symbol.clone(),
                    period: Period::Day,
                    open: o,
                    high: synth.current_high,
                    low: synth.current_low,
                    close: c,
                    volume: synth.volume,
                    timestamp: chrono::DateTime::from_timestamp_millis(day_start)
                        .expect("日K线时间戳无效"),
                    is_closed: true,
                };

                // 写入存储
                let kline_data = KlineData {
                    kline_start_time: day_start,
                    kline_close_time: close_time,
                    symbol: symbol.clone(),
                    interval: "1d".to_string(),
                    open: o.to_string(),
                    close: c.to_string(),
                    high: synth.current_high.to_string(),
                    low: synth.current_low.to_string(),
                    volume: synth.volume.to_string(),
                    is_closed: true,
                };

                self.store.write_kline(&symbol, kline_data, true);
            }
        }
    }

    /// 获取存储引用
    pub fn store(&self) -> &Arc<MarketDataStoreImpl> {
        &self.store
    }
}
