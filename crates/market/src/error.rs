use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum MarketError {
    #[error("WebSocket连接失败: {0}")]
    WebSocketConnectionFailed(String),

    #[error("WebSocket错误: {0}")]
    WebSocketError(String),

    #[error("序列化错误: {0}")]
    SerializeError(String),

    #[error("订阅失败: {0}")]
    SubscribeFailed(String),

    #[error("取消订阅失败: {0}")]
    UnsubscribeFailed(String),

    #[error("数据解析错误: {0}")]
    ParseError(String),

    #[error("K线合成错误: {0}")]
    KLineError(String),

    #[error("订单簿错误: {0}")]
    OrderBookError(String),

    #[error("超时: {0}")]
    Timeout(String),
}
