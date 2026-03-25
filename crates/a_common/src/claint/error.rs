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

/// 统一应用错误类型
#[derive(Debug, Clone, Error)]
pub enum AppError {
    // === 引擎错误 ===
    #[error("[Engine] 风控检查失败: {0}")]
    RiskCheckFailed(String),
    #[error("[Engine] 订单执行失败: {0}")]
    OrderExecutionFailed(String),
    #[error("[Engine] 锁获取失败: {0}")]
    LockFailed(String),
    #[error("[Engine] 资金不足: {0}")]
    InsufficientFund(String),
    #[error("[Engine] 持仓超出限制: {0}")]
    PositionLimitExceeded(String),
    #[error("[Engine] 模式切换失败: {0}")]
    ModeSwitchFailed(String),
    #[error("[Engine] 交易对不存在: {0}")]
    SymbolNotFound(String),

    // === 市场数据错误 ===
    #[error("[Market] WebSocket连接失败: {0}")]
    WebSocketConnectionFailed(String),
    #[error("[Market] WebSocket错误: {0}")]
    WebSocketError(String),
    #[error("[Market] 订阅失败: {0}")]
    SubscribeFailed(String),
    #[error("[Market] 取消订阅失败: {0}")]
    UnsubscribeFailed(String),
    #[error("[Market] K线合成错误: {0}")]
    KLineError(String),
    #[error("[Market] 订单簿错误: {0}")]
    OrderBookError(String),
    #[error("[Market] 超时: {0}")]
    Timeout(String),

    // === 数据错误 ===
    #[error("[Data] 序列化错误: {0}")]
    SerializeError(String),
    #[error("[Data] 解析错误: {0}")]
    ParseError(String),

    // === 基础设施错误 ===
    #[error("[Infra] 内存备份错误: {0}")]
    MemoryBackup(String),
    #[error("[Infra] 网络错误: {0}")]
    Network(String),
    #[error("[Infra] Redis错误: {0}")]
    RedisError(String),

    // === 其他 ===
    #[error("[Other] {0}")]
    Other(String),
}

// From<EngineError> 实现
impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::RiskCheckFailed(msg) => AppError::RiskCheckFailed(msg),
            EngineError::OrderExecutionFailed(msg) => AppError::OrderExecutionFailed(msg),
            EngineError::LockFailed(msg) => AppError::LockFailed(msg),
            EngineError::InsufficientFund(msg) => AppError::InsufficientFund(msg),
            EngineError::PositionLimitExceeded(msg) => AppError::PositionLimitExceeded(msg),
            EngineError::ModeSwitchFailed(msg) => AppError::ModeSwitchFailed(msg),
            EngineError::SymbolNotFound(msg) => AppError::SymbolNotFound(msg),
            EngineError::Network(msg) => AppError::Network(msg),
            EngineError::MemoryBackup(msg) => AppError::MemoryBackup(msg),
            EngineError::Other(msg) => AppError::Other(msg),
        }
    }
}

// From<MarketError> 实现
impl From<MarketError> for AppError {
    fn from(e: MarketError) -> Self {
        match e {
            MarketError::WebSocketConnectionFailed(msg) => AppError::WebSocketConnectionFailed(msg),
            MarketError::WebSocketError(msg) => AppError::WebSocketError(msg),
            MarketError::SubscribeFailed(msg) => AppError::SubscribeFailed(msg),
            MarketError::UnsubscribeFailed(msg) => AppError::UnsubscribeFailed(msg),
            MarketError::SerializeError(msg) => AppError::SerializeError(msg),
            MarketError::ParseError(msg) => AppError::ParseError(msg),
            MarketError::KLineError(msg) => AppError::KLineError(msg),
            MarketError::OrderBookError(msg) => AppError::OrderBookError(msg),
            MarketError::Timeout(msg) => AppError::Timeout(msg),
            MarketError::RedisError(msg) => AppError::RedisError(msg),
            MarketError::NetworkError(msg) => AppError::Network(msg),
        }
    }
}
