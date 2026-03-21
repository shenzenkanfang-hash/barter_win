//! MarketStream trait 和 Mock 实现
//!
//! 定义市场数据流接口，返回业务类型 Tick。

use crate::types::Tick;
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;

/// 市场数据流 trait
#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&self) -> Option<Tick>;
}

/// 模拟市场数据流 - 用于测试
pub struct MockMarketStream {
    symbol: String,
    base_price: Decimal,
    current_price: Decimal,
}

impl MockMarketStream {
    pub fn new(symbol: String, base_price: Decimal) -> Self {
        Self {
            symbol,
            base_price,
            current_price: base_price,
        }
    }
}

#[async_trait]
impl MarketStream for MockMarketStream {
    async fn next_tick(&self) -> Option<Tick> {
        use rand::Rng;

        // 简单的随机游走价格生成
        let change_percent = rand::thread_rng().gen_range(-0.001..0.001);
        let price_change = self.current_price * Decimal::try_from(change_percent).ok()?;

        let new_price = self.current_price + price_change;

        // 确保价格不会变得太小
        let final_price = if new_price < self.base_price * Decimal::try_from(0.5).ok()? {
            self.base_price
        } else {
            new_price
        };

        // 注意: 这不是线程安全的，但在单线程测试场景下可以工作
        // 如果需要真正的线程安全，应该用 Arc<Mutex<Decimal>>
        let price_for_tick = final_price;

        Some(Tick {
            symbol: self.symbol.clone(),
            price: price_for_tick,
            qty: Decimal::try_from(1.0).ok()?,
            timestamp: Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        })
    }
}
