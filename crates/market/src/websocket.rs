use crate::error::MarketError;
use crate::types::Tick;
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;

#[async_trait]
pub trait MarketConnector: Send + Sync {
    async fn subscribe(&mut self, symbol: &str) -> Result<(), MarketError>;
    async fn unsubscribe(&mut self, symbol: &str) -> Result<(), MarketError>;
}

#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&mut self) -> Option<Tick>;
}

/// 模拟市场数据流 - 用于测试
pub struct MockMarketStream {
    symbol: String,
    base_price: Decimal,
    current_price: Decimal,
    tick_count: u64,
}

impl MockMarketStream {
    pub fn new(symbol: String, base_price: Decimal) -> Self {
        Self {
            symbol,
            base_price,
            current_price: base_price,
            tick_count: 0,
        }
    }
}

#[async_trait]
impl MarketStream for MockMarketStream {
    async fn next_tick(&mut self) -> Option<Tick> {
        use rand::Rng;

        self.tick_count += 1;

        // 简单的随机游走价格生成
        let change_percent = rand::thread_rng().gen_range(-0.001..0.001);
        let price_change = self.current_price * Decimal::try_from(change_percent).unwrap();
        self.current_price += price_change;

        // 确保价格不会变得太小
        if self.current_price < self.base_price * Decimal::try_from(0.5).unwrap() {
            self.current_price = self.base_price;
        }

        Some(Tick {
            symbol: self.symbol.clone(),
            price: self.current_price,
            qty: Decimal::try_from(1.0).unwrap(),
            timestamp: Utc::now(),
        })
    }
}

/// 模拟连接器 - 用于测试
pub struct MockMarketConnector {
    subscribed_symbols: Vec<String>,
}

impl MockMarketConnector {
    pub fn new() -> Self {
        Self {
            subscribed_symbols: Vec::new(),
        }
    }
}

#[async_trait]
impl MarketConnector for MockMarketConnector {
    async fn subscribe(&mut self, symbol: &str) -> Result<(), MarketError> {
        if !self.subscribed_symbols.contains(&symbol.to_string()) {
            self.subscribed_symbols.push(symbol.to_string());
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, symbol: &str) -> Result<(), MarketError> {
        self.subscribed_symbols.retain(|s| s != symbol);
        Ok(())
    }
}
