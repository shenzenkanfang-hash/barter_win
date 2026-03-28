//! K线合成器
//!
//! 将 Tick 数据聚合为指定周期的 K线。
//! 复制自 b_data_source::ws::kline_1m::kline

use crate::models::{KLine, Period, Tick};
use chrono::{DateTime, TimeZone, Utc};

/// K线合成器
pub struct KLineSynthesizer {
    pub symbol: String,
    pub period: Period,
    current: Option<KLine>,
}

impl KLineSynthesizer {
    pub fn new(symbol: String, period: Period) -> Self {
        Self {
            symbol,
            period,
            current: None,
        }
    }

    pub fn current_kline(&self) -> Option<&KLine> {
        self.current.as_ref()
    }

    pub fn update(&mut self, tick: &Tick) -> Option<KLine> {
        let kline_timestamp = self.period_start(tick.timestamp);

        match &mut self.current {
            Some(kline) if kline.timestamp == kline_timestamp => {
                kline.high = kline.high.max(tick.price);
                kline.low = kline.low.min(tick.price);
                kline.close = tick.price;
                kline.volume += tick.qty;
                None
            }
            Some(kline) => {
                let completed = kline.clone();
                self.current = Some(self.new_kline(tick, kline_timestamp));
                Some(completed)
            }
            None => {
                self.current = Some(self.new_kline(tick, kline_timestamp));
                None
            }
        }
    }

    fn period_start(&self, timestamp: DateTime<Utc>) -> DateTime<Utc> {
        match self.period {
            Period::Minute(m) => {
                let ts = timestamp.timestamp();
                // 正确公式：(ts / (m*60)) * (m*60) = 地板到 m 分钟周期的起点
                // ts=0 -> 周期0起点 00:00:00
                // ts=30 -> 周期0起点 00:00:00（仍在同一1分钟周期内）
                // ts=60 -> 周期1起点 00:01:00
                let m_secs = m as i64 * 60;
                let period_secs = (ts / m_secs) * m_secs;
                timestamp + chrono::Duration::seconds(period_secs - ts)
            }
            Period::Day => {
                let naive = timestamp.date_naive();
                Utc.from_utc_datetime(&naive.and_hms_opt(0, 0, 0).unwrap())
            }
        }
    }

    fn new_kline(&self, tick: &Tick, timestamp: DateTime<Utc>) -> KLine {
        KLine {
            symbol: self.symbol.clone(),
            period: self.period,
            open: tick.price,
            high: tick.price,
            low: tick.price,
            close: tick.price,
            volume: tick.qty,
            timestamp,
            is_closed: false,
        }
    }
}
