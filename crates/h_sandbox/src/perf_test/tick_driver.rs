//! TickDriver - 数据源驱动
//!
//! 从 parquet 读取 K线数据，生成 tick 流

use std::path::Path;
use std::time::{Duration, Instant};
use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;

use crate::perf_test::PerfTestConfig;
use b_data_source::Tick;

/// 配置
const TICKS_PER_1M: u8 = 60;
const TICK_INTERVAL_MS: i64 = 1000; // 1秒一个 tick（简化）

/// Tick + 发送时间戳
#[derive(Debug, Clone)]
pub struct TimedTick {
    /// tick 数据
    pub tick: Tick,
    /// 发送时间（t0）
    pub t0: Instant,
    /// tick 序号
    pub seq: u64,
}

/// K线输入
#[derive(Debug, Clone)]
struct KLineInput {
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
    timestamp: DateTime<Utc>,
}

/// Tick 生成器
struct TickGenerator {
    symbol: String,
    klines: VecDeque<KLineInput>,
    current_kline: Option<KLineInput>,
    tick_index: u8,
    accumulated_high: Decimal,
    accumulated_low: Decimal,
    price_path: VecDeque<Decimal>,
    volume_per_tick: Decimal,
}

impl TickGenerator {
    fn new(symbol: String, klines: Vec<KLineInput>) -> Self {
        Self {
            symbol,
            klines: VecDeque::from(klines),
            current_kline: None,
            tick_index: TICKS_PER_1M,
            accumulated_high: Decimal::ZERO,
            accumulated_low: Decimal::MAX,
            price_path: VecDeque::new(),
            volume_per_tick: Decimal::ZERO,
        }
    }

    fn next_tick(&mut self, seq: u64, t0: Instant) -> Option<TimedTick> {
        if self.tick_index >= TICKS_PER_1M {
            self.load_next_kline()?;
        }

        let price = self.price_path.pop_front()?;

        if price > self.accumulated_high {
            self.accumulated_high = price;
        }
        if price < self.accumulated_low {
            self.accumulated_low = price;
        }

        let kline_ts = self.current_kline.as_ref().unwrap().timestamp;
        let tick_offset_ms = (self.tick_index as i64) * TICK_INTERVAL_MS;
        let tick_ts = kline_ts + chrono::Duration::milliseconds(tick_offset_ms);

        self.tick_index += 1;

        Some(TimedTick {
            tick: Tick {
                symbol: self.symbol.clone(),
                price,
                qty: dec!(0.01),
                timestamp: tick_ts,
                kline_1m: None,
                kline_15m: None,
                kline_1d: None,
            },
            t0,
            seq,
        })
    }

    fn is_exhausted(&self) -> bool {
        self.current_kline.is_none() && self.klines.is_empty()
    }

    fn total_ticks(&self) -> u64 {
        (self.klines.len() as u64 + if self.current_kline.is_some() { 1 } else { 0 }) * TICKS_PER_1M as u64
    }

    fn load_next_kline(&mut self) -> Option<()> {
        self.current_kline = Some(self.klines.pop_front()?);
        let kline = self.current_kline.as_ref().unwrap();

        self.tick_index = 0;
        self.accumulated_high = kline.high;
        self.accumulated_low = kline.low;
        self.volume_per_tick = kline.volume / Decimal::from(TICKS_PER_1M);

        self.price_path = Self::generate_price_path(
            kline.open,
            kline.high,
            kline.low,
            kline.close,
        );

        Some(())
    }

    fn generate_price_path(
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
    ) -> VecDeque<Decimal> {
        use rust_decimal::prelude::ToPrimitive;
        
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
            let seg0 = ((dist[0] / total_dist * Decimal::from(TICKS_PER_1M)).to_u8().unwrap_or(20)).max(2);
            let seg1 = ((dist[1] / total_dist * Decimal::from(TICKS_PER_1M)).to_u8().unwrap_or(20)).max(2);
            let seg2 = ((dist[2] / total_dist * Decimal::from(TICKS_PER_1M)).to_u8().unwrap_or(20)).max(2);
            [seg0, seg1, seg2]
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

        let mut path = VecDeque::new();
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
                path.push_back(price);
            }
        }

        while path.len() < TICKS_PER_1M as usize {
            path.push_back(close);
        }

        path
    }
}

/// TickDriver - 数据源驱动
pub struct TickDriver {
    generator: std::sync::Arc<std::sync::RwLock<TickGenerator>>,
    config: PerfTestConfig,
    sender: mpsc::Sender<TimedTick>,
    total_ticks: u64,
}

impl TickDriver {
    /// 创建 TickDriver（使用模拟数据）
    pub fn new(config: PerfTestConfig, sender: mpsc::Sender<TimedTick>) -> Result<Self, String> {
        // 生成模拟 K线数据
        let klines = Self::generate_mock_klines(&config.symbol, 100);

        let total_ticks = (klines.len() as u64) * TICKS_PER_1M as u64;

        let generator = TickGenerator::new(config.symbol.clone(), klines);

        Ok(Self {
            generator: std::sync::Arc::new(std::sync::RwLock::new(generator)),
            config,
            sender,
            total_ticks,
        })
    }

    /// 创建 TickDriver（从 parquet 加载）
    pub fn from_parquet(config: PerfTestConfig, sender: mpsc::Sender<TimedTick>) -> Result<Self, String> {
        if !Path::new(&config.parquet_path).exists() {
            return Err(format!("文件不存在: {}", config.parquet_path));
        }

        // 简化：先用模拟数据，后续添加 parquet 支持
        tracing::warn!("Parquet 加载暂未实现，使用模拟数据");
        Self::new(config, sender)
    }

    /// 生成模拟 K线数据
    fn generate_mock_klines(_symbol: &str, count: usize) -> Vec<KLineInput> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut klines = Vec::new();
        let base_price = 50000.0;
        let mut current_price = base_price;
        let now = Utc::now();

        for i in 0..count {
            // 简单的伪随机价格变动
            let mut hasher = DefaultHasher::new();
            i.hash(&mut hasher);
            let hash = hasher.finish();
            
            let change = ((hash % 200) as f64 - 100.0) / 1000.0;
            let open = current_price;
            let close = current_price * (1.0 + change);
            
            let high = open.max(close) * (1.0 + (hash % 50) as f64 / 10000.0);
            let low = open.min(close) * (1.0 - (hash % 50) as f64 / 10000.0);
            
            current_price = close;

            klines.push(KLineInput {
                open: Decimal::try_from(open).unwrap_or(Decimal::ZERO),
                high: Decimal::try_from(high).unwrap_or(Decimal::ZERO),
                low: Decimal::try_from(low).unwrap_or(Decimal::ZERO),
                close: Decimal::try_from(close).unwrap_or(Decimal::ZERO),
                volume: Decimal::from(100 + (hash % 900)),
                timestamp: now + chrono::Duration::minutes(i as i64),
            });
        }

        klines
    }

    /// 获取总 tick 数
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    /// 获取配置
    pub fn config(&self) -> &PerfTestConfig {
        &self.config
    }

    /// 运行（快速模式 - 不等待）
    pub async fn run_fast(&self) {
        let mut seq = 0u64;

        while let Some(tick) = {
            let t0 = Instant::now();
            let mut g = self.generator.write().unwrap();
            if g.is_exhausted() {
                None
            } else {
                g.next_tick(seq, t0)
            }
        } {
            if self.sender.send(tick).await.is_err() {
                break;
            }
            seq += 1;
        }
    }

    /// 运行（实时模式 - 等待间隔）
    pub async fn run_realtime(&self) {
        let mut seq = 0u64;
        let interval = Duration::from_millis(self.config.tick_interval_ms);
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;

            let t0 = Instant::now();
            let tick = {
                let mut g = self.generator.write().unwrap();
                if g.is_exhausted() {
                    break;
                }
                let tick = g.next_tick(seq, t0);
                if tick.is_none() {
                    break;
                }
                tick
            };

            if let Some(t) = tick {
                if self.sender.send(t).await.is_err() {
                    break;
                }
                seq += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator() {
        let klines = vec![
            KLineInput {
                open: dec!(50000),
                high: dec!(50100),
                low: dec!(49900),
                close: dec!(50050),
                volume: dec!(100),
                timestamp: Utc::now(),
            },
        ];

        let generator = TickGenerator::new("BTCUSDT".to_string(), klines);
        assert!(!generator.is_exhausted());
    }
}
