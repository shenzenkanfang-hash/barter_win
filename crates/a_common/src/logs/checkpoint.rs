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

    /// 记录通用事件（用于交易系统各组件的关键操作日志）
    fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str);
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

    fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        eprintln!(
            "[{}] [{}] [{}] {} {:?}",
            timestamp,
            component,
            event,
            data,
            symbol
        );
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

    fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str) {
        tracing::info!(
            component = component,
            event = event,
            symbol = symbol,
            data = data,
            "trading_event"
        );
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

    fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str) {
        for logger in &self.loggers {
            logger.log_event(component, event, symbol, data);
        }
    }
}

/// 组件健康状态（用于数据层/指标层1小时间隔健康摘要）
#[derive(Debug, Clone, Default)]
pub struct ComponentHealth {
    /// 最后处理K线时间戳（毫秒，Unix epoch）
    pub last_tick_timestamp_ms: i64,
    /// 已处理K线数
    pub processed_kline_count: u64,
    /// 上次计算延迟（毫秒）
    pub last_compute_latency_ms: u64,
    /// 累计错误数
    pub error_count: u32,
}

impl ComponentHealth {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_tick(&mut self, timestamp_ms: i64, latency_ms: u64) {
        self.last_tick_timestamp_ms = timestamp_ms;
        self.last_compute_latency_ms = latency_ms;
        self.processed_kline_count += 1;
    }

    pub fn add_error(&mut self) {
        self.error_count += 1;
    }
}

/// Thread-safe 健康状态累加器（无锁设计）
pub struct HealthAccumulator {
    last_tick_timestamp_ms: std::sync::atomic::AtomicI64,
    processed_kline_count: std::sync::atomic::AtomicU64,
    last_compute_latency_ms: std::sync::atomic::AtomicU64,
    error_count: std::sync::atomic::AtomicU32,
}

impl HealthAccumulator {
    pub fn new() -> Self {
        Self {
            last_tick_timestamp_ms: std::sync::atomic::AtomicI64::new(0),
            processed_kline_count: std::sync::atomic::AtomicU64::new(0),
            last_compute_latency_ms: std::sync::atomic::AtomicU64::new(0),
            error_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    pub fn update_tick(&self, timestamp_ms: i64, latency_ms: u64) {
        self.last_tick_timestamp_ms.store(timestamp_ms, std::sync::atomic::Ordering::SeqCst);
        self.last_compute_latency_ms.store(latency_ms, std::sync::atomic::Ordering::SeqCst);
        self.processed_kline_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn add_error(&self) {
        self.error_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> ComponentHealth {
        ComponentHealth {
            last_tick_timestamp_ms: self.last_tick_timestamp_ms.load(std::sync::atomic::Ordering::SeqCst),
            processed_kline_count: self.processed_kline_count.load(std::sync::atomic::Ordering::SeqCst),
            last_compute_latency_ms: self.last_compute_latency_ms.load(std::sync::atomic::Ordering::SeqCst),
            error_count: self.error_count.load(std::sync::atomic::Ordering::SeqCst),
        }
    }
}

impl Default for HealthAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// ComponentHealthLogger - 定时输出 health.summary 日志
///
/// 每小时输出一次组件健康摘要，使用 tokio 定时器。
/// 输出格式：JSON Lines，字段对齐 ComponentHealth。
pub struct ComponentHealthLogger {
    component: String,
    accumulator: std::sync::Arc<HealthAccumulator>,
    interval_secs: u64,
}

impl ComponentHealthLogger {
    pub fn new(component: &str, interval_secs: u64) -> Self {
        Self {
            component: component.to_string(),
            accumulator: std::sync::Arc::new(HealthAccumulator::new()),
            interval_secs,
        }
    }

    pub fn accumulator(&self) -> std::sync::Arc<HealthAccumulator> {
        std::sync::Arc::clone(&self.accumulator)
    }

    /// 启动后台定时日志任务（tokio::spawn）
    pub fn start_background_logger(self: std::sync::Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(self.interval_secs));
            loop {
                interval.tick().await;
                let health = self.accumulator.snapshot();
                tracing::info!(
                    component = %self.component,
                    event = "health.summary",
                    last_tick_ms = health.last_tick_timestamp_ms,
                    processed = health.processed_kline_count,
                    latency_ms = health.last_compute_latency_ms,
                    errors = health.error_count,
                    "health_summary"
                );
            }
        })
    }
}

/// 交易系统日志事件类型
///
/// 对齐 07-CONTEXT.md 中定义的所有事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingLogEventType {
    // 组件生命周期
    ComponentStarted,
    ComponentStopped,
    HealthSummary,

    // 数据层
    DataReceived,
    IndicatorComputed,

    // 策略层
    StrategySignal,

    // 风控层
    RiskCheck,
    RiskCheckSkipped,

    // 交易执行
    TradeLockAcquired,
    TradeLockSkipped,
    OrderSubmitted,
    OrderFilled,
    OrderRejected,
    OrderCancelled,

    // 仓位
    PositionOpened,
    PositionClosed,

    // 异常
    StaleDetected,
    Error,
}

impl std::fmt::Display for TradingLogEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradingLogEventType::ComponentStarted => write!(f, "component.started"),
            TradingLogEventType::ComponentStopped => write!(f, "component.stopped"),
            TradingLogEventType::HealthSummary => write!(f, "health.summary"),
            TradingLogEventType::DataReceived => write!(f, "data.received"),
            TradingLogEventType::IndicatorComputed => write!(f, "indicator.computed"),
            TradingLogEventType::StrategySignal => write!(f, "strategy.signal"),
            TradingLogEventType::RiskCheck => write!(f, "risk.check"),
            TradingLogEventType::RiskCheckSkipped => write!(f, "risk.check.skipped"),
            TradingLogEventType::TradeLockAcquired => write!(f, "trade.lock.acquired"),
            TradingLogEventType::TradeLockSkipped => write!(f, "trade.lock.skipped"),
            TradingLogEventType::OrderSubmitted => write!(f, "order.submitted"),
            TradingLogEventType::OrderFilled => write!(f, "order.filled"),
            TradingLogEventType::OrderRejected => write!(f, "order.rejected"),
            TradingLogEventType::OrderCancelled => write!(f, "order.cancelled"),
            TradingLogEventType::PositionOpened => write!(f, "position.opened"),
            TradingLogEventType::PositionClosed => write!(f, "position.closed"),
            TradingLogEventType::StaleDetected => write!(f, "stale.detected"),
            TradingLogEventType::Error => write!(f, "error"),
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
