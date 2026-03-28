//! 模拟 Depth 订单簿 WebSocket
//!
//! 生成模拟订单簿数据

use std::collections::HashMap;
use std::sync::Arc;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use rand::Rng;

use crate::store::{MarketDataStoreImpl, OrderBookData};
use crate::store::store_trait::MarketDataStore;
use super::orderbook::OrderBook;

/// 模拟 Depth 流管理器
pub struct DepthStream {
    /// 共享存储
    store: Arc<MarketDataStoreImpl>,
    /// 最新订单簿缓存
    latest_orderbooks: HashMap<String, OrderBook>,
    /// 基准价格
    base_prices: HashMap<String, Decimal>,
}

impl DepthStream {
    pub fn new() -> Self {
        Self {
            store: Arc::new(MarketDataStoreImpl::new()),
            latest_orderbooks: HashMap::new(),
            base_prices: HashMap::new(),
        }
    }

    /// 设置基准价格
    pub fn set_base_price(&mut self, symbol: &str, price: Decimal) {
        self.base_prices.insert(symbol.to_lowercase(), price);
    }

    /// 生成模拟订单簿
    pub fn generate_orderbook(&mut self, symbol: &str) -> OrderBook {
        let symbol_lower = symbol.to_lowercase();
        let base_price = self.base_prices.get(&symbol_lower)
            .copied()
            .unwrap_or(dec!(50000));

        let mut rng = rand::thread_rng();

        // 生成买单
        let bids: Vec<(Decimal, Decimal)> = (0..20)
            .map(|i| {
                let offset = Decimal::from(i) * dec!(0.1);
                let price = base_price - offset;
                // 用整数采样避免 Decimal::from(f64) 问题
                let qty_i: i32 = rng.gen_range(1..100);
                let qty = Decimal::from(qty_i) / dec!(100) * dec!(10.0);
                (price, qty)
            })
            .collect();

        // 生成卖单
        let asks: Vec<(Decimal, Decimal)> = (0..20)
            .map(|i| {
                let offset = Decimal::from(i + 1) * dec!(0.1);
                let price = base_price + offset;
                let qty_i: i32 = rng.gen_range(1..100);
                let qty = Decimal::from(qty_i) / dec!(100) * dec!(10.0);
                (price, qty)
            })
            .collect();

        let mut orderbook = OrderBook::new(symbol.to_string());
        orderbook.update(
            chrono::Utc::now().timestamp_millis() as u64,
            bids.clone(),
            asks.clone(),
        );

        self.latest_orderbooks.insert(symbol_lower.clone(), orderbook.clone());

        // 写入存储
        let orderbook_data = OrderBookData {
            symbol: symbol.to_string(),
            bids: bids.iter().map(|(p, q)| (p.to_f64().unwrap_or(0.0), q.to_f64().unwrap_or(0.0))).collect(),
            asks: asks.iter().map(|(p, q)| (p.to_f64().unwrap_or(0.0), q.to_f64().unwrap_or(0.0))).collect(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };

        self.store.write_orderbook(symbol, orderbook_data);

        orderbook
    }

    /// 获取最新订单簿
    pub fn get_latest_orderbook(&self, symbol: &str) -> Option<OrderBook> {
        self.latest_orderbooks.get(&symbol.to_lowercase()).cloned()
    }

    /// 获取存储
    pub fn store(&self) -> &Arc<MarketDataStoreImpl> {
        &self.store
    }
}

impl Default for DepthStream {
    fn default() -> Self {
        Self::new()
    }
}

/// Depth 数据结构
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepthData {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: i64,
    pub bids: Vec<(String, String)>,
    pub asks: Vec<(String, String)>,
}
