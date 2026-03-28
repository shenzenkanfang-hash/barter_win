//! 模拟统一数据接口 - DataFeeder
//!
//! 对齐 b_data_source::api::data_feeder，但使用模拟数据源

use crate::models::Tick;
use crate::store::MarketDataStoreImpl;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 订阅者条目
struct Subscriber {
    tx: mpsc::Sender<Tick>,
    symbol: String,
}

/// 模拟数据提供者
pub struct DataFeeder {
    /// 缓存的最新 Tick
    latest_ticks: RwLock<HashMap<String, Tick>>,
    /// 1m 订阅者列表
    subscribers_1m: RwLock<Vec<Subscriber>>,
    /// 存储
    store: Arc<MarketDataStoreImpl>,
}

impl DataFeeder {
    pub fn new() -> Self {
        Self {
            latest_ticks: RwLock::new(HashMap::new()),
            subscribers_1m: RwLock::new(Vec::new()),
            store: Arc::new(MarketDataStoreImpl::new()),
        }
    }

    /// 获取存储
    pub fn store(&self) -> &Arc<MarketDataStoreImpl> {
        &self.store
    }

    /// 订阅 1m Tick 数据流
    pub fn subscribe_1m(&self, symbol: &str, tx: mpsc::Sender<Tick>) {
        let subscriber = Subscriber {
            tx,
            symbol: symbol.to_uppercase(),
        };
        self.subscribers_1m.write().push(subscriber);
    }

    /// 推送 Tick
    pub fn push_tick(&self, tick: Tick) {
        let symbol = tick.symbol.clone();

        // 更新内部缓存
        {
            let mut ticks = self.latest_ticks.write();
            ticks.insert(symbol.clone(), tick.clone());
        }

        // 广播给订阅者
        {
            let mut subs = self.subscribers_1m.write();
            subs.retain(|sub| {
                if sub.symbol == symbol.to_uppercase() {
                    let _ = sub.tx.try_send(tick.clone());
                    true
                } else {
                    true
                }
            });
        }
    }

    /// 获取最新 Tick
    pub fn get_latest_tick(&self, symbol: &str) -> Option<Tick> {
        self.latest_ticks.read().get(&symbol.to_uppercase()).cloned()
    }
}

impl Default for DataFeeder {
    fn default() -> Self {
        Self::new()
    }
}
