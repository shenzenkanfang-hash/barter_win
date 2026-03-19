use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum StrategyError {
    #[error("策略执行错误: {0}")]
    ExecutionError(String),

    #[error("策略未找到: {0}")]
    NotFound(String),

    #[error("信号生成错误: {0}")]
    SignalError(String),

    #[error("无效参数: {0}")]
    InvalidArgument(String),
}
