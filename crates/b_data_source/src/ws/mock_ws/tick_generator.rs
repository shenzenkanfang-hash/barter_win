//! 流式 Tick 生成器
//!
//! 基于 K线数据，生成仿真 Tick 流。
//!
//! ## 核心算法（1:1 还原 Python）
//!
//! ```text
//! 牛市 K线（收盘 >= 开盘）: 路径 O → L → H → C
//! 熊市 K线（收盘 < 开盘）:  路径 O → H → L → C
//! ```
//!
//! 每段价格根据距离比例分配 Tick 数量：
//! ```text
//! dist[0] = |O-L|, dist[1] = |L-H|, dist[2] = |H-C|
//! total = sum(dist)
//! ticks_per_seg = dist / total * 60
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
const PERIOD_MS: i64 = 60_000; // 1m = 60s = 60000ms
const TICK_INTERVAL_MS: i64 = PERIOD_MS / TICKS_PER_1M as i64; // ~16ms
const NOISE_FACTOR: Decimal = dec!(0.02); // 2% 噪声因子

/// Tick 数据结构
#[derive(Debug, Clone)]
pub struct SimulatedTick {
    /// 交易对
    pub symbol: String,
    /// 价格
    pub price: Decimal,
    /// 数量
    pub qty: Decimal,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 序列号（用于幂等性去重）
    pub sequence_id: u64,
    /// 所属 1m K线开盘价
    pub open: Decimal,
    /// 累积最高价
    pub high: Decimal,
    /// 累积最低价
    pub low: Decimal,
    /// 分摊成交量
    pub volume: Decimal,
    /// 所属 1m K线时间戳
    pub kline_timestamp: DateTime<Utc>,
    /// 是否为当前 K 线的最后一根 Tick（由生成器内部判断）
    pub is_last_in_kline: bool,
}

/// Tick 生成器状态
#[derive(Debug, Clone)]
enum GeneratorState {
    /// 等待下一根 K线
    Waiting,
    /// 正在生成当前 K线 的 Tick
    Generating {
        /// 当前 K线
        kline: KLine,
        /// Tick 索引 (0-59)
        tick_index: u8,
        /// 预处理的价格路径
        price_path: VecDeque<Decimal>,
        /// 累积最高价
        accumulated_high: Decimal,
        /// 累积最低价
        accumulated_low: Decimal,
        /// 每 Tick 成交量
        volume_per_tick: Decimal,
    },
}

/// 流式 Tick 生成器
///
/// 使用 Iterator 模式，按需生成 Tick。
pub struct StreamTickGenerator {
    /// 交易对
    symbol: String,
    /// K线迭代器
    kline_iter: Box<dyn Iterator<Item = KLine> + Send>,
    /// 当前状态
    state: GeneratorState,
    /// 噪声生成器
    noise: GaussianNoise,
    /// 固定 qty（可配置）
    qty: Decimal,
    /// 全局序列号（用于幂等性去重）
    sequence_counter: u64,
}

impl StreamTickGenerator {
    /// 创建生成器
    pub fn new(
        symbol: String,
        kline_iter: Box<dyn Iterator<Item = KLine> + Send>,
    ) -> Self {
        Self {
            symbol,
            kline_iter,
            state: GeneratorState::Waiting,
            noise: GaussianNoise::new(),
            qty: dec!(0.01), // 默认固定数量
            sequence_counter: 0,
        }
    }

    /// 创建生成器（从任意 K线迭代器）
    pub fn from_loader(
        symbol: String,
        loader: impl Iterator<Item = KLine> + Send + 'static,
    ) -> Self {
        Self::new(symbol, Box::new(loader))
    }

    /// 设置固定 qty
    pub fn with_qty(mut self, qty: Decimal) -> Self {
        self.qty = qty;
        self
    }

    /// 内部：加载下一根 K线
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

    /// 生成单根 K线的价格路径
    ///
    /// 牛市: open → low → high → close
    /// 熊市: open → high → low → close
    fn generate_price_path(
        &mut self,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
    ) -> VecDeque<Decimal> {
        let is_bullish = close >= open;

        // 确定路径节点
        let nodes = if is_bullish {
            [open, low, high, close]
        } else {
            [open, high, low, close]
        };

        // 计算每段距离
        let dist = [
            (nodes[1] - nodes[0]).abs(),
            (nodes[2] - nodes[1]).abs(),
            (nodes[3] - nodes[2]).abs(),
        ];

        let total_dist: Decimal = dist[0] + dist[1] + dist[2];

        // 计算每段 tick 数量
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

        // 调整总数为 60
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

        // 生成价格序列
        let mut path = VecDeque::new();
        let range = high - low;
        let noise_scale = range * NOISE_FACTOR;

        for (seg_idx, &n) in ticks.iter().enumerate() {
            let start_p = nodes[seg_idx];
            let end_p = nodes[seg_idx + 1];

            for i in 0..n {
                let t = if n == 1 {
                    dec!(0.5)
                } else {
                    Decimal::from(i) / Decimal::from(n - 1)
                };

                // 线性插值
                let price = start_p + (end_p - start_p) * t;

                // 端点不加噪声
                let noise = if i == 0 || i == n - 1 {
                    Decimal::ZERO
                } else {
                    let z = self.noise.sample_with_params(0.0, 1.0);
                    let offset = (noise_scale.to_f64().unwrap_or(0.0) * z * 0.5)
                        .to_string().parse::<Decimal>().unwrap_or(Decimal::ZERO);
                    offset
                };

                path.push_back(price + noise);
            }
        }

        // 截取/填充正好 60 个
        while path.len() > TICKS_PER_1M as usize {
            path.pop_back();
        }
        while path.len() < TICKS_PER_1M as usize {
            path.push_back(close);
        }

        path
    }
}

/// 流式迭代器实现
impl Iterator for StreamTickGenerator {
    type Item = SimulatedTick;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.state {
                GeneratorState::Waiting => {
                    // 尝试加载下一根 K线
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
                    // 如果当前 K线已经发完，加载下一根
                    if *tick_index >= TICKS_PER_1M {
                        self.load_next_kline();
                        continue;
                    }

                    // 获取价格
                    let price = price_path.pop_front()?;

                    // 更新累积高低
                    if price > *accumulated_high {
                        *accumulated_high = price;
                    }
                    if price < *accumulated_low {
                        *accumulated_low = price;
                    }

                    // 计算时间戳
                    let kline_ts = kline.timestamp;
                    let tick_offset_ms = (*tick_index as i64) * TICK_INTERVAL_MS;
                    let tick_ts = kline_ts + Duration::milliseconds(tick_offset_ms);

                    // 计算 volume_per_tick（如果还没计算）
                    if volume_per_tick.is_zero() && !kline.volume.is_zero() {
                        *volume_per_tick = kline.volume / Decimal::from(TICKS_PER_1M);
                    }

                    // 判断是否为当前 K 线最后一根 Tick
                    // 注意：此时 tick_index 还未 +1，是当前 tick 的索引（0-59）
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_klines() -> Vec<KLine> {
        vec![
            KLine {
                symbol: "BTCUSDT".to_string(),
                period: b_data_source::Period::Minute(1),
                open: dec!(50000),
                high: dec!(50100),
                low: dec!(49900),
                close: dec!(50050),
                volume: dec!(100),
                timestamp: Utc::now(),
            },
            KLine {
                symbol: "BTCUSDT".to_string(),
                period: b_data_source::Period::Minute(1),
                open: dec!(50050),
                high: dec!(50200),
                low: dec!(50000),
                close: dec!(50150),
                volume: dec!(120),
                timestamp: Utc::now() + Duration::minutes(1),
            },
        ]
    }

    #[test]
    fn test_generate_tick() {
        let klines = create_test_klines();
        let mut generator = StreamTickGenerator::new(
            "BTCUSDT".to_string(),
            Box::new(klines.into_iter()),
        );

        let tick = generator.next();
        assert!(tick.is_some());

        let tick = tick.unwrap();
        assert_eq!(tick.symbol, "BTCUSDT");
        assert!(tick.price >= dec!(49900));
        assert!(tick.price <= dec!(50100));
    }

    #[test]
    fn test_all_ticks_exhausted() {
        let klines = create_test_klines();
        let mut generator = StreamTickGenerator::new(
            "BTCUSDT".to_string(),
            Box::new(klines.into_iter()),
        );

        // 2根K线 * 60 ticks = 120 ticks
        for i in 0..120 {
            let tick = generator.next();
            assert!(tick.is_some(), "Should have tick {}", i);
        }

        // 再请求应该返回 None
        let tick = generator.next();
        assert!(tick.is_none());
    }

    /// 验证每根 K 线第 60 根 tick 的 is_last_in_kline = true
    #[test]
    fn test_last_tick_in_kline_is_closed() {
        let klines = create_test_klines();
        let mut generator = StreamTickGenerator::new(
            "BTCUSDT".to_string(),
            Box::new(klines.into_iter()),
        );

        for tick_idx in 0..120 {
            let tick = generator.next().expect("Should have tick");
            let expected_kline_idx = tick_idx / 60; // 0 或 1
            let expected_pos_in_kline = tick_idx % 60; // 0-59
            let expected_is_last = expected_pos_in_kline == 59;

            assert_eq!(
                tick.is_last_in_kline, expected_is_last,
                "tick {} (kline={}, pos={}): is_last_in_kline should be {}, got {}",
                tick_idx, expected_kline_idx, expected_pos_in_kline,
                expected_is_last, tick.is_last_in_kline
            );
        }
    }
}
