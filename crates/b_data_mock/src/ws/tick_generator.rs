//! 流式 Tick 生成器
//!
//! 基于 K线数据，生成仿真 Tick 流。
//!
//! ## 核心算法
//!
//! ```text
//! 牛市 K线（收盘 >= 开盘）: 路径 O → L → H → C
//! 熊市 K线（收盘 < 开盘）:  路径 O → H → L → C
//! ```

use std::collections::VecDeque;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;

use crate::models::types::KLine;
use super::noise::GaussianNoise;

/// 配置参数
const TICKS_PER_1M: u8 = 60;
const PERIOD_MS: i64 = 60_000;
const TICK_INTERVAL_MS: i64 = PERIOD_MS / TICKS_PER_1M as i64;
const NOISE_FACTOR: Decimal = dec!(0.02);

/// Tick 数据结构
#[derive(Debug, Clone)]
pub struct SimulatedTick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    pub sequence_id: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub volume: Decimal,
    pub kline_timestamp: DateTime<Utc>,
    pub is_last_in_kline: bool,
}

enum GeneratorState {
    Waiting,
    Generating {
        kline: KLine,
        tick_index: u8,
        price_path: VecDeque<Decimal>,
        accumulated_high: Decimal,
        accumulated_low: Decimal,
        volume_per_tick: Decimal,
    },
}

/// 流式 Tick 生成器
pub struct StreamTickGenerator {
    symbol: String,
    kline_iter: Box<dyn Iterator<Item = KLine> + Send>,
    state: GeneratorState,
    noise: GaussianNoise,
    qty: Decimal,
    sequence_counter: u64,
}

impl StreamTickGenerator {
    pub fn new(symbol: String, kline_iter: Box<dyn Iterator<Item = KLine> + Send>) -> Self {
        Self {
            symbol,
            kline_iter,
            state: GeneratorState::Waiting,
            noise: GaussianNoise::new(),
            qty: dec!(0.01),
            sequence_counter: 0,
        }
    }

    pub fn from_loader(
        symbol: String,
        loader: impl Iterator<Item = KLine> + Send + 'static,
    ) -> Self {
        Self::new(symbol, Box::new(loader))
    }

    pub fn with_qty(mut self, qty: Decimal) -> Self {
        self.qty = qty;
        self
    }

    fn load_next_kline(&mut self) -> Option<()> {
        if let Some(kline) = self.kline_iter.next() {
            let price_path = self.generate_price_path(
                kline.open,
                kline.high,
                kline.low,
                kline.close,
            );

            self.state = GeneratorState::Generating {
                kline,
                tick_index: 0,
                price_path,
                accumulated_high: Decimal::ZERO,
                accumulated_low: Decimal::MAX,
                volume_per_tick: Decimal::ZERO,
            };
            Some(())
        } else {
            self.state = GeneratorState::Waiting;
            None
        }
    }

    fn generate_price_path(
        &mut self,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
    ) -> VecDeque<Decimal> {
        let is_bullish = close >= open;

        let nodes = if is_bullish {
            [open, low, high, close]
        } else {
            [open, high, low, close]
        };

        let dist = [
            (nodes[1] - nodes[0]).abs(),
            (nodes[2] - nodes[1]).abs(),
            (nodes[3] - nodes[2]).abs(),
        ];

        let total_dist: Decimal = dist[0] + dist[1] + dist[2];

        let ticks_per_seg: [u8; 3] = if total_dist.is_zero() {
            [20, 20, 20]
        } else {
            let seg0 = (dist[0] / total_dist * Decimal::from(TICKS_PER_1M))
                .to_u8().unwrap_or(20);
            let seg1 = (dist[1] / total_dist * Decimal::from(TICKS_PER_1M))
                .to_u8().unwrap_or(20);
            let seg2 = (dist[2] / total_dist * Decimal::from(TICKS_PER_1M))
                .to_u8().unwrap_or(20);

            [
                seg0.max(2),
                seg1.max(2),
                seg2.max(2),
            ]
        };

        let mut ticks = ticks_per_seg;
        while ticks.iter().map(|x| *x as u16).sum::<u16>() < TICKS_PER_1M as u16 {
            let max_idx = if dist[0] >= dist[1] && dist[0] >= dist[2] {
                0
            } else if dist[1] >= dist[2] {
                1
            } else {
                2
            };
            ticks[max_idx] += 1;
        }
        while ticks.iter().map(|x| *x as u16).sum::<u16>() > TICKS_PER_1M as u16 {
            let max_idx = ticks.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.cmp(b))
                .map(|(i, _)| i)
                .unwrap_or(0);
            if ticks[max_idx] > 2 {
                ticks[max_idx] -= 1;
            } else {
                break;
            }
        }

        let mut path = VecDeque::new();
        let range = high - low;
        let noise_scale_factor = range * NOISE_FACTOR;

        for (seg_idx, &n) in ticks.iter().enumerate() {
            let start_p = nodes[seg_idx];
            let end_p = nodes[seg_idx + 1];

            for i in 0..n {
                let t = if n == 1 {
                    dec!(0.5)
                } else {
                    Decimal::from(i) / Decimal::from(n - 1)
                };

                let price = start_p + (end_p - start_p) * t;

                let noise = if i == 0 || i == n - 1 {
                    Decimal::ZERO
                } else {
                    let z = self.noise.sample_with_params(0.0, 1.0);
                    let offset = (noise_scale_factor.to_f64().unwrap_or(0.0) * z * 0.5)
                        .to_string().parse::<Decimal>().unwrap_or(Decimal::ZERO);
                    offset
                };

                path.push_back(price + noise);
            }
        }

        while path.len() > TICKS_PER_1M as usize {
            path.pop_back();
        }
        while path.len() < TICKS_PER_1M as usize {
            path.push_back(close);
        }

        path
    }
}

impl Iterator for StreamTickGenerator {
    type Item = SimulatedTick;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.state {
                GeneratorState::Waiting => {
                    if self.load_next_kline().is_none() {
                        return None;
                    }
                }
                GeneratorState::Generating {
                    kline,
                    tick_index,
                    price_path,
                    accumulated_high,
                    accumulated_low,
                    volume_per_tick,
                } => {
                    if *tick_index >= TICKS_PER_1M {
                        self.load_next_kline();
                        continue;
                    }

                    let price = price_path.pop_front()?;

                    if price > *accumulated_high {
                        *accumulated_high = price;
                    }
                    if price < *accumulated_low {
                        *accumulated_low = price;
                    }

                    let kline_ts = kline.timestamp;
                    let tick_offset_ms = (*tick_index as i64) * TICK_INTERVAL_MS;
                    let tick_ts = kline_ts + Duration::milliseconds(tick_offset_ms);

                    if volume_per_tick.is_zero() && !kline.volume.is_zero() {
                        *volume_per_tick = kline.volume / Decimal::from(TICKS_PER_1M);
                    }

                    let is_last_in_kline = *tick_index + 1 >= TICKS_PER_1M;

                    *tick_index += 1;
                    self.sequence_counter += 1;

                    return Some(SimulatedTick {
                        symbol: self.symbol.clone(),
                        price,
                        qty: self.qty,
                        timestamp: tick_ts,
                        sequence_id: self.sequence_counter,
                        open: kline.open,
                        high: *accumulated_high,
                        low: *accumulated_low,
                        volume: *volume_per_tick,
                        kline_timestamp: kline_ts,
                        is_last_in_kline,
                    });
                }
            }
        }
    }
}
