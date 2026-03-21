//! WebSocket 连接器 trait 定义
//!
//! 只包含抽象的连接器接口，不依赖业务类型。

use crate::claint::error::MarketError;
use async_trait::async_trait;

/// 市场数据连接器 trait - 订阅/退订
#[async_trait]
pub trait MarketConnector: Send + Sync {
    async fn subscribe(&mut self, symbol: &str) -> Result<(), MarketError>;
    async fn unsubscribe(&mut self, symbol: &str) -> Result<(), MarketError>;
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
