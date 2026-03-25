//! TickGenerator - 从 1m K线生成模拟 Tick 数据
//!
//! 移植自 Python historical_retracement.py 的 TickGenerator
//! 每根 1m K线生成 60 个 tick，包含累积 OHLC 路径

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 配置
const TICKS_PER_1M: u8 = 60;
const PERIOD_MS: i64 = 60_000; // 1m = 60s = 60000ms
const TICK_INTERVAL_MS: i64 = PERIOD_MS / TICKS_PER_1M as i64; // ~16ms

/// SimulatedTick - 模拟 Tick 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedTick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    pub open: Decimal,          // 该 1m 的开盘价（固定）
    pub high: Decimal,          // 累积最高
    pub low: Decimal,          // 累积最低
    pub volume: Decimal,        // 分摊成交量
    pub kline_timestamp: DateTime<Utc>, // 所属 1m K线时间
}

/// K线输入（用于生成 tick）
#[derive(Debug, Clone)]
pub struct KLineInput {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
}

impl KLineInput {
    /// 从 b_data_source::KLine 转换
    pub fn from_kline(k: &b_data_source::KLine) -> Self {
        Self {
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            timestamp: k.timestamp,
        }
    }
}

/// TickGenerator - 从 1m K线生成模拟 Tick 数据
pub struct TickGenerator {
    symbol: String,
    klines: VecDeque<KLineInput>,
    current_kline: Option<KLineInput>,
    tick_index: u8, // 0-59
    accumulated_high: Decimal,
    accumulated_low: Decimal,
    price_path: VecDeque<Decimal>, // 预处理的价格路径
    volume_per_tick: Decimal,
}

impl TickGenerator {
    /// 创建生成器
    pub fn new(symbol: String, klines: Vec<KLineInput>) -> Self {
        let klines = VecDeque::from(klines);
        Self {
            symbol,
            klines,
            current_kline: None,
            tick_index: TICKS_PER_1M, // 初始状态，触发加载下一根 K线
            accumulated_high: Decimal::ZERO,
            accumulated_low: Decimal::MAX,
            price_path: VecDeque::new(),
            volume_per_tick: Decimal::ZERO,
        }
    }

    /// 创建生成器（从 b_data_source::KLine 列表）
    pub fn from_klines(symbol: String, klines: Vec<b_data_source::KLine>) -> Self {
        let kline_inputs: Vec<KLineInput> = klines
            .iter()
            .map(KLineInput::from_kline)
            .collect();
        Self::new(symbol, kline_inputs)
    }

    /// 生成下一个 tick
    pub fn next_tick(&mut self) -> Option<SimulatedTick> {
        // 如果当前 K线已经发完，加载下一根
        if self.tick_index >= TICKS_PER_1M {
            self.load_next_kline()?;
        }

        // 获取价格
        let price = self.price_path.pop_front()?;

        // 更新累积高低
        if price > self.accumulated_high {
            self.accumulated_high = price;
        }
        if price < self.accumulated_low {
            self.accumulated_low = price;
        }

        // 计算时间戳
        let kline_ts = self.current_kline.as_ref().unwrap().timestamp;
        let tick_offset_ms = (self.tick_index as i64) * TICK_INTERVAL_MS;
        let tick_ts = kline_ts + Duration::milliseconds(tick_offset_ms);

        self.tick_index += 1;

        Some(SimulatedTick {
            symbol: self.symbol.clone(),
            price,
            qty: dec!(0.01), // 固定数量，可配置
            timestamp: tick_ts,
            open: self.current_kline.as_ref().unwrap().open,
            high: self.accumulated_high,
            low: self.accumulated_low,
            volume: self.volume_per_tick,
            kline_timestamp: kline_ts,
        })
    }

    /// 检查是否还有数据
    pub fn is_exhausted(&self) -> bool {
        self.current_kline.is_none() && self.klines.is_empty()
    }

    /// 获取已发送的 tick 总数
    pub fn tick_count(&self) -> u64 {
        let klines_sent = if self.current_kline.is_some() {
            0
        } else {
            self.klines.len() as u64
        };
        klines_sent * TICKS_PER_1M as u64 + self.tick_index as u64
    }

    /// 获取总 K线数
    pub fn total_klines(&self) -> usize {
        self.klines.len() + (self.current_kline.is_some() as usize)
    }

    /// 获取当前 K线剩余 tick 数
    pub fn remaining_in_current_kline(&self) -> u8 {
        if self.tick_index >= TICKS_PER_1M {
            0
        } else {
            TICKS_PER_1M - self.tick_index
        }
    }

    /// 加载下一根 K线
    fn load_next_kline(&mut self) -> Option<()> {
        self.current_kline = Some(self.klines.pop_front()?);

        let kline = self.current_kline.as_ref().unwrap();

        // 重置状态
        self.tick_index = 0;
        self.accumulated_high = kline.high;
        self.accumulated_low = kline.low;

        // 计算每 tick 成交量
        self.volume_per_tick = kline.volume / Decimal::from(TICKS_PER_1M);

        // 生成价格路径
        self.price_path = Self::generate_price_path(
            kline.open,
            kline.high,
            kline.low,
            kline.close,
        );

        Some(())
    }

    /// 生成价格路径
    ///
    /// 牛市: open → low → high → close
    /// 熊市: open → high → low → close
    fn generate_price_path(
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

        // 计算每段距离（取绝对值）
        let dist = [
            (nodes[1] - nodes[0]).abs(),
            (nodes[2] - nodes[1]).abs(),
            (nodes[3] - nodes[2]).abs(),
        ];

        let total_dist: Decimal = dist[0] + dist[1] + dist[2];

        // 计算每段 tick 数量
        let ticks_per_seg: [u8; 3] = if total_dist.is_zero() {
            // 无波动，均分
            [20, 20, 20]
        } else {
            let seg0 = (dist[0] / total_dist * Decimal::from(TICKS_PER_1M)).to_u8().unwrap_or(20);
            let seg1 = (dist[1] / total_dist * Decimal::from(TICKS_PER_1M)).to_u8().unwrap_or(20);
            let seg2 = (dist[2] / total_dist * Decimal::from(TICKS_PER_1M)).to_u8().unwrap_or(20);

            [
                seg0.max(2),
                seg1.max(2),
                seg2.max(2),
            ]
        };

        // 调整总数为 60
        let mut ticks = ticks_per_seg;
        while ticks.iter().map(|x| *x as u16).sum::<u16>() < TICKS_PER_1M as u16 {
            // 找最大距离的段加 tick
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
            // 找最大段减 tick（但不小于2）
            let max_idx = ticks
                .iter()
                .enumerate()
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
        let noise_scale = range * dec!(0.02); // 2% 噪声范围

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
                    // 简化：使用固定小噪声（实际可用随机数）
                    // 这里简单加一个微小偏移
                    let offset = if i % 3 == 0 {
                        noise_scale * dec!(0.5)
                    } else if i % 3 == 1 {
                        -noise_scale * dec!(0.5)
                    } else {
                        Decimal::ZERO
                    };
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

impl Default for TickGenerator {
    fn default() -> Self {
        Self::new(String::new(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_klines() -> Vec<KLineInput> {
        vec![
            KLineInput {
                open: dec!(50000),
                high: dec!(50100),
                low: dec!(49900),
                close: dec!(50050),
                volume: dec!(100),
                timestamp: Utc::now(),
            },
            KLineInput {
                open: dec!(50050),
                high: dec!(50200),
                low: dec!(50000),
                close: dec!(50150),
                volume: dec!(120),
                timestamp: Utc::now() + chrono::Duration::minutes(1),
            },
        ]
    }

    #[test]
    fn test_generator_creation() {
        let klines = create_test_klines();
        let g = TickGenerator::new("BTCUSDT".to_string(), klines);

        assert_eq!(g.total_klines(), 2);
        assert!(!g.is_exhausted());
    }

    #[test]
    fn test_generate_tick() {
        let klines = create_test_klines();
        let mut g = TickGenerator::new("BTCUSDT".to_string(), klines);

        let tick = g.next_tick();
        assert!(tick.is_some());

        let tick = tick.unwrap();
        assert_eq!(tick.symbol, "BTCUSDT");
        assert!(tick.price >= dec!(49900));
        assert!(tick.price <= dec!(50100));
    }

    #[test]
    fn test_all_ticks_exhausted() {
        let klines = create_test_klines();
        let mut g = TickGenerator::new("BTCUSDT".to_string(), klines);

        // 2根K线 * 60 ticks = 120 ticks
        for _ in 0..120 {
            let tick = g.next_tick();
            assert!(tick.is_some(), "Should have tick");
        }

        // 再请求应该返回 None
        let tick = g.next_tick();
        assert!(tick.is_none());
        assert!(g.is_exhausted());
    }

    #[test]
    fn test_price_path_bullish() {
        let path = TickGenerator::generate_price_path(
            dec!(50000), // open
            dec!(50100), // high
            dec!(49900), // low
            dec!(50050), // close ( > open = 牛市)
        );

        assert_eq!(path.len(), 60);
        assert_eq!(path[0], dec!(50000)); // 起点是 open
        assert_eq!(*path.back().unwrap(), dec!(50050)); // 终点是 close
    }

    #[test]
    fn test_price_path_bearish() {
        let path = TickGenerator::generate_price_path(
            dec!(50050), // open
            dec!(50100), // high
            dec!(49900), // low
            dec!(50000), // close ( < open = 熊市)
        );

        assert_eq!(path.len(), 60);
        assert_eq!(path[0], dec!(50050)); // 起点是 open
        assert_eq!(*path.back().unwrap(), dec!(50000)); // 终点是 close
    }

    #[test]
    fn test_accumulated_high_low() {
        let klines = vec![KLineInput {
            open: dec!(50000),
            high: dec!(50100),
            low: dec!(49900),
            close: dec!(50050),
            volume: dec!(100),
            timestamp: Utc::now(),
        }];

        let mut g = TickGenerator::new("BTCUSDT".to_string(), klines);

        let mut prev_high = dec!(0);
        let mut prev_low = Decimal::MAX;

        // 前10个 tick
        for _ in 0..10 {
            let tick = g.next_tick().unwrap();
            assert!(tick.high >= prev_high);
            assert!(tick.low <= prev_low);
            prev_high = tick.high;
            prev_low = tick.low;
        }
    }
}
