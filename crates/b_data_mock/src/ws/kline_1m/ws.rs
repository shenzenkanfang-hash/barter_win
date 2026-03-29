//! 模拟 1m K线 WebSocket
//!
//! 使用 KlineStreamGenerator + KLineSynthesizer 替代真实 Binance WS
//!
//! v4.0: 自循环架构 - store 统一使用 b_data_source::store::MarketDataStore

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;

use a_common::heartbeat::Token as HeartbeatToken;
use crate::store::MarketDataStoreImpl;
use crate::ws::kline_1m::kline::KLineSynthesizer;

/// 心跳报到测试点 ID
const HEARTBEAT_POINT_KLINE_STREAM: &str = "BS-001";

/// Store 类型别名：统一使用 b_data_source 的 trait，支持跨 crate 注入
pub type StoreRef = Arc<dyn b_data_source::store::MarketDataStore + Send + Sync>;

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
    /// 共享存储（统一使用 b_data_source::store::MarketDataStore trait）
    store: StoreRef,
    /// 品种 -> K线合成器
    synthesizers: HashMap<String, KLineSynthesizer>,
    /// 当前处理的子K线
    current_sub: Option<crate::ws::kline_generator::SimulatedKline>,
    /// K线生成器
    kline_generator: Option<crate::ws::kline_generator::KlineStreamGenerator>,
    /// v3.0: 心跳 Token
    heartbeat_token: Arc<RwLock<Option<HeartbeatToken>>>,
}

impl Kline1mStream {
    /// 创建模拟 1m K线流（从历史 K线数据）
    ///
    /// 使用内部默认 store（仅用于测试，main.rs 应使用 from_klines_with_store）
    pub fn from_klines(
        symbol: String,
        kline_iter: Box<dyn Iterator<Item = crate::models::KLine> + Send>,
    ) -> Self {
        // 将内部 MarketDataStoreImpl 包装为 dyn trait
        let inner: Arc<MarketDataStoreImpl> = Arc::new(MarketDataStoreImpl::new());
        let store: StoreRef = inner as StoreRef;
        Self {
            store,
            synthesizers: HashMap::new(),
            current_sub: None,
            kline_generator: Some(crate::ws::kline_generator::KlineStreamGenerator::new(symbol, kline_iter)),
            heartbeat_token: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取共享存储
    pub fn store(&self) -> StoreRef {
        self.store.clone()
    }

    /// 创建模拟 1m K线流（使用外部提供的 store）
    ///
    /// 用于 main.rs 中让 Trader 和 Kline1mStream 共享同一个 store 实例，
    /// 这样 Trader 读取 K线时能获取到 Kline1mStream 写入的数据。
    ///
    /// `store` 必须实现 `b_data_source::store::MarketDataStore` trait。
    /// 由于 b_data_mock::MarketDataStoreImpl 现在也 impl了该 trait，
    /// 因此可以用 b_data_mock 的 store 或 b_data_source 的 store。
    pub fn from_klines_with_store(
        symbol: String,
        kline_iter: Box<dyn Iterator<Item = crate::models::KLine> + Send>,
        store: StoreRef,
    ) -> Self {
        Self {
            store,
            synthesizers: HashMap::new(),
            current_sub: None,
            kline_generator: Some(crate::ws::kline_generator::KlineStreamGenerator::new(symbol, kline_iter)),
            heartbeat_token: Arc::new(RwLock::new(None)),
        }
    }

    /// v3.0: 设置心跳 Token
    pub fn set_heartbeat_token(&self, token: HeartbeatToken) {
        let mut guard = self.heartbeat_token.write();
        *guard = Some(token);
    }

    /// v3.0: 获取当前心跳 Token
    pub fn get_heartbeat_token(&self) -> Option<HeartbeatToken> {
        self.heartbeat_token.read().clone()
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

        // 写入共享存储：将 b_data_mock 的 KlineData 转换为 b_data_source 的 KlineData
        let store_kline = b_data_source::ws::kline_1m::ws::KlineData {
            kline_start_time: kline_data.kline_start_time,
            kline_close_time: kline_data.kline_close_time,
            symbol: kline_data.symbol.clone(),
            interval: kline_data.interval.clone(),
            open: kline_data.open.clone(),
            close: kline_data.close.clone(),
            high: kline_data.high.clone(),
            low: kline_data.low.clone(),
            volume: kline_data.volume.clone(),
            is_closed: kline_data.is_closed,
        };
        self.store.write_kline(&sub.symbol, store_kline, sub.is_last_in_kline);

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

    /// v3.0: 获取下一个 K线数据（带心跳报到）
    pub async fn next_message_with_heartbeat(&mut self) -> Option<String> {
        // 心跳报到（使用 spawn_blocking 因为 next_message 是同步的）
        let token = self.get_heartbeat_token();
        if let Some(token) = token {
            let reporter = a_common::heartbeat::global();
            let point_id = HEARTBEAT_POINT_KLINE_STREAM.to_string();
            let module = "b_data_mock::ws::kline_1m".to_string();
            let function = "next_message_with_heartbeat".to_string();
            let file = file!().to_string();

            // 使用 spawn_blocking 执行异步心跳报到
            tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    reporter.report(&token, &point_id, &module, &function, &file).await;
                });
            }).await.ok();
        }

        self.next_message()
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
        let inner: Arc<MarketDataStoreImpl> = Arc::new(MarketDataStoreImpl::new());
        let store: StoreRef = inner as StoreRef;
        Self {
            store,
            synthesizers: HashMap::new(),
            current_sub: None,
            kline_generator: None,
            heartbeat_token: Arc::new(RwLock::new(None)),
        }
    }
}
