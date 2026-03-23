//! TradingEngine 核心引擎 - 最小化实现
//!
//! 简化版：只保留核心功能

use c_data_process::SignalProcessor;
use c_data_process::types::TradingAction;
use crate::types::{Mode, ModeSwitcher, OrderRequest, OrderType, Side, TradingDecision};
use crate::core::{SymbolState, CheckConfig, StartupState};
use b_data_source::ws::KLineSynthesizer;
use b_data_source::{MarketStream, Period, Tick};
use e_risk_monitor::risk::RiskPreChecker;
use e_risk_monitor::shared::account_pool::AccountPool;
use e_risk_monitor::shared::pnl_manager::PnlManager;
use e_risk_monitor::position::position_manager::LocalPositionManager;
use e_risk_monitor::shared::round_guard::RoundGuard;
use e_risk_monitor::risk::{RiskReChecker, OrderCheck, ThresholdConstants};
use e_risk_monitor::position::position_exclusion::PositionExclusionChecker;
use e_risk_monitor::shared::market_status::MarketStatusDetector;
use e_risk_monitor::persistence::disaster_recovery::DisasterRecovery;
use e_risk_monitor::persistence::PersistenceService;
use e_risk_monitor::position::position_manager::Direction;
use d_checktable::check_table::CheckTable;
use a_common::EngineError;
use a_common::backup::MemoryBackup;
use crate::order::gateway::ExchangeGateway;
use crate::order::OrderExecutor;
use crate::core::strategy_pool::StrategyPool;
use c_data_process::{EMA, RSI};
use fnv::FnvHashMap;
use futures::future::join_all;
use parking_lot::{Mutex, RwLock};
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

// ============================================================================
// TradingEngine 主结构
// ============================================================================

/// 交易引擎 - 串联所有层
pub struct TradingEngine {
    market_stream: Arc<Mutex<Box<dyn MarketStream>>>,
    kline_1m: KLineSynthesizer,
    ema_fast: EMA,
    ema_slow: EMA,
    rsi: RSI,
    risk_checker: Arc<RiskPreChecker>,
    risk_rechecker: RiskReChecker,
    mode_switcher: ModeSwitcher,
    market_detector: MarketStatusDetector,
    position_exclusion: PositionExclusionChecker,
    order_check: OrderCheck,
    position_manager: LocalPositionManager,
    pnl_manager: PnlManager,
    account_pool: AccountPool,
    strategy_pool: StrategyPool,
    persistence: PersistenceService,
    memory_backup: Option<Arc<MemoryBackup>>,
    disaster_recovery: Option<Arc<DisasterRecovery>>,
    round_guard: RoundGuard,
    check_table: CheckTable,
    thresholds: ThresholdConstants,
    gateway: Arc<dyn ExchangeGateway>,
    order_executor: OrderExecutor,
    symbol: String,
    current_price: Decimal,
    is_running: Arc<AtomicBool>,
    symbol_states: RwLock<FnvHashMap<String, SymbolState>>,
    check_config: CheckConfig,
    signal_processor: Arc<SignalProcessor>,
}

impl TradingEngine {
    /// 创建交易引擎
    pub fn new(
        market_stream: Box<dyn MarketStream>,
        symbol: String,
        initial_balance: Decimal,
        gateway: Arc<dyn ExchangeGateway>,
    ) -> Self {
        let risk_checker = Arc::new(RiskPreChecker::new(
            Decimal::try_from(0.95).unwrap(),
            Decimal::try_from(1000.0).unwrap(),
        ));

        let account_pool = AccountPool::with_config(
            initial_balance,
            Decimal::try_from(0.20).unwrap(),
            Decimal::try_from(0.10).unwrap(),
        );

        let strategy_pool = StrategyPool::new();

        let order_executor = OrderExecutor::new(
            gateway.clone() as Arc<dyn ExchangeGateway>,
            risk_checker.clone(),
        );

        Self {
            market_stream: Arc::new(Mutex::new(market_stream)),
            kline_1m: KLineSynthesizer::new(symbol.clone(), Period::Minute(1)),
            ema_fast: EMA::new(12),
            ema_slow: EMA::new(26),
            rsi: RSI::new(14),
            risk_checker,
            risk_rechecker: RiskReChecker::new(),
            mode_switcher: ModeSwitcher::new(),
            market_detector: MarketStatusDetector::new(),
            position_exclusion: PositionExclusionChecker::new(),
            order_check: OrderCheck::new(),
            position_manager: LocalPositionManager::new(),
            pnl_manager: PnlManager::new(),
            account_pool,
            strategy_pool,
            persistence: PersistenceService::new(),
            memory_backup: None,
            disaster_recovery: None,
            round_guard: RoundGuard::new(),
            check_table: CheckTable::new(),
            thresholds: ThresholdConstants::production(),
            gateway,
            order_executor,
            symbol: symbol.clone(),
            current_price: Decimal::ZERO,
            is_running: Arc::new(AtomicBool::new(false)),
            symbol_states: RwLock::new({
                let mut map = FnvHashMap::default();
                map.insert(symbol.clone(), SymbolState::new(symbol));
                map
            }),
            check_config: CheckConfig::production(),
            signal_processor: Arc::new(SignalProcessor::new()),
        }
    }

    /// 处理单个 tick - 主入口
    pub async fn on_tick(&mut self, tick: &Tick) {
        let now_ts = tick.timestamp.timestamp();
        self.current_price = tick.price;

        // 1. K线增量更新
        let _completed_1m = self.kline_1m.update(tick);

        // 2. 实时价格位置
        self.update_price_position(tick);

        // 3. 分钟K线完成 → 分钟级策略
        if let Some(_kline) = _completed_1m {
            self.on_minute_bar(tick).await;
        }
    }

    /// 实时价格位置判断
    fn update_price_position(&mut self, tick: &Tick) {
        info!("Price update: price={}", tick.price);
    }

    /// 分钟K线完成回调
    async fn on_minute_bar(&mut self, tick: &Tick) {
        if !self.mode_switcher.is_trading_allowed() {
            return;
        }

        let now_ts = tick.timestamp.timestamp();

        // 并发检查所有品种
        let symbols: Vec<String> = self.symbol_states.read().keys().cloned().collect();
        let mut handles = Vec::new();

        for symbol in symbols {
            let signal_processor = self.signal_processor.clone();
            let states = Arc::new(RwLock::new(self.symbol_states.read().clone()));
            let now = now_ts;

            let handle = tokio::spawn(async move {
                Self::check_minute_strategy_inner(&symbol, now, signal_processor, states).await
            });
            handles.push(handle);
        }

        let results: Vec<bool> = join_all(handles)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let success_count = results.iter().filter(|r| **r).count();
        info!("Minute check: {}/{} success", success_count, results.len());
    }

    /// 内部：分钟级策略检查
    async fn check_minute_strategy_inner(
        symbol: &str,
        now_ts: i64,
        signal_processor: Arc<SignalProcessor>,
        states: Arc<RwLock<FnvHashMap<String, SymbolState>>>,
    ) -> bool {
        let mut states_write = states.write();
        let state = match states_write.get_mut(symbol) {
            Some(s) => s,
            None => return false,
        };

        // 灾备重启检查
        if matches!(state.startup_state, StartupState::Recovery) {
            // Recovery 状态需要等待新鲜信号
            info!("Recovery in progress for {}", symbol);
        }

        // 首次请求时间记录
        if state.last_1m_request_ts == 0 {
            state.record_1m_request(now_ts);
        }

        // 超时检查
        if state.is_1m_timeout(now_ts) {
            warn!("Minute timeout for {}", symbol);
            return false;
        }

        // 获取信号
        if let Some((decision, signal_ts)) = signal_processor.get_min_signal(symbol) {
            let signal_age = now_ts - signal_ts;

            if signal_age > state.timeout_secs {
                warn!("Signal delay: age={}s > {}s", signal_age, state.timeout_secs);
                return false;
            }

            // 记录成功
            state.record_1m_ok(now_ts, signal_ts, decision);
            return true;
        }

        false
    }

    /// 检查日线级策略
    fn check_daily_strategies_batch(&mut self) {
        let symbols: Vec<String> = self.symbol_states.read().keys().cloned().collect();

        for symbol in symbols {
            self.check_daily_strategy(&symbol);
        }
    }

    /// 检查单品种日线级策略
    fn check_daily_strategy(&mut self, symbol: &str) {
        let now_ts = chrono::Utc::now().timestamp();

        let mut states = self.symbol_states.write();
        let state = match states.get_mut(symbol) {
            Some(s) => s,
            None => return,
        };

        if state.last_daily_request_ts == 0 {
            state.record_daily_request(now_ts);
        }

        if state.is_daily_timeout(now_ts) {
            warn!("Daily timeout for {}", symbol);
            self.mode_switcher.set_mode(Mode::Maintenance);
            return;
        }

        if let Some((decision, _)) = self.signal_processor.get_day_signal(symbol) {
            state.record_daily_ok(now_ts, decision);
        }
    }

    /// 执行交易决策
    pub async fn execute_decision(&mut self, symbol: &str, decision: TradingDecision) -> bool {
        // 1. 风控预检
        if !self.mode_switcher.is_trading_allowed() {
            warn!("Trading not allowed");
            return false;
        }

        // 2. 风控预检 - 订单价值检查
        let order_value = decision.qty * decision.price;
        if !self.pre_trade_check(order_value) {
            warn!("Pre-trade check failed for {}", symbol);
            return false;
        }

        // 3. 构建订单请求
        let side = match decision.action {
            TradingAction::Long => Side::Buy,
            TradingAction::Short => Side::Sell,
            TradingAction::Flat => {
                // 平仓 - 需要根据当前持仓方向决定
                info!("Flat action received for {}", symbol);
                return self.execute_flat(symbol, decision.price).await;
            }
            TradingAction::Hedge | TradingAction::Wait => {
                // 不执行
                info!("No action: {:?}", decision.action);
                return false;
            }
        };

        // 4. 执行订单
        let order = OrderRequest {
            symbol: symbol.to_string(),
            side,
            order_type: OrderType::Market,
            qty: decision.qty,
            price: Some(decision.price),
        };

        match self.execute_order_internal(order).await {
            Ok(_) => {
                info!("Order executed: {} {:?} {}@{}",
                    symbol, side, decision.qty, decision.price);

                // 更新交易锁
                let mut states = self.symbol_states.write();
                if let Some(state) = states.get_mut(symbol) {
                    state.trade_lock.update(
                        decision.timestamp,
                        decision.qty,
                        decision.price,
                    );
                }

                true
            }
            Err(e) => {
                warn!("Order failed for {}: {}", symbol, e);
                false
            }
        }
    }

    /// 执行平仓
    async fn execute_flat(&mut self, symbol: &str, price: Decimal) -> bool {
        // 获取当前持仓方向 (同步调用)
        if let Ok(Some(pos)) = self.gateway.get_position(symbol) {
            let side = match pos.direction.as_str() {
                "long" => Side::Sell,
                "short" => Side::Buy,
                _ => return false,
            };

            let order = OrderRequest {
                symbol: symbol.to_string(),
                side,
                order_type: OrderType::Market,
                qty: pos.quantity,
                price: Some(price),
            };

            match self.execute_order_internal(order).await {
                Ok(_) => {
                    info!("Flat position: {} qty={}", symbol, pos.quantity);
                    true
                }
                Err(e) => {
                    warn!("Flat failed for {}: {}", symbol, e);
                    false
                }
            }
        } else {
            info!("No position to flat for {}", symbol);
            false
        }
    }

    /// 内部订单执行
    async fn execute_order_internal(&mut self, order: OrderRequest) -> Result<a_common::OrderResult, EngineError> {
        self.order_executor.execute(
            &order.symbol,
            order.side,
            order.qty,
            order.price.unwrap_or(self.current_price),
            order.order_type,
        )
    }

    /// 风控预检
    fn pre_trade_check(&self, order_value: Decimal) -> bool {
        if !self.mode_switcher.is_trading_allowed() {
            return false;
        }

        // 检查订单价值是否合理
        if order_value <= Decimal::ZERO {
            return false;
        }

        // TODO: 添加更多风控检查
        // - 账户余额检查
        // - 持仓限制检查
        // - 风险敞口检查

        true
    }

    /// 停止引擎
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        info!("Trading engine stopped");
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// 获取网关
    pub fn gateway(&self) -> &Arc<dyn ExchangeGateway> {
        &self.gateway
    }
}
