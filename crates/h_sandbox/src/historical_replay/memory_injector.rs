//! MemoryInjector - 内存写入适配器
//!
//! 将仿真 Tick 流式写入系统共享内存（DataFeeder），触发 K线合成器更新。

use std::sync::Arc;
use parking_lot::RwLock;
use chrono::Utc;
use rust_decimal::Decimal;
use tracing::{info, debug, warn};

use b_data_source::{KLine, Period, Tick};
use super::tick_generator::SimulatedTick;

/// 内存注入器配置
#[derive(Debug, Clone)]
pub struct MemoryInjectorConfig {
    /// 写入间隔（毫秒）
    pub write_interval_ms: u64,
    /// 是否自动触发 K线更新
    pub auto_update_kline: bool,
    /// 是否输出调试日志
    pub debug_log: bool,
}

impl Default for MemoryInjectorConfig {
    fn default() -> Self {
        Self {
            write_interval_ms: 16, // ~60fps
            auto_update_kline: true,
            debug_log: false,
        }
    }
}

/// 简化的内存数据结构（对齐 DataFeeder）
///
/// 实际使用时替换为 b_data_source 的 DataFeeder
#[derive(Debug, Clone)]
pub struct SharedMarketData {
    /// 当前 Tick
    pub tick: Option<Tick>,
    /// 当前 K线
    pub kline: Option<KLine>,
    /// 最后更新时间戳
    pub last_update: i64,
}

impl SharedMarketData {
    pub fn new() -> Self {
        Self {
            tick: None,
            kline: None,
            last_update: Utc::now().timestamp_millis(),
        }
    }
}

impl Default for SharedMarketData {
    fn default() -> Self {
        Self::new()
    }
}

/// 内存写入器
///
/// 将 SimulatedTick 写入系统共享内存，触发 K线合成器更新。
pub struct MemoryInjector {
    /// 共享内存引用
    shared_data: Arc<RwLock<SharedMarketData>>,
    /// 配置
    config: MemoryInjectorConfig,
    /// 当前 K线（用于合成）
    current_kline: Option<KLine>,
    /// 当前 K线的 Tick 累积
    kline_ticks: Vec<SimulatedTick>,
}

impl MemoryInjector {
    /// 创建写入器
    pub fn new(shared_data: Arc<RwLock<SharedMarketData>>) -> Self {
        Self {
            shared_data,
            config: MemoryInjectorConfig::default(),
            current_kline: None,
            kline_ticks: Vec::new(),
        }
    }

    /// 创建写入器（带配置）
    pub fn with_config(
        shared_data: Arc<RwLock<SharedMarketData>>,
        config: MemoryInjectorConfig,
    ) -> Self {
        Self {
            shared_data,
            config,
            current_kline: None,
            kline_ticks: Vec::new(),
        }
    }

    /// 写入单个 Tick
    pub fn write_tick(&mut self, tick: SimulatedTick) {
        // 检查是否需要开启新的 K线
        let kline_ts = tick.kline_timestamp.timestamp_millis();
        let current_kline_ts = self.current_kline
            .as_ref()
            .map(|k| k.timestamp.timestamp_millis())
            .unwrap_or(-1);

        if kline_ts != current_kline_ts {
            // 新的 K线：先完成旧的
            if self.current_kline.is_some() && !self.kline_ticks.is_empty() {
                self.finalize_kline();
            }
            // 开启新 K线
            self.current_kline = Some(KLine {
                symbol: tick.symbol.clone(),
                period: Period::Minute(1),
                open: tick.price,
                high: tick.price,
                low: tick.price,
                close: tick.price,
                volume: tick.volume,
                timestamp: tick.kline_timestamp,
            });
            self.kline_ticks.clear();
        }

        // 更新当前 K线
        if let Some(ref mut kline) = self.current_kline {
            if tick.price > kline.high {
                kline.high = tick.price;
            }
            if tick.price < kline.low {
                kline.low = tick.price;
            }
            kline.close = tick.price;
            kline.volume = kline.volume + tick.volume;
        }

        self.kline_ticks.push(tick.clone());

        // 写入共享内存
        let tick_model = Tick {
            symbol: tick.symbol,
            price: tick.price,
            qty: tick.qty,
            timestamp: tick.timestamp,
        };

        {
            let mut shared = self.shared_data.write();
            shared.tick = Some(tick_model);
            shared.kline = self.current_kline.clone();
            shared.last_update = Utc::now().timestamp_millis();
        }

        if self.config.debug_log {
            debug!("写入 Tick: price={}, qty={}", tick.price, tick.qty);
        }
    }

    /// 完成当前 K线并更新合成器
    fn finalize_kline(&mut self) {
        if let Some(ref kline) = self.current_kline {
            if self.config.debug_log {
                debug!("完成 K线: O={}, H={}, L={}, C={}, V={}",
                    kline.open, kline.high, kline.low, kline.close, kline.volume);
            }
            // 触发 K线合成器更新（如果有监听器）
        }
    }

    /// 批量写入 Tick
    pub fn write_batch(&mut self, ticks: impl Iterator<Item = SimulatedTick>) {
        for tick in ticks {
            self.write_tick(tick);
        }
    }

    /// 获取当前已写入的 Tick 数
    pub fn tick_count(&self) -> usize {
        self.kline_ticks.len()
    }

    /// 获取当前 K线信息
    pub fn current_kline(&self) -> Option<&KLine> {
        self.current_kline.as_ref()
    }
}

/// 数据写入目标 trait
pub trait DataWriter {
    fn write(&mut self, tick: SimulatedTick);
}

impl DataWriter for MemoryInjector {
    fn write(&mut self, tick: SimulatedTick) {
        self.write_tick(tick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injector_creation() {
        let shared = Arc::new(RwLock::new(SharedMarketData::new()));
        let injector = MemoryInjector::new(shared);
        assert!(injector.current_kline.is_none());
        assert_eq!(injector.tick_count(), 0);
    }
}
