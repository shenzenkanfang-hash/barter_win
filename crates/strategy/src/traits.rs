use crate::error::StrategyError;
use crate::types::{Signal, TradingMode};

pub trait Strategy: Send + Sync {
    fn id(&self) -> &str;
    fn mode(&self) -> TradingMode;
    fn check_signal(&mut self) -> Result<Option<Signal>, StrategyError>;
    fn on_tick(&mut self) -> Result<Option<Signal>, StrategyError>;
}
