use thiserror::Error;

#[derive(Debug, Error)]
pub enum TradingError {
    #[error("market error: {0}")]
    Market(String),

    #[error("order error: {0}")]
    Order(String),

    #[error("position error: {0}")]
    Position(String),

    #[error("fund error: {0}")]
    Fund(String),

    #[error("config error: {0}")]
    Config(String),
}
