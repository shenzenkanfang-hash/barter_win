//! MarketStream trait 和 Mock 实现
//!
//! 定义市场数据流接口，返回业务类型 Tick。

use crate::models::types::Tick;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::sync::Arc;

/// 市场数据流 trait
#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&self) -> Option<Tick>;
}

/// 模拟市场数据流配置
#[derive(Debug, Clone)]
pub struct MockStreamConfig {
    /// 基础价格
    pub base_price: Decimal,
    /// 价格波动率 (每 Tick 变化百分比)
    pub volatility: f64,
    /// 每次价格变动的最大百分比
    pub max_change_rate: f64,
    /// 最小成交量
    pub min_qty: f64,
    /// 最大成交量
    pub max_qty: f64,
}

impl Default for MockStreamConfig {
    fn default() -> Self {
        Self {
            base_price: Decimal::try_from(50000.0).unwrap(),
            volatility: 0.0001,
            max_change_rate: 0.001,
            min_qty: 0.001,
            max_qty: 1.0,
        }
    }
}

/// 模拟市场数据流 - 用于测试
pub struct MockMarketStream {
    symbol: String,
    config: MockStreamConfig,
    current_price: RwLock<Decimal>,
    tick_count: RwLock<u64>,
}

impl MockMarketStream {
    pub fn new(symbol: String, base_price: Decimal) -> Self {
        Self {
            symbol,
            config: MockStreamConfig::default(),
            current_price: RwLock::new(base_price),
            tick_count: RwLock::new(0),
        }
    }

    pub fn with_config(mut self, config: MockStreamConfig) -> Self {
        self.config = config;
        self
    }

    /// 创建带波动率的模拟流
    pub fn with_volatility(symbol: String, base_price: Decimal, volatility: f64) -> Self {
        Self {
            symbol,
            config: MockStreamConfig {
                base_price,
                volatility,
                ..Default::default()
            },
            current_price: RwLock::new(base_price),
            tick_count: RwLock::new(0),
        }
    }

    /// 获取当前价格
    pub fn current_price(&self) -> Decimal {
        *self.current_price.read()
    }

    /// 获取 Tick 计数
    pub fn tick_count(&self) -> u64 {
        *self.tick_count.read()
    }

    /// 重置
    pub fn reset(&self) {
        *self.current_price.write() = self.config.base_price;
        *self.tick_count.write() = 0;
    }
}

#[async_trait]
impl MarketStream for MockMarketStream {
    async fn next_tick(&self) -> Option<Tick> {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        // 更新 tick 计数
        {
            let mut count = self.tick_count.write();
            *count += 1;
        }

        // 计算价格变动
        let change_percent = rng.gen_range(-self.config.volatility..self.config.volatility);
        let price_change = {
            let current = *self.current_price.read();
            current * Decimal::try_from(change_percent).ok()?
        };

        let new_price = {
            let current = *self.current_price.read();
            let new = current + price_change;
            
            // 限制最大变化
            let max_change = current * Decimal::try_from(self.config.max_change_rate).ok()?;
            if new > current + max_change {
                current + max_change
            } else if new < current - max_change {
                current - max_change
            } else {
                new
            }
        };

        *self.current_price.write() = new_price;

        // 生成成交量
        let qty = rng.gen_range(self.config.min_qty..self.config.max_qty);

        // 生成时间戳（每秒一个 tick）
        let tick_cnt = *self.tick_count.read();
        let timestamp = Utc::now() - Duration::seconds(tick_cnt as i64);

        Some(Tick {
            symbol: self.symbol.clone(),
            price: new_price,
            qty: Decimal::try_from(qty).ok()?,
            timestamp,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        })
    }
}

/// 多品种模拟市场数据流
pub struct MockMultiSymbolStream {
    streams: RwLock<FnvHashMap<String, Arc<MockMarketStream>>>,
    symbols: RwLock<Vec<String>>,
    current_idx: RwLock<usize>,
}

impl MockMultiSymbolStream {
    pub fn new() -> Self {
        Self {
            streams: RwLock::new(FnvHashMap::default()),
            symbols: RwLock::new(Vec::new()),
            current_idx: RwLock::new(0),
        }
    }

    /// 添加品种
    pub fn add_symbol(&self, symbol: String, base_price: Decimal) {
        let stream = Arc::new(MockMarketStream::new(symbol.clone(), base_price));
        
        let mut streams = self.streams.write();
        streams.insert(symbol.clone(), stream);
        
        self.symbols.write().push(symbol);
    }

    /// 添加带配置的品种
    pub fn add_symbol_with_config(&self, symbol: String, config: MockStreamConfig) {
        let stream = Arc::new(MockMarketStream::new(symbol.clone(), config.base_price).with_config(config));
        
        let mut streams = self.streams.write();
        streams.insert(symbol.clone(), stream);
        
        self.symbols.write().push(symbol);
    }

    /// 轮询获取下一个品种的 Tick
    pub async fn next_tick_round_robin(&self) -> Option<Tick> {
        let symbols = self.symbols.read().clone();
        if symbols.is_empty() {
            return None;
        }

        let mut idx = *self.current_idx.write();
        let symbol = symbols[idx].clone();
        
        idx = (idx + 1) % symbols.len();
        *self.current_idx.write() = idx;

        let streams = self.streams.read();
        if let Some(stream) = streams.get(&symbol) {
            stream.next_tick().await
        } else {
            None
        }
    }

    /// 获取指定品种的 Stream
    pub fn get_stream(&self, symbol: &str) -> Option<Arc<MockMarketStream>> {
        self.streams.read().get(symbol).cloned()
    }
}

impl Default for MockMultiSymbolStream {
    fn default() -> Self {
        Self::new()
    }
}
