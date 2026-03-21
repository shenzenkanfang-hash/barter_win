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

    #[error("其他错误: {0}")]
    Other(String),
}
