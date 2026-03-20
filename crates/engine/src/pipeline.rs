#![forbid(unsafe_code)]

use crate::checkpoint::{CheckpointLogger, Stage, StageResult};
use crate::check_table::CheckTable;
use market::types::Tick;
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

    #[test]
    fn test_pipeline_with_noop_logger() {
        let pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger::new()),
            Box::new(DefaultIndicatorProcessor::new()),
            Box::new(DefaultStrategyProcessor::new()),
            Box::new(DefaultRiskProcessor::new()),
        );

        let tick = Tick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50000),
            qty: dec!(1.0),
            timestamp: chrono::Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        // NoOp logger 不会 panic
        let result = pipeline.process(&tick);
        // Default processor 返回 HOLD，所以没有订单
        assert!(result.is_none());
    }
}
