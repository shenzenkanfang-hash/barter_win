use crate::models::types::{KLine, Period, Tick};
use chrono::{DateTime, Utc};

/// K线合成器
///
/// 将 Tick 数据聚合为指定周期的 K线。
/// 支持 O(1) 增量更新，每次 tick 只更新当前 K线。
///
/// # 泛型参数
/// 无，使用 Period 枚举区分周期
///
/// # 示例
/// ```
/// use b_data_source::{KLineSynthesizer, Period};
/// let mut synthesizer = KLineSynthesizer::new("BTCUSDT".to_string(), Period::Minute(1));
/// ```
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
                let minutes = (timestamp.timestamp() / 60 / m as i64) * 60 * m as i64;
                DateTime::from_timestamp(minutes as i64, 0).unwrap()
            }
            Period::Day => {
                let days = timestamp.date_naive().and_hms_opt(0, 0, 0).unwrap();
                DateTime::<Utc>::from_naive_utc_and_offset(days, Utc)
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
        }
    }
}
