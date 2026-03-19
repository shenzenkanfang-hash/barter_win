use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum IndicatorError {
    #[error("指标计算错误: {0}")]
    CalculationError(String),

    #[error("指标未初始化: {0}")]
    NotInitialized(String),

    #[error("无效周期: {0}")]
    InvalidPeriod(String),

    #[error("除零错误: {0}")]
    DivisionByZero(String),
}
