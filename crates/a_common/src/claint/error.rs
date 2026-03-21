use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum EngineError {
    #[error("风控检查失败: {0}")]
    RiskCheckFailed(String),

    #[error("订单执行失败: {0}")]
    OrderExecutionFailed(String),

    #[error("锁获取失败: {0}")]
    LockFailed(String),

    #[error("资金不足: {0}")]
    InsufficientFund(String),

    #[error("持仓超出限制: {0}")]
    PositionLimitExceeded(String),

    #[error("模式切换失败: {0}")]
    ModeSwitchFailed(String),

    #[error("网络错误: {0}")]
    Network(String),

    #[error("内存备份错误: {0}")]
    MemoryBackup(String),

    #[error("交易对不存在: {0}")]
    SymbolNotFound(String),

    #[error("其他错误: {0}")]
    Other(String),
}

/// 市场数据错误类型
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

    #[error("Redis错误: {0}")]
    RedisError(String),

    #[error("网络请求错误: {0}")]
    NetworkError(String),
}
