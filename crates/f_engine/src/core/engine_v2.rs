//! TradingEngine v2 - 基于接口的解耦架构
//!
//! 新架构遵循：
//! 1. 模块隔离：禁止直接访问其他模块内部
//! 2. 接口强制：所有跨模块调用通过 Trait 接口
//! 3. 依赖注入：核心组件通过构造函数注入
//!
//! 架构图：
//!
//! ```text
//! +-----------------+
//! |   f_engine      |
//! |  (本模块)        |
//! +--------+--------+
//!          |
//!          v
//! +--------+--------+
//! | interfaces/    | <-- 统一的接口层
//! +--------+--------+
//!          |
//! +--------+--------+--------+
//! |        |        |        |
//! v        v        v        v
//! +--+  +-----+  +------+  +------+
//! |b_|  |c_d_|  |e_risk|  |a_com_|
//! |data|  |proc|  |_monit|  |mon---|
//! +--+  +-----+  +------+  +------+
//!
//! 通信规则：
//! - f_engine 通过接口访问 b_data_source
//! - f_engine 通过接口访问 c_data_process
//! - f_engine 通过接口访问 e_risk_monitor
//! - 所有内部通信走接口，不直接访问实现
//! ```

use crate::interfaces::{
    market_data::{MarketDataProvider, MarketKLine, MarketTick},
    strategy::{StrategyExecutor as StrategyExecutorTrait, TradingSignal as StrategySignal},
    risk::{RiskChecker, OrderRequest as RiskOrderRequest, AccountInfo, RiskCheckResult},
    execution::{ExchangeGateway as ExecutionGateway, OrderResult as ExecutionOrderResult},
};
use crate::interfaces::strategy::SignalDirection;
use crate::interfaces::risk::OrderSide as RiskOrderSide;
use chrono::Utc;
use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

/// 交易模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingMode {
    /// 实盘交易
    Live,
    /// 回测模式
    Backtest,
    /// 回放模式
    Replay,
}

impl Default for TradingMode {
    fn default() -> Self {
        Self::Live
    }
}

/// 引擎状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Initialized,
    Running,
    Paused,
    Stopped,
}

impl Default for EngineState {
    fn default() -> Self {
        Self::Initialized
    }
}

/// 品种状态
#[derive(Debug, Clone)]
pub struct SymbolState {
    pub symbol: String,
    pub mode: TradingMode,
    pub current_price: Decimal,
    pub last_tick_time: Option<chrono::DateTime<Utc>>,
    pub last_signal_time: Option<chrono::DateTime<Utc>>,
    pub trade_lock_until: Option<i64>,
    pub is_active: bool,
}

impl SymbolState {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            mode: TradingMode::Live,
            current_price: Decimal::ZERO,
            last_tick_time: None,
            last_signal_time: None,
            trade_lock_until: None,
            is_active: true,
        }
    }

    /// 检查是否可以交易
    pub fn can_trade(&self, now_ts: i64) -> bool {
        if !self.is_active {
            return false;
        }
        if let Some(lock_until) = self.trade_lock_until {
            if now_ts < lock_until {
                return false;
            }
        }
        true
    }

    /// 设置交易锁
    pub fn set_trade_lock(&mut self, until_ts: i64) {
        self.trade_lock_until = Some(until_ts);
    }
}

/// TradingEngine - 解耦后的核心引擎
///
/// 核心设计原则：
/// 1. **接口注入**：所有依赖通过构造函数注入，不直接依赖实现
/// 2. **模块隔离**：不直接访问 b_data_source、c_data_process 等内部
/// 3. **状态封装**：内部状态完全封装，不暴露给外部
///
/// # 泛型约束
/// - `M`: 市场数据提供者，必须实现 MarketDataProvider
/// - `S`: 策略执行器，必须实现 StrategyExecutor
/// - `R`: 风控检查器，必须实现 RiskChecker
/// - `G`: 交易所网关，必须实现 ExchangeGateway
pub struct TradingEngine<M, S, R, G>
where
    M: MarketDataProvider,
    S: StrategyExecutorTrait,
    R: RiskChecker,
    G: ExecutionGateway,
{
    /// 市场数据源（接口注入）
    market_data: Arc<M>,
    
    /// 策略执行器（接口注入）
    strategy_executor: Arc<S>,
    
    /// 风控检查器（接口注入）
    risk_checker: Arc<R>,
    
    /// 交易所网关（接口注入）
    gateway: Arc<G>,
    
    /// 交易模式
    mode: TradingMode,
    
    /// 引擎状态
    state: RwLock<EngineState>,
    
    /// 是否正在运行
    is_running: Arc<AtomicBool>,
    
    /// 品种状态映射
    symbol_states: RwLock<FnvHashMap<String, SymbolState>>,
    
    /// 当前处理的 symbol
    current_symbol: String,
    
    /// 初始资金
    initial_balance: Decimal,
}

impl<M, S, R, G> TradingEngine<M, S, R, G>
where
    M: MarketDataProvider,
    S: StrategyExecutorTrait,
    R: RiskChecker,
    G: ExecutionGateway,
{
    /// 创建引擎
    ///
    /// # 设计说明
    /// 所有核心组件通过参数注入，而非直接实例化：
    /// - 允许注入 Mock 实现进行测试
    /// - 允许替换具体实现（如从实盘切换到回测）
    /// - 符合依赖反转原则 (DIP)
    pub fn new(
        market_data: Arc<M>,
        strategy_executor: Arc<S>,
        risk_checker: Arc<R>,
        gateway: Arc<G>,
        symbol: String,
        initial_balance: Decimal,
        mode: TradingMode,
    ) -> Self {
        let symbol_states = RwLock::new({
            let mut map = FnvHashMap::default();
            map.insert(symbol.clone(), SymbolState::new(symbol.clone()));
            map
        });

        Self {
            market_data,
            strategy_executor,
            risk_checker,
            gateway,
            mode,
            state: RwLock::new(EngineState::Initialized),
            is_running: Arc::new(AtomicBool::new(false)),
            symbol_states,
            current_symbol: symbol,
            initial_balance,
        }
    }

    /// 启动引擎
    pub async fn start(&mut self) -> Result<(), EngineError> {
        if *self.state.read() == EngineState::Running {
            return Err(EngineError::AlreadyRunning);
        }

        info!("Starting trading engine in {:?} mode", self.mode);
        
        *self.state.write() = EngineState::Running;
        self.is_running.store(true, Ordering::SeqCst);
        
        Ok(())
    }

    /// 停止引擎
    pub fn stop(&mut self) {
        info!("Stopping trading engine");
        *self.state.write() = EngineState::Stopped;
        self.is_running.store(false, Ordering::SeqCst);
    }

    /// 暂停引擎
    pub fn pause(&mut self) {
        info!("Pausing trading engine");
        *self.state.write() = EngineState::Paused;
    }

    /// 恢复引擎
    pub fn resume(&mut self) {
        info!("Resuming trading engine");
        *self.state.write() = EngineState::Running;
    }

    /// 主循环：处理 Tick
    ///
    /// # 核心流程
    /// 1. 从市场数据源获取 Tick（通过接口）
    /// 2. 更新品种状态
    /// 3. 分发到策略执行器（通过接口）
    /// 4. 获取信号
    /// 5. 执行风控检查（通过接口）
    /// 6. 执行订单（通过接口）
    pub async fn run_loop(&mut self) -> Result<(), EngineError> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(EngineError::NotRunning);
        }

        // 通过接口获取 Tick，不直接访问实现
        while let Some(tick) = self.market_data.next_tick().await {
            self.process_tick(&tick).await?;
        }

        Ok(())
    }

    /// 处理单个 Tick
    async fn process_tick(&mut self, tick: &MarketTick) -> Result<(), EngineError> {
        let now_ts = tick.timestamp.timestamp();
        
        // 1. 更新品种状态
        self.update_symbol_state(&tick.symbol, tick.price, now_ts);
        
        // 2. 检查是否可以交易
        if !self.can_trade(&tick.symbol, now_ts) {
            return Ok(());
        }
        
        // 3. 构建 K 线数据（从 Tick 转换）
        let kline = self.build_kline_from_tick(tick);
        
        // 4. 分发到策略执行器（通过接口）
        let signals = self.strategy_executor.dispatch(&kline);
        
        // 5. 处理信号
        for signal in signals {
            self.process_signal(&signal).await?;
        }
        
        Ok(())
    }

    /// 处理交易信号
    async fn process_signal(&mut self, signal: &StrategySignal) -> Result<(), EngineError> {
        let now_ts = Utc::now().timestamp();
        
        // 检查信号有效性
        if !self.validate_signal(signal) {
            return Ok(());
        }
        
        // 构建订单请求
        let order = self.build_order_from_signal(signal);
        
        // 获取账户信息（通过接口）
        let account = self.gateway.get_account()
            .map_err(|e| EngineError::GatewayError(e.to_string()))?;
        
        // 风控预检（通过接口）
        let risk_result = self.risk_checker.pre_check(&order, &account);
        
        if !risk_result.allowed {
            warn!("Risk check rejected: {:?}", risk_result.reason);
            return Ok(());
        }
        
        // 执行订单（通过接口）
        let result = self.gateway.place_order(order)
            .map_err(|e| EngineError::ExecutionError(e.to_string()))?;
        
        if result.status == crate::interfaces::execution::OrderStatus::Filled {
            info!("Order filled: {} {} @ {}", 
                result.executed_quantity, signal.symbol, result.executed_price);
            
            // 设置交易锁
            self.set_trade_lock(&signal.symbol, now_ts);
        }
        
        Ok(())
    }

    /// 更新品种状态
    fn update_symbol_state(&self, symbol: &str, price: Decimal, now_ts: i64) {
        let mut states = self.symbol_states.write();
        if let Some(state) = states.get_mut(symbol) {
            state.current_price = price;
            state.last_tick_time = Some(Utc::now());
        }
    }

    /// 检查是否可以交易
    fn can_trade(&self, symbol: &str, now_ts: i64) -> bool {
        if *self.state.read() != EngineState::Running {
            return false;
        }
        
        let states = self.symbol_states.read();
        states.get(symbol)
            .map(|s| s.can_trade(now_ts))
            .unwrap_or(false)
    }

    /// 设置交易锁
    fn set_trade_lock(&self, symbol: &str, now_ts: i64) {
        let mut states = self.symbol_states.write();
        if let Some(state) = states.get_mut(symbol) {
            // 默认锁定 60 秒
            state.set_trade_lock(now_ts + 60);
        }
    }

    /// 从 Tick 构建 K 线
    fn build_kline_from_tick(&self, tick: &MarketTick) -> MarketKLine {
        MarketKLine {
            symbol: tick.symbol.clone(),
            period: "1m".to_string(),
            open: tick.price,
            high: tick.price,
            low: tick.price,
            close: tick.price,
            volume: tick.qty,
            timestamp: tick.timestamp,
            is_closed: false,
        }
    }

    /// 验证信号
    fn validate_signal(&self, signal: &StrategySignal) -> bool {
        // 信号必须有效
        if signal.quantity <= Decimal::ZERO {
            return false;
        }
        
        // Flat 信号不需要处理
        if signal.direction == SignalDirection::Flat {
            return false;
        }
        
        true
    }

    /// 从信号构建订单
    fn build_order_from_signal(&self, signal: &StrategySignal) -> RiskOrderRequest {
        RiskOrderRequest {
            symbol: signal.symbol.clone(),
            side: match signal.direction {
                SignalDirection::Long => RiskOrderSide::Buy,
                SignalDirection::Short => RiskOrderSide::Sell,
                SignalDirection::Flat => return RiskOrderRequest {
                    symbol: signal.symbol.clone(),
                    side: RiskOrderSide::Sell, // 平仓用 Sell
                    order_type: crate::interfaces::risk::OrderType::Market,
                    quantity: signal.quantity,
                    price: signal.price,
                    stop_loss: signal.stop_loss,
                    take_profit: signal.take_profit,
                },
            },
            order_type: crate::interfaces::risk::OrderType::Market,
            quantity: signal.quantity,
            price: signal.price,
            stop_loss: signal.stop_loss,
            take_profit: signal.take_profit,
        }
    }

    /// 获取引擎状态
    pub fn get_state(&self) -> EngineState {
        *self.state.read()
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// 获取品种状态
    pub fn get_symbol_state(&self, symbol: &str) -> Option<SymbolState> {
        self.symbol_states.read().get(symbol).cloned()
    }
}

/// 引擎错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("Engine not running")]
    NotRunning,

    #[error("Engine already running")]
    AlreadyRunning,

    #[error("Gateway error: {0}")]
    GatewayError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Risk check failed: {0}")]
    RiskCheckFailed(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),
}
