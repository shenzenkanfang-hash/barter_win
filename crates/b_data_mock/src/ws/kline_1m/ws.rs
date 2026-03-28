//! 模拟 1m K线 WebSocket
//!
//! 使用 KlineStreamGenerator + KLineSynthesizer 替代真实 Binance WS

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;

use crate::store::{MarketDataStore, MarketDataStoreImpl};
use crate::ws::kline_1m::kline::KLineSynthesizer;

/// Kline 数据结构（与 b_data_source 完全对齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    #[serde(rename = "t")]
    pub kline_start_time: i64,
    #[serde(rename = "T")]
    pub kline_close_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub interval: String,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "x")]
    pub is_closed: bool,
}

/// 模拟 1m K线流管理器
///
/// 使用 KlineStreamGenerator 生成的子K线流，内部维护 KLineSynthesizer
pub struct Kline1mStream {
    /// 共享存储
    store: Arc<MarketDataStoreImpl>,
    /// 品种 -> K线合成器
    synthesizers: HashMap<String, KLineSynthesizer>,
    /// 当前处理的子K线
    current_sub: Option<crate::ws::kline_generator::SimulatedKline>,
    /// K线生成器
    kline_generator: Option<crate::ws::kline_generator::KlineStreamGenerator>,
}

impl Kline1mStream {
    /// 创建模拟 1m K线流（从历史 K线数据）
    pub fn from_klines(
        symbol: String,
        kline_iter: Box<dyn Iterator<Item = crate::models::KLine> + Send>,
    ) -> Self {
        Self {
            store: Arc::new(MarketDataStoreImpl::new()),
            synthesizers: HashMap::new(),
            current_sub: None,
            kline_generator: Some(crate::ws::kline_generator::KlineStreamGenerator::new(symbol, kline_iter)),
        }
    }

    /// 获取共享存储
    pub fn store(&self) -> &Arc<MarketDataStoreImpl> {
        &self.store
    }

    /// 获取下一个 K线数据（从子K线生成）
    pub fn next_message(&mut self) -> Option<String> {
        // 如果没有生成器，直接返回
        let sub = self.kline_generator.as_mut()?.next()?;
        self.current_sub = Some(sub.clone());

        // 获取或创建合成器
        let synthesizer = self.synthesizers
            .entry(sub.symbol.clone())
            .or_insert_with(|| KLineSynthesizer::new(sub.symbol.clone(), crate::models::Period::Minute(1)));

        // 转换为内部 Tick
        let tick_model = crate::models::Tick {
            symbol: sub.symbol.clone(),
            price: sub.price,
            qty: sub.qty,
            timestamp: sub.timestamp,
            sequence_id: sub.sequence_id,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        // 更新合成器
        let completed_kline = synthesizer.update(&tick_model);

        // 构建 KlineData
        let period_ms = 60_000i64;
        let kline_start = sub.kline_timestamp.timestamp_millis();
        let kline_end = kline_start + period_ms;

        let kline_data = KlineData {
            kline_start_time: kline_start,
            kline_close_time: kline_end,
            symbol: sub.symbol.clone(),
            interval: "1m".to_string(),
            open: sub.open.to_string(),
            close: sub.price.to_string(),
            high: sub.high.to_string(),
            low: sub.low.to_string(),
            volume: sub.volume.to_string(),
            is_closed: sub.is_last_in_kline,
        };

        // 写入存储
        self.store.write_kline(&sub.symbol, kline_data.clone(), sub.is_last_in_kline);

        // 如果有完成的 K线，序列化返回
        if sub.is_last_in_kline {
            if let Some(completed) = completed_kline {
                let json = serde_json::json!({
                    "data": {
                        "k": {
                            "t": completed.timestamp.timestamp_millis(),
                            "T": kline_end,
                            "s": completed.symbol,
                            "i": "1m",
                            "o": completed.open,
                            "c": completed.close,
                            "h": completed.high,
                            "l": completed.low,
                            "v": completed.volume,
                            "x": true
                        }
                    }
                });
                return serde_json::to_string(&json).ok();
            }
        }

        serde_json::to_string(&kline_data).ok()
    }

    /// 是否还有数据
    pub fn is_exhausted(&self) -> bool {
        self.kline_generator.as_ref().map(|_g| {
            // 检查生成器是否还有数据
            // 注意: KlineStreamGenerator 消耗自身，不能多次迭代
            // 这里通过检查 current_sub 来判断
            false // 简化：需要外部控制
        }).unwrap_or(true)
    }
}

impl Default for Kline1mStream {
    fn default() -> Self {
        Self {
            store: Arc::new(MarketDataStoreImpl::new()),
            synthesizers: HashMap::new(),
            current_sub: None,
            kline_generator: None,
        }
    }
}
