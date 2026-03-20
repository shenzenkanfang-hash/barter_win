#![forbid(unsafe_code)]

/// Pipeline 环节
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 指标计算
    Indicator,
    /// 策略判断
    Strategy,
    /// 风控预检
    RiskPre,
    /// 风控复核
    RiskRe,
    /// 订单执行
    Order,
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stage::Indicator => write!(f, "指标"),
            Stage::Strategy => write!(f, "策略"),
            Stage::RiskPre => write!(f, "风控预检"),
            Stage::RiskRe => write!(f, "风控复核"),
            Stage::Order => write!(f, "订单"),
        }
    }
}

/// 单个环节的结果
#[derive(Debug, Clone)]
pub struct StageResult {
    /// 环节
    pub stage: Stage,
    /// 是否通过
    pub passed: bool,
    /// 详细信息
    pub details: String,
    /// 失败原因（如果有）
    pub blocked_reason: Option<String>,
}

impl StageResult {
    /// 通过的结果
    pub fn pass(stage: Stage, details: impl Into<String>) -> Self {
        Self {
            stage,
            passed: true,
            details: details.into(),
            blocked_reason: None,
        }
    }

    /// 失败的结果
    pub fn fail(stage: Stage, reason: impl Into<String>) -> Self {
        let reason_str = reason.into();
        Self {
            stage,
            passed: false,
            details: String::new(),
            blocked_reason: Some(reason_str),
        }
    }
}

/// Pipeline 各环节的 checkpoint 日志记录器
pub trait CheckpointLogger: Send + Sync {
    /// 记录环节开始
    fn log_start(&self, stage: Stage, symbol: &str);

    /// 记录环节完成（通过）
    fn log_pass(&self, stage: Stage, symbol: &str, details: &str);

    /// 记录环节失败（Pipeline 停止）
    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str);

    /// 记录完整 checkpoint（所有环节结果）
    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>);
}

/// 控制台彩色输出 CheckpointLogger
pub struct ConsoleCheckpointLogger;

impl ConsoleCheckpointLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConsoleCheckpointLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for ConsoleCheckpointLogger {
    fn log_start(&self, stage: Stage, symbol: &str) {
        eprintln!("[{}] [{}] [▶ {} 开始]", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), symbol, stage);
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        eprintln!("[{}] [{}] [✔ {}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), symbol, stage, details);
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        eprintln!("[{}] [{}] [✘ {}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), symbol, stage, reason);
    }

    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        eprintln!("[{}] [{}] [CHECKPOINT]", timestamp, symbol);

        for result in results {
            if result.passed {
                eprintln!("  ├─ [✔ {}] {}", result.stage, result.details);
            } else {
                eprintln!("  ├─ [✘ {}] {}", result.stage, result.blocked_reason.as_ref().unwrap_or(&result.details));
                break;
            }
        }

        if let Some(blocked) = blocked_at {
            eprintln!("  └─ [BLOCKED] 数据停止传递 - 原因: {}", blocked);
        } else {
            eprintln!("  └─ [COMPLETE] Pipeline 完成");
        }
    }
}

/// 基于 tracing 的结构化日志 CheckpointLogger
pub struct TracingCheckpointLogger;

impl TracingCheckpointLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TracingCheckpointLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for TracingCheckpointLogger {
    fn log_start(&self, stage: Stage, symbol: &str) {
        tracing::info!(stage = ?stage, symbol = symbol, "Pipeline stage started");
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        tracing::info!(stage = ?stage, symbol = symbol, details = details, "Pipeline stage passed");
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        tracing::warn!(stage = ?stage, symbol = symbol, reason = reason, "Pipeline stage blocked");
    }

    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>) {
        let stages: Vec<_> = results.iter().map(|r| {
            if r.passed {
                format!("{:?}=OK", r.stage)
            } else {
                format!("{:?}=BLOCKED", r.stage)
            }
        }).collect();

        if let Some(blocked) = blocked_at {
            tracing::warn!(
                symbol = symbol,
                stages = ?stages,
                blocked_at = ?blocked,
                "Pipeline blocked at stage"
            );
        } else {
            tracing::info!(
                symbol = symbol,
                stages = ?stages,
                "Pipeline completed successfully"
            );
        }
    }
}

/// 组合多个 Logger
pub struct CompositeCheckpointLogger {
    loggers: Vec<Box<dyn CheckpointLogger>>,
}

impl CompositeCheckpointLogger {
    pub fn new() -> Self {
        Self { loggers: Vec::new() }
    }

    pub fn add<L: CheckpointLogger + 'static>(mut self, logger: L) -> Self {
        self.loggers.push(Box::new(logger));
        self
    }
}

impl Default for CompositeCheckpointLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for CompositeCheckpointLogger {
    fn log_start(&self, stage: Stage, symbol: &str) {
        for logger in &self.loggers {
            logger.log_start(stage, symbol);
        }
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        for logger in &self.loggers {
            logger.log_pass(stage, symbol, details);
        }
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        for logger in &self.loggers {
            logger.log_blocked(stage, symbol, reason);
        }
    }

    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>) {
        for logger in &self.loggers {
            logger.log_checkpoint(symbol, results, blocked_at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_result_pass() {
        let result = StageResult::pass(Stage::Indicator, "EMA12=100");
        assert!(result.passed);
        assert_eq!(result.details, "EMA12=100");
        assert!(result.blocked_reason.is_none());
    }

    #[test]
    fn test_stage_result_fail() {
        let result = StageResult::fail(Stage::Strategy, "TR_RATIO < 1");
        assert!(!result.passed);
        assert!(result.blocked_reason.is_some());
        assert_eq!(result.blocked_reason.unwrap(), "TR_RATIO < 1");
    }

    #[test]
    fn test_console_logger() {
        let logger = ConsoleCheckpointLogger::new();
        logger.log_start(Stage::Indicator, "BTCUSDT");
        logger.log_pass(Stage::Indicator, "BTCUSDT", "EMA12=100 RSI=50");
        logger.log_blocked(Stage::Strategy, "BTCUSDT", "TR_RATIO < 1");
    }

    #[test]
    fn test_composite_logger() {
        let logger = CompositeCheckpointLogger::new()
            .add(ConsoleCheckpointLogger::new())
            .add(TracingCheckpointLogger::new());

        logger.log_pass(Stage::Indicator, "BTCUSDT", "test");
    }
}
