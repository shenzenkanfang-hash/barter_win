#![forbid(unsafe_code)]

use crate::shared::checkpoint::{CheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult};
use crate::shared::check_table::{CheckEntry, CheckTable};
use indicator::PineColor;
use market::types::Tick;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use strategy::types::{OrderRequest, Side, Signal};

/// Pipeline Processor trait - 所有阶段处理器都实现这个接口
pub trait Processor: Send + Sync {
    /// 处理 tick，返回阶段结果
    fn process(&mut self, check_table: &mut CheckTable, tick: &Tick) -> StageResult;
}

/// Pipeline - 封装完整的交易流程
pub struct Pipeline {
    /// CheckTable 数据存储
    check_table: CheckTable,
    /// Checkpoint 日志记录器
    logger: Box<dyn CheckpointLogger>,
    /// 指标处理器
    indicator_processor: Box<dyn Processor>,
    /// 策略处理器
    strategy_processor: Box<dyn Processor>,
    /// 风控处理器
    risk_processor: Box<dyn Processor>,
    /// 当前品种
    symbol: String,
}

impl Pipeline {
    /// 创建 Pipeline
    pub fn new(
        symbol: String,
        logger: Box<dyn CheckpointLogger>,
        indicator_processor: Box<dyn Processor>,
        strategy_processor: Box<dyn Processor>,
        risk_processor: Box<dyn Processor>,
    ) -> Self {
        Self {
            check_table: CheckTable::new(),
            logger,
            indicator_processor,
            strategy_processor,
            risk_processor,
            symbol,
        }
    }

    /// 处理单个 Tick - 明确的流程控制
    pub fn process(&mut self, tick: &Tick) -> Option<OrderRequest> {
        let mut stage_results: Vec<StageResult> = Vec::new();
        let mut blocked_at: Option<Stage> = None;

        // 1. 指标计算阶段
        let indicator_result = self.indicator_processor.process(&mut self.check_table, tick);
        self.logger.log_pass(Stage::Indicator, &self.symbol, &indicator_result.details);
        stage_results.push(indicator_result.clone());

        if !indicator_result.passed {
            blocked_at = Some(Stage::Indicator);
            self.logger.log_blocked(
                Stage::Indicator,
                &self.symbol,
                indicator_result.blocked_reason.as_deref().unwrap_or("指标计算失败"),
            );
            self.logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return None;
        }

        // 2. 策略判断阶段
        let strategy_result = self.strategy_processor.process(&mut self.check_table, tick);
        self.logger.log_pass(Stage::Strategy, &self.symbol, &strategy_result.details);
        stage_results.push(strategy_result.clone());

        if !strategy_result.passed {
            blocked_at = Some(Stage::Strategy);
            self.logger.log_blocked(
                Stage::Strategy,
                &self.symbol,
                strategy_result.blocked_reason.as_deref().unwrap_or("策略判断失败"),
            );
            self.logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return None;
        }

        // 3. 风控预检阶段
        let risk_result = self.risk_processor.process(&mut self.check_table, tick);
        self.logger.log_pass(Stage::RiskPre, &self.symbol, &risk_result.details);
        stage_results.push(risk_result.clone());

        if !risk_result.passed {
            blocked_at = Some(Stage::RiskPre);
            self.logger.log_blocked(
                Stage::RiskPre,
                &self.symbol,
                risk_result.blocked_reason.as_deref().unwrap_or("风控预检失败"),
            );
            self.logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return None;
        }

        // 4. 全部通过，获取交易决策
        self.logger.log_checkpoint(&self.symbol, &stage_results, None);
        self.build_order_request()
    }

    /// 从 CheckTable 构建订单请求
    fn build_order_request(&self) -> Option<OrderRequest> {
        let entry = self.check_table.get(&self.symbol, "main", "1m")?;

        if !matches!(
            entry.final_signal,
            Signal::LongEntry | Signal::ShortEntry | Signal::LongHedge | Signal::ShortHedge
        ) {
            return None;
        }

        Some(OrderRequest {
            symbol: self.symbol.clone(),
            side: match entry.final_signal {
                Signal::LongEntry | Signal::LongHedge => Side::Long,
                Signal::ShortEntry | Signal::ShortHedge => Side::Short,
                _ => return None,
            },
            order_type: strategy::types::OrderType::Market,
            price: Some(entry.target_price),
            qty: entry.quantity,
        })
    }

    /// 获取 CheckTable 引用（只读）
    pub fn check_table(&self) -> &CheckTable {
        &self.check_table
    }

    /// 获取 CheckTable 可变引用
    pub fn check_table_mut(&mut self) -> &mut CheckTable {
        &mut self.check_table
    }
}

/// NoOp Logger - 不输出任何日志
pub struct NoOpLogger;

impl NoOpLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for NoOpLogger {
    fn log_start(&self, _: Stage, _: &str) {}
    fn log_pass(&self, _: Stage, _: &str, _: &str) {}
    fn log_blocked(&self, _: Stage, _: &str, _: &str) {}
    fn log_checkpoint(&self, _: &str, _: &[StageResult], _: Option<Stage>) {}
}

/// Mock 指标处理器 - 可配置结果
pub struct MockIndicatorProcessor {
    should_pass: bool,
    details: String,
    blocked_reason: Option<String>,
}

impl MockIndicatorProcessor {
    pub fn new_pass(details: &str) -> Self {
        Self {
            should_pass: true,
            details: details.to_string(),
            blocked_reason: None,
        }
    }

    pub fn new_blocking(reason: &str) -> Self {
        Self {
            should_pass: false,
            details: String::new(),
            blocked_reason: Some(reason.to_string()),
        }
    }
}

impl Processor for MockIndicatorProcessor {
    fn process(&mut self, _check_table: &mut CheckTable, tick: &Tick) -> StageResult {
        if self.should_pass {
            StageResult::pass(Stage::Indicator, &self.details)
        } else {
            StageResult::fail(Stage::Indicator, self.blocked_reason.as_deref().unwrap_or("指标失败"))
        }
    }
}

/// Mock 策略处理器 - 可配置结果，并写入 CheckTable
pub struct MockStrategyProcessor {
    should_pass: bool,
    signal: Signal,
    details: String,
    blocked_reason: Option<String>,
}

impl MockStrategyProcessor {
    /// 创建通过的策略处理器
    pub fn new_pass(signal: Signal, details: &str) -> Self {
        Self {
            should_pass: true,
            signal,
            details: details.to_string(),
            blocked_reason: None,
        }
    }

    /// 创建失败的策略处理器
    pub fn new_blocking(reason: &str) -> Self {
        Self {
            should_pass: false,
            signal: Signal::LongEntry, // 默认值
            details: String::new(),
            blocked_reason: Some(reason.to_string()),
        }
    }

    /// HOLD 信号（不产生订单）
    pub fn new_hold() -> Self {
        Self {
            should_pass: true,
            signal: Signal::LongExit, // 或其他非 Entry 信号
            details: "Signal=HOLD".to_string(),
            blocked_reason: None,
        }
    }
}

impl Processor for MockStrategyProcessor {
    fn process(&mut self, check_table: &mut CheckTable, tick: &Tick) -> StageResult {
        if !self.should_pass {
            return StageResult::fail(Stage::Strategy, self.blocked_reason.as_deref().unwrap_or("策略失败"));
        }

        // 写入 CheckEntry（只有 pass 时才写入）
        let entry = CheckEntry {
            symbol: tick.symbol.clone(),
            strategy_id: "main".to_string(),
            period: "1m".to_string(),
            ema_signal: self.signal,
            rsi_value: dec!(50),
            pine_color: indicator::PineColor::Neutral,
            price_position: dec!(50),
            final_signal: self.signal,
            target_price: tick.price,
            quantity: dec!(0.1),
            risk_flag: false,
            timestamp: tick.timestamp,
            round_id: check_table.next_round_id(),
            is_high_freq: false,
        };
        check_table.fill(entry);

        StageResult::pass(Stage::Strategy, &self.details)
    }
}

/// Mock 风控处理器 - 可配置结果
pub struct MockRiskProcessor {
    should_pass: bool,
    details: String,
    blocked_reason: Option<String>,
}

impl MockRiskProcessor {
    pub fn new_pass(details: &str) -> Self {
        Self {
            should_pass: true,
            details: details.to_string(),
            blocked_reason: None,
        }
    }

    pub fn new_blocking(reason: &str) -> Self {
        Self {
            should_pass: false,
            details: String::new(),
            blocked_reason: Some(reason.to_string()),
        }
    }
}

impl Processor for MockRiskProcessor {
    fn process(&mut self, _check_table: &mut CheckTable, _tick: &Tick) -> StageResult {
        if self.should_pass {
            StageResult::pass(Stage::RiskPre, &self.details)
        } else {
            StageResult::fail(Stage::RiskPre, self.blocked_reason.as_deref().unwrap_or("风控失败"))
        }
    }
}

/// 默认的指标处理器
pub struct DefaultIndicatorProcessor;

impl DefaultIndicatorProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for DefaultIndicatorProcessor {
    fn process(&mut self, _check_table: &mut CheckTable, tick: &Tick) -> StageResult {
        let details = format!(
            "EMA12={:.2} EMA26={:.2} RSI=50.00",
            tick.price * dec!(0.99),
            tick.price
        );
        StageResult::pass(Stage::Indicator, details)
    }
}

/// 默认的策略处理器
pub struct DefaultStrategyProcessor;

impl DefaultStrategyProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for DefaultStrategyProcessor {
    fn process(&mut self, _check_table: &mut CheckTable, _tick: &Tick) -> StageResult {
        StageResult::pass(Stage::Strategy, "Signal=HOLD")
    }
}

/// 默认的风控处理器
pub struct DefaultRiskProcessor;

impl DefaultRiskProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for DefaultRiskProcessor {
    fn process(&mut self, _check_table: &mut CheckTable, _tick: &Tick) -> StageResult {
        StageResult::pass(Stage::RiskPre, "OK")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick(symbol: &str, price: Decimal) -> Tick {
        Tick {
            symbol: symbol.to_string(),
            price,
            qty: dec!(1.0),
            timestamp: chrono::Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        }
    }

    // ========== 完整流程测试 ==========

    #[test]
    fn test_pipeline_long_entry_signal() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA12=49500 EMA26=50000 RSI=55")),
            Box::new(MockStrategyProcessor::new_pass(Signal::LongEntry, "Signal=BUY")),
            Box::new(MockRiskProcessor::new_pass("margin_ratio=10%")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // 完整流程通过，生成 Long 订单
        let result = pipeline.process(&tick);
        assert!(result.is_some());
        let order = result.unwrap();
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, Side::Long);
        assert_eq!(order.price, Some(dec!(50000)));
        assert_eq!(order.qty, dec!(0.1));
    }

    #[test]
    fn test_pipeline_short_entry_signal() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA12=50500 EMA26=50000 RSI=45")),
            Box::new(MockStrategyProcessor::new_pass(Signal::ShortEntry, "Signal=SELL")),
            Box::new(MockRiskProcessor::new_pass("margin_ratio=10%")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // 完整流程通过，生成 Short 订单
        let result = pipeline.process(&tick);
        assert!(result.is_some());
        let order = result.unwrap();
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, Side::Short);
    }

    #[test]
    fn test_pipeline_hedge_signal() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA12=49000 EMA26=50000")),
            Box::new(MockStrategyProcessor::new_pass(Signal::LongHedge, "Hedge=BUY")),
            Box::new(MockRiskProcessor::new_pass("OK")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // LongHedge 信号，生成 Long 订单
        let result = pipeline.process(&tick);
        assert!(result.is_some());
        assert_eq!(result.unwrap().side, Side::Long);
    }

    #[test]
    fn test_pipeline_hold_signal_no_order() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA12=50000")),
            Box::new(MockStrategyProcessor::new_hold()), // HOLD 信号
            Box::new(MockRiskProcessor::new_pass("OK")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // Strategy 返回 HOLD，不生成订单
        let result = pipeline.process(&tick);
        assert!(result.is_none());
    }

    // ========== 阶段失败拦截测试 ==========

    #[test]
    fn test_pipeline_indicator_blocked() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_blocking("EMA 计算超时")),
            Box::new(MockStrategyProcessor::new_pass(Signal::LongEntry, "Signal=BUY")),
            Box::new(MockRiskProcessor::new_pass("OK")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // Indicator 失败，Pipeline 停止，不生成订单
        let result = pipeline.process(&tick);
        assert!(result.is_none());

        // CheckTable 不应有数据（因为 Strategy 未执行）
        assert!(pipeline.check_table().get("BTCUSDT", "main", "1m").is_none());
    }

    #[test]
    fn test_pipeline_strategy_blocked() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA12=49500")),
            Box::new(MockStrategyProcessor::new_blocking("TR_RATIO < 1")),
            Box::new(MockRiskProcessor::new_pass("OK")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // Strategy 失败，Pipeline 停止
        let result = pipeline.process(&tick);
        assert!(result.is_none());
    }

    #[test]
    fn test_pipeline_risk_blocked() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA12=49500")),
            Box::new(MockStrategyProcessor::new_pass(Signal::LongEntry, "Signal=BUY")),
            Box::new(MockRiskProcessor::new_blocking("保证金不足")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // RiskPre 失败，Pipeline 停止
        let result = pipeline.process(&tick);
        assert!(result.is_none());
    }

    // ========== ConsoleCheckpointLogger 日志测试 ==========

    #[test]
    fn test_pipeline_with_console_logger() {
        let mut pipeline = Pipeline::new(
            "ETHUSDT".to_string(),
            Box::new(ConsoleCheckpointLogger::new()),
            Box::new(MockIndicatorProcessor::new_pass("EMA=3000")),
            Box::new(MockStrategyProcessor::new_pass(Signal::LongEntry, "Signal=BUY")),
            Box::new(MockRiskProcessor::new_pass("OK")),
        );

        let tick = make_tick("ETHUSDT", dec!(3000));

        // Console logger 不会 panic，会输出日志
        let result = pipeline.process(&tick);
        assert!(result.is_some());
    }

    #[test]
    fn test_pipeline_console_logger_blocked() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(ConsoleCheckpointLogger::new()),
            Box::new(MockIndicatorProcessor::new_blocking("指标失败")),
            Box::new(MockStrategyProcessor::new_pass(Signal::LongEntry, "Signal=BUY")),
            Box::new(MockRiskProcessor::new_pass("OK")),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // 日志会显示 Blocked at Indicator
        let result = pipeline.process(&tick);
        assert!(result.is_none());
    }

    // ========== 默认处理器测试 ==========

    #[test]
    fn test_pipeline_with_default_processors() {
        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(DefaultIndicatorProcessor::new()),
            Box::new(DefaultStrategyProcessor::new()),
            Box::new(DefaultRiskProcessor::new()),
        );

        let tick = make_tick("BTCUSDT", dec!(50000));

        // Default processor 返回 HOLD，所以没有订单
        let result = pipeline.process(&tick);
        assert!(result.is_none());
    }
}
