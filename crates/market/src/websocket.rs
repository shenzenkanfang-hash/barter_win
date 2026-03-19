use crate::error::MarketError;
use crate::types::Tick;
use async_trait::async_trait;

#[async_trait]
pub trait MarketConnector: Send + Sync {
    async fn subscribe(&mut self, symbol: &str) -> Result<(), MarketError>;
    async fn unsubscribe(&mut self, symbol: &str) -> Result<(), MarketError>;
}

#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&mut self) -> Option<Tick>;
}
