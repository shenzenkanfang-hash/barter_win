use crate::account_pool::{AccountPool, CircuitBreakerState};
use crate::check_table::CheckTable;
use crate::market_status::{MarketStatus, MarketStatusDetector};
use crate::mode::ModeSwitcher;
use crate::order::OrderExecutor;
use crate::order_check::OrderCheck;
use crate::persistence::PersistenceService;
use crate::position_exclusion::PositionExclusionChecker;
use crate::position_manager::{Direction, LocalPositionManager};
use crate::pnl_manager::PnlManager;
use crate::risk::RiskPreChecker;
use crate::risk::VolatilityMode;
use crate::risk_rechecker::RiskReChecker;
use crate::round_guard::{RoundGuard, RoundGuardScope};
use crate::strategy_pool::StrategyPool;
use crate::thresholds::ThresholdConstants;
use indicator::{EMA, RSI};
use market::{KLineSynthesizer, MarketStream, Period, Tick};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::time::Duration;
use strategy::types::{OrderRequest, Side};
use strategy::StrategyId;
use tracing::{info, warn};

/// 交易引擎 - 串联所有层
///
/// 集成所有 Phase 7 Enhancement 模块:
/// - AccountPool: 账户保证金池 (熔断保护)
/// - StrategyPool: 策略资金池 (再平衡)
/// - RiskReChecker: 风控锁内复核
/// - PnlManager: 盈亏管理
/// - MarketStatusDetector: 市场状态检测
/// - PositionExclusionChecker: 仓位互斥检查
/// - OrderCheck: 订单预占检查
/// - PersistenceService: 持久化服务
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

    // 风控预检 (锁外)
    risk_checker: RiskPreChecker,

    // 风控锁内复核
    risk_rechecker: RiskReChecker,

    // 模式切换器
    mode_switcher: ModeSwitcher,

    // 市场状态检测
    market_detector: MarketStatusDetector,

    // 仓位互斥检查
    position_exclusion: PositionExclusionChecker,

    // 订单预占检查
    order_check: OrderCheck,

    // 持仓管理器
    position_manager: LocalPositionManager,

    // 盈亏管理器
    pnl_manager: PnlManager,

    // 账户保证金池
    account_pool: AccountPool,

    // 策略资金池
    strategy_pool: StrategyPool,

    // 持久化服务
    persistence: PersistenceService,

    // 一轮编码守卫
    round_guard: RoundGuard,

    // Check 表
    check_table: CheckTable,

    // 阈值常量
    thresholds: ThresholdConstants,

    // 订单执行
    order_executor: OrderExecutor,

    // 策略实例
    #[allow(dead_code)]
    strategy_id: StrategyId,

    // 当前交易对
    symbol: String,

    // 当前时间戳
    current_ts: i64,

    // 当前市场价格
    current_price: Decimal,
}

impl TradingEngine {
    /// 创建交易引擎
    ///
    /// # 参数
    /// * `market_stream` - 市场数据流
    /// * `symbol` - 交易品种
    /// * `initial_balance` - 初始资金
    pub fn new(
        market_stream: Box<dyn MarketStream>,
        symbol: String,
        initial_balance: Decimal,
    ) -> Self {
        Self {
            market_stream,
            kline_1m: KLineSynthesizer::new(symbol.clone(), Period::Minute(1)),
            kline_1d: KLineSynthesizer::new(symbol.clone(), Period::Day),
            ema_fast: EMA::new(12),
            ema_slow: EMA::new(26),
            rsi: RSI::new(14),
            risk_checker: RiskPreChecker::new(
                Decimal::try_from(0.95).unwrap(),
                Decimal::try_from(1000.0).unwrap(),
            ),
            risk_rechecker: RiskReChecker::new(),
            mode_switcher: ModeSwitcher::new(),
            market_detector: MarketStatusDetector::new(),
            position_exclusion: PositionExclusionChecker::new(),
            order_check: OrderCheck::new(),
            position_manager: LocalPositionManager::new(),
            pnl_manager: PnlManager::new(),
            account_pool: AccountPool::with_config(
                initial_balance,
                Decimal::try_from(0.20).unwrap(),
                Decimal::try_from(0.10).unwrap(),
            ),
            strategy_pool: StrategyPool::new(),
            persistence: PersistenceService::new(),
            round_guard: RoundGuard::new(),
            check_table: CheckTable::new(),
            thresholds: ThresholdConstants::production(),
            order_executor: OrderExecutor::new(),
            strategy_id: StrategyId("main".to_string()),
            symbol,
            current_ts: 0,
            current_price: Decimal::ZERO,
        }
    }

    /// 处理单个 tick
    pub async fn on_tick(&mut self, tick: &Tick) {
        self.current_ts = tick.timestamp.timestamp();
        self.current_price = tick.price;

        // 1. 更新 K线
        let completed_1m = self.kline_1m.update(tick);
        let completed_1d = self.kline_1d.update(tick);

        // 2. 更新指标
        self.update_indicators(tick.price);

        // 3. 风控预检 (锁外)
        self.pre_trade_check(tick);

        // 4. 如果有完成的 K线，生成信号
        if let Some(kline) = completed_1m {
            self.on_kline_completed(&kline);
        }

        // 5. 日线 K线完成处理
        if let Some(kline) = completed_1d {
            self.on_daily_kline_completed(&kline);
        }

        // 6. 打印状态
        self.print_status(tick);
    }

    fn update_indicators(&mut self, price: Decimal) {
        // 更新 EMA
        let ema_f = self.ema_fast.calculate(price);
        let ema_s = self.ema_slow.calculate(price);

        // 更新 RSI
        let _rsi_value = self.rsi.calculate(ema_f - ema_s);

        // 更新市场状态检测
        let _market_status = self.market_detector.detect(
            dec!(1.0), // tr_ratio - 默认值
            dec!(0.0), // zscore - 默认值
            dec!(0.01), // volatility - 默认低波动
            dec!(50.0), // price_position - 默认中间值
            true, // is_data_valid
            self.current_ts, // last_update_ts
            self.current_ts, // current_ts
        );
    }

    fn pre_trade_check(&self, tick: &Tick) {
        let order_value = tick.price * tick.qty;

        // 检查账户是否可以交易
        if !self.account_pool.can_trade(order_value) {
            return;
        }

        // 检查策略是否可以开仓
        if !self.strategy_pool.can_open_position("main", order_value) {
            return;
        }
    }

    fn on_kline_completed(&mut self, kline: &market::types::KLine) {
        info!(
            "1分钟K线完成: {} close={} high={} low={}",
            kline.symbol, kline.close, kline.high, kline.low
        );
    }

    fn on_daily_kline_completed(&mut self, kline: &market::types::KLine) {
        info!(
            "日线K线完成: {} close={}",
            kline.symbol, kline.close
        );

        // 日线 K线完成，可能需要重新平衡策略
        if self.strategy_pool.needs_rebalance(self.current_ts) {
            self.strategy_pool.rebalance(self.account_pool.total_equity(), self.current_ts);
        }

        // 持久化交易记录
        self.persistence.record_daily_kline(kline);
    }

    fn print_status(&self, tick: &Tick) {
        let unrealized = self.position_manager.unrealized_pnl(tick.price);
        info!(
            "Tick: {} price={} | 账户: 可用={} 熔断={:?} | 未实现盈亏={}",
            tick.symbol,
            tick.price,
            self.account_pool.available(),
            self.account_pool.circuit_state(),
            unrealized,
        );
    }

    /// 执行订单 (带锁内复核)
    pub async fn execute_order(&mut self, order: OrderRequest) -> Result<(), crate::EngineError> {
        let order_value = order.qty * order.price.unwrap_or(order.qty);

        // 1. 风控预检 (锁外) - 使用 AccountPool
        self.risk_checker.pre_check(
            &order.symbol,
            self.account_pool.available(),
            order_value,
            self.account_pool.total_equity(),
        )?;

        // 2. 预占保证金
        self.strategy_pool.reserve_margin("main", order_value)
            .map_err(|e| crate::EngineError::RiskCheckFailed(e))?;

        // 3. 一轮编码作用域 (RAII 自动管理)
        let _round_scope = RoundGuardScope::new(&self.round_guard);

        // 4. 风控锁内复核
        self.risk_rechecker.re_check(
            self.account_pool.available(),
            order_value,
            self.current_price,
            self.current_price,
            VolatilityMode::Normal, // 默认使用正常模式
        )?;

        // 5. 执行订单
        match order.order_type {
            strategy::types::OrderType::Market => {
                self.order_executor.execute_market_order(&order)?;
            }
            strategy::types::OrderType::Limit => {
                self.order_executor.execute_limit_order(&order)?;
            }
        }

        // 6. 更新持仓
        let direction = match order.side {
            Side::Long => Direction::Long,
            Side::Short => Direction::Short,
        };
        self.position_manager.open_position(direction, order.qty, order.price.unwrap_or(order.qty), self.current_ts);

        Ok(())
    }

    /// 获取市场状态
    pub fn market_status(&self) -> MarketStatus {
        // 默认返回 TREND 状态
        MarketStatus::TREND
    }

    /// 获取熔断状态
    pub fn circuit_state(&self) -> CircuitBreakerState {
        self.account_pool.circuit_state()
    }

    /// 获取未实现盈亏
    pub fn unrealized_pnl(&self) -> Decimal {
        self.position_manager.unrealized_pnl(self.current_price)
    }

    /// 获取账户信息
    pub fn account_info(&self) -> &AccountPool {
        &self.account_pool
    }

    /// 获取策略池
    pub fn strategy_pool_info(&self) -> &StrategyPool {
        &self.strategy_pool
    }

    /// 主循环
    pub async fn run(&mut self) {
        info!("TradingEngine 启动");

        loop {
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
            if let Some(tick) = self.market_stream.next_tick().await {
                self.on_tick(&tick).await;
            } else {
                warn!("市场数据流结束");
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        info!("TradingEngine 超时退出");
    }
}
