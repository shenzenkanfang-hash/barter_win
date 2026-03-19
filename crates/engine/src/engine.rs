use account::types::FundPool;
use engine::order::OrderExecutor;
use engine::risk::RiskPreChecker;
use indicator::{EMA, RSI};
use market::{KLineSynthesizer, MarketStream, Period, Tick};
use rust_decimal::Decimal;
use std::sync::Arc;
use strategy::types::{OrderRequest, Signal};
use strategy::{Strategy, StrategyId};
use tracing::{info, warn};

/// 交易引擎 - 串联所有层
pub struct TradingEngine {
    // 市场数据
    market_stream: Box<dyn MarketStream>,

    // K线合成器
    kline_1m: KLineSynthesizer,
    kline_1d: KLineSynthesizer,

    // 指标
    ema_fast: EMA,
    ema_slow: EMA,
    rsi: RSI,

    // 风控
    risk_checker: RiskPreChecker,

    // 订单执行
    order_executor: OrderExecutor,

    // 资金池
    fund_pool: FundPool,

    // 策略实例 (简化版)
    strategy_id: StrategyId,
}

impl TradingEngine {
    pub fn new(
        market_stream: Box<dyn MarketStream>,
        symbol: String,
        fund_pool: FundPool,
    ) -> Self {
        Self {
            market_stream,
            kline_1m: KLineSynthesizer::new(symbol.clone(), Period::Minute(1)),
            kline_1d: KLineSynthesizer::new(symbol.clone(), Period::Day),
            ema_fast: EMA::new(12),
            ema_slow: EMA::new(26),
            rsi: RSI::new(14),
            risk_checker: RiskPreChecker::new(
                Decimal::try_from(0.95).unwrap(), // 95% 最大仓位
                Decimal::try_from(1000.0).unwrap(), // 最低保留 1000
            ),
            order_executor: OrderExecutor::new(),
            fund_pool,
            strategy_id: StrategyId("main".to_string()),
        }
    }

    /// 处理单个 tick
    pub async fn on_tick(&mut self, tick: &Tick) {
        // 1. 更新 K线
        let completed_1m = self.kline_1m.update(tick);
        let completed_1d = self.kline_1d.update(tick);

        // 2. 更新指标
        self.update_indicators(tick.price);

        // 3. 如果有完成的 K线，生成信号
        if let Some(kline) = completed_1m {
            self.on_kline_completed(&kline);
        }

        // 4. 演示: 打印当前状态
        self.print_status(tick);
    }

    fn update_indicators(&mut self, price: Decimal) {
        // 更新 EMA (使用 calculate 方法)
        let _ema_f = self.ema_fast.calculate(price);
        let _ema_s = self.ema_slow.calculate(price);

        // 更新 RSI (使用 calculate 方法)
        let _rsi_value = self.rsi.calculate(_ema_f - _ema_s);

        // 后续可以在这里调用 PineColorDetector::detect() 进行颜色检测
    }

    fn on_kline_completed(&mut self, kline: &market::types::KLine) {
        // 当 1 分钟 K 线完成时，可以在这里做更复杂的策略判断
        info!(
            "K线完成: {} {} close={}",
            kline.symbol, kline.period, kline.close
        );
    }

    fn print_status(&self, tick: &Tick) {
        info!(
            "Tick: symbol={} price={} qty={}",
            tick.symbol, tick.price, tick.qty
        );
    }

    /// 执行订单
    pub async fn execute_order(&self, order: OrderRequest) -> Result<(), engine::EngineError> {
        // 1. 风控预检
        let order_value = order.qty * order.price.unwrap_or(order.qty);
        self.risk_checker.pre_check(
            self.fund_pool.available,
            order_value,
            self.fund_pool.total_equity,
        )?;

        // 2. 执行订单
        match order.order_type {
            strategy::types::OrderType::Market => {
                self.order_executor.execute_market_order(&order)?;
            }
            strategy::types::OrderType::Limit => {
                self.order_executor.execute_limit_order(&order)?;
            }
        }

        Ok(())
    }

    /// 主循环
    pub async fn run(&mut self) {
        info!("TradingEngine 启动");

        loop {
            // 从市场获取 tick
            if let Some(tick) = self.market_stream.next_tick().await {
                self.on_tick(&tick).await;
            } else {
                warn!("市场数据流结束");
                break;
            }
        }
    }

    /// 带超时的运行 (用于测试模拟)
    pub async fn run_with_timeout(&mut self, seconds: u64) {
        info!("TradingEngine 启动 (超时: {}秒)", seconds);

        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < seconds {
            // 从市场获取 tick
            if let Some(tick) = self.market_stream.next_tick().await {
                self.on_tick(&tick).await;
            } else {
                warn!("市场数据流结束");
                break;
            }
            // 小延迟避免过于密集
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        info!("TradingEngine 超时退出");
    }
}
