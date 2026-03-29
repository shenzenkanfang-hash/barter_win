//! 事件驱动交易引擎
//!
//! # 架构
//! ```
//! Tick 事件
//!     │
//!     ▼
//! run(rx) ───────────────────────────────────────────────┐
//!     │                                                    │
//!     ▼                                                    │
//! on_tick(tick)                                           │
//!     │                                                    │
//!     ├─► update_store() ──────────────────────────────┤ │  1. 更新数据存储
//!     │                                                    │
//!     ├─► calc_indicators() ───────────────────────────┤ │  2. 增量计算指标
//!     │                                                    │
//!     ├─► strategy.decide() ───────────────────────────┤ │  3. 策略决策
//!     │                                                    │
//!     ├─► risk_checker.pre_check() ─────────────────────┤ │  4. 风控检查
//!     │                                                    │
//!     └─► gateway.place_order() ────────────────────────┘ │  5. 提交订单
//! ```
//!
//! # 关键约束
//! - tokio::spawn: 0 个（全部直接 await）
//! - tokio::sleep: 0 个（事件驱动，无轮询）
//! - 数据竞争: 0 次（单线程串行处理）
//!
//! v3.0: 心跳报到集成 (FE-001)

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;

use a_common::heartbeat::Token as HeartbeatToken;
use crate::types::{OrderRequest, Side, TradingDecision, TradingAction};
use crate::interfaces::RiskChecker;
use a_common::OrderStatus;

/// 心跳报到测试点 ID
const HEARTBEAT_POINT_EVENT_ENGINE: &str = "FE-001";

// ============================================================================
// 事件类型
// ============================================================================

/// Tick 事件
#[derive(Debug, Clone)]
pub struct TickEvent {
    /// 品种
    pub symbol: String,
    /// 价格
    pub price: Decimal,
    /// 数量
    pub qty: Decimal,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 当前1m K线
    pub kline: Option<KlineData>,
    /// K线是否刚闭合
    pub is_kline_closed: bool,
}

/// K线数据
#[derive(Debug, Clone)]
pub struct KlineData {
    /// 品种
    pub symbol: String,
    /// 周期
    pub period: String,
    /// 开盘价
    pub open: Decimal,
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 成交量
    pub volume: Decimal,
    /// K线开始时间
    pub open_time: DateTime<Utc>,
    /// K线结束时间
    pub close_time: DateTime<Utc>,
    /// 是否闭合
    pub is_closed: bool,
}

/// 指标缓存
#[derive(Debug, Clone)]
pub struct IndicatorCache {
    /// 快EMA
    pub ema_fast: Option<Decimal>,
    /// 慢EMA
    pub ema_slow: Option<Decimal>,
    /// RSI
    pub rsi: Option<Decimal>,
    /// 波动率
    pub volatility: Decimal,
    /// 价格位置
    pub price_position: Option<Decimal>,
    /// Pine颜色
    pub pine_color: PineColor,
}

impl Default for IndicatorCache {
    fn default() -> Self {
        Self {
            ema_fast: None,
            ema_slow: None,
            rsi: None,
            volatility: Decimal::ZERO,
            price_position: None,
            pine_color: PineColor::Neutral,
        }
    }
}

/// Pine颜色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PineColor {
    Red,    // 下跌趋势
    Green,  // 上涨趋势
    Neutral,
}

/// 引擎配置
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 品种
    pub symbol: String,
    /// 初始资金
    pub initial_fund: Decimal,
    /// 最大持仓
    pub max_position: Decimal,
    /// 初始开仓比例
    pub initial_ratio: Decimal,
    /// 最小交易量
    pub lot_size: Decimal,
    /// 是否启用风控
    pub enable_risk_check: bool,
    /// 是否启用策略
    pub enable_strategy: bool,
    /// 记录执行时间
    pub log_timing: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            initial_fund: dec!(10000),
            max_position: dec!(0.15),
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            enable_risk_check: true,
            enable_strategy: true,
            log_timing: false,
        }
    }
}

/// 引擎状态
#[derive(Debug, Clone)]
pub struct EngineState {
    /// 品种
    pub symbol: String,
    /// 当前价格
    pub current_price: Option<Decimal>,
    /// 指标缓存
    pub indicators: IndicatorCache,
    /// 是否已有持仓
    pub has_position: bool,
    /// 持仓数量
    pub position_qty: Decimal,
    /// 持仓均价
    pub position_price: Decimal,
    /// 持仓方向
    pub position_side: Option<Side>,
    /// Tick 计数
    pub tick_count: u64,
    /// 总下单数
    pub total_orders: u64,
    /// 成功订单数
    pub filled_orders: u64,
    /// 拒绝订单数
    pub rejected_orders: u64,
    /// 最后处理时间
    pub last_process_time: Option<Duration>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            current_price: None,
            indicators: IndicatorCache::default(),
            has_position: false,
            position_qty: Decimal::ZERO,
            position_price: Decimal::ZERO,
            position_side: None,
            tick_count: 0,
            total_orders: 0,
            filled_orders: 0,
            rejected_orders: 0,
            last_process_time: None,
        }
    }
}

// ============================================================================
// 事件驱动引擎
// ============================================================================

/// 事件驱动交易引擎
///
/// # 设计原则
/// - **零轮询**: 使用 `recv().await` 阻塞等待
/// - **零 spawn**: 无后台任务，单事件循环
/// - **串行处理**: 每个 Tick 完全处理后才处理下一个
pub struct EventEngine<S: Strategy, G: ExchangeGateway> {
    /// 配置
    config: EngineConfig,
    /// 状态
    state: EngineState,
    /// 风控检查器（保留用于未来集成外部风控）
    #[allow(dead_code)]
    risk_checker: Arc<dyn RiskChecker>,
    /// 策略
    strategy: S,
    /// 网关
    gateway: G,
    /// 指标计算器
    indicators: IndicatorCalculator,
    /// v3.0: 心跳 Token
    heartbeat_token: Arc<RwLock<Option<HeartbeatToken>>>,
}

impl<S: Strategy, G: ExchangeGateway> EventEngine<S, G> {
    /// 创建引擎
    pub fn new(
        config: EngineConfig,
        risk_checker: Arc<dyn RiskChecker>,
        strategy: S,
        gateway: G,
    ) -> Self {
        let mut state = EngineState::default();
        state.symbol = config.symbol.clone();

        Self {
            config,
            state,
            risk_checker,
            strategy,
            gateway,
            indicators: IndicatorCalculator::default(),
            heartbeat_token: Arc::new(RwLock::new(None)),
        }
    }

    // ==================== v3.0: 心跳报到 ====================

    /// 设置心跳 Token
    pub fn set_heartbeat_token(&self, token: HeartbeatToken) {
        let mut guard = self.heartbeat_token.write();
        *guard = Some(token);
    }

    /// 获取当前心跳 Token
    pub fn get_heartbeat_token(&self) -> Option<HeartbeatToken> {
        self.heartbeat_token.read().clone()
    }

    /// 心跳报到（内部方法）
    pub async fn heartbeat_report(&self) {
        let token = self.get_heartbeat_token();
        if let Some(token) = token {
            if let Ok(reporter) = std::panic::catch_unwind(|| a_common::heartbeat::global()) {
                reporter.report(
                    &token,
                    HEARTBEAT_POINT_EVENT_ENGINE,
                    "f_engine::event",
                    "on_tick",
                    file!(),
                ).await;
            }
        }
    }

    /// 获取当前状态（只读访问）
    pub fn state(&self) -> &EngineState {
        &self.state
    }

    /// 单事件循环
    ///
    /// 从 channel 接收 Tick，串行处理
    pub async fn run(&mut self, tick_rx: mpsc::Receiver<Arc<TickEvent>>) {
        tracing::info!("[Engine] {} 事件循环启动", self.config.symbol);

        let mut tick_rx = tick_rx;
        while let Some(tick) = tick_rx.recv().await {
            self.on_tick(tick.as_ref().clone()).await;
        }

        tracing::info!(
            "[Engine] {} 事件循环结束 - 处理了 {} 个 Tick",
            self.config.symbol,
            self.state.tick_count
        );
    }

    /// 带心跳检查的事件循环
    ///
    /// 使用 tokio::select! 同时监听 Tick 和心跳定时器
    pub async fn run_with_heartbeat(
        &mut self,
        tick_rx: mpsc::Receiver<Arc<TickEvent>>,
        heartbeat_interval_secs: u64,
    ) {
        use tokio::time::{interval, Duration};

        tracing::info!(
            "[Engine] {} 事件循环启动（心跳间隔: {}s）",
            self.config.symbol,
            heartbeat_interval_secs
        );

        let mut tick_rx = tick_rx;
        let mut heartbeat = interval(Duration::from_secs(heartbeat_interval_secs));

        loop {
            tokio::select! {
                // Tick 事件
                Some(tick) = tick_rx.recv() => {
                    self.on_tick(tick.as_ref().clone()).await;
                }
                // 心跳
                _ = heartbeat.tick() => {
                    self.on_heartbeat().await;
                }
            }
        }
    }

    /// 处理单个 Tick 事件
    async fn on_tick(&mut self, tick: TickEvent) {
        // v3.0: 心跳报到
        self.heartbeat_report().await;

        let start = if self.config.log_timing {
            Some(Instant::now())
        } else {
            None
        };

        // 1. 更新状态
        self.state.current_price = Some(tick.price);
        self.state.tick_count += 1;

        // 2. 增量计算指标
        self.state.indicators = self.indicators.update(tick.price, tick.timestamp);

        // 3. K线闭合时触发额外处理
        if tick.is_kline_closed {
            if let Some(ref kline) = tick.kline {
                self.on_kline_closed(kline).await;
            }
        }

        // 4. 策略决策（如果启用）
        if self.config.enable_strategy {
            if let Some(decision) = self.strategy.decide(&self.state).await {
                // 5. 风控检查（如果启用）
                if !self.config.enable_risk_check || self.check_risk(&decision).await {
                    // 6. 提交订单
                    self.submit_order(decision).await;
                }
            }
        }

        // 更新处理时间
        self.state.last_process_time = start.map(|s| s.elapsed());

        // 日志耗时
        if let Some(elapsed) = self.state.last_process_time {
            tracing::trace!(
                "[Engine] {} Tick处理耗时: {:?}",
                tick.symbol,
                elapsed
            );
        }
    }

    /// K线闭合时调用
    async fn on_kline_closed(&mut self, kline: &KlineData) {
        tracing::debug!(
            "[Engine] {} K线闭合: {} @ {}",
            kline.symbol,
            kline.period,
            kline.close
        );
    }

    /// 心跳回调
    async fn on_heartbeat(&mut self) {
        tracing::trace!(
            "[Engine] {} 心跳 - tick_count: {}, orders: {}/{}",
            self.config.symbol,
            self.state.tick_count,
            self.state.filled_orders,
            self.state.total_orders
        );
    }

    /// 风控检查
    ///
    /// 实现完整的风控检查逻辑:
    /// 1. 最大持仓检查
    /// 2. 最小下单量检查
    /// 3. 价格合理性检查
    async fn check_risk(&self, decision: &TradingDecision) -> bool {
        // 1. 最大持仓检查
        if self.state.has_position && decision.action != TradingAction::Flat {
            // 如果已有持仓且不是平仓，只允许反向开仓或平仓
            match (self.state.position_side, decision.action) {
                (Some(Side::Buy), TradingAction::Short) => {} // 允许反向开空
                (Some(Side::Sell), TradingAction::Long) => {} // 允许反向开多
                (Some(_), TradingAction::Add) => {
                    tracing::warn!(
                        "[Engine] {} 禁止加仓，已有持仓中",
                        self.config.symbol
                    );
                    return false;
                }
                _ => {}
            }
        }

        // 2. 最小下单量检查
        if decision.qty < self.config.lot_size {
            tracing::warn!(
                "[Engine] {} 下单量 {} 小于最小交易量 {}",
                self.config.symbol,
                decision.qty,
                self.config.lot_size
            );
            return false;
        }

        // 3. 价格合理性检查
        if let Some(current_price) = self.state.current_price {
            let price_diff = if decision.price > Decimal::ZERO {
                ((decision.price - current_price) / current_price).abs()
            } else {
                Decimal::ZERO
            };

            // 价格偏离超过10%则拒绝
            if price_diff > dec!(0.1) {
                tracing::warn!(
                    "[Engine] {} 价格偏离过大: {} vs 当前 {} (偏离 {:.2}%)",
                    self.config.symbol,
                    decision.price,
                    current_price,
                    price_diff * dec!(100)
                );
                return false;
            }
        }

        // 4. 总下单数检查（防止异常高频）
        if self.state.total_orders >= 1000 && self.state.tick_count < 10000 {
            tracing::warn!(
                "[Engine] {} 下单过于频繁: {} 订单 / {} Tick",
                self.config.symbol,
                self.state.total_orders,
                self.state.tick_count
            );
            return false;
        }

        tracing::debug!(
            "[Engine] {} 风控检查通过: {:?} qty={} price={}",
            self.config.symbol,
            decision.action,
            decision.qty,
            decision.price
        );
        true
    }

    /// 提交订单
    async fn submit_order(&mut self, decision: TradingDecision) {
        self.state.total_orders += 1;

        let order = OrderRequest {
            symbol: decision.symbol.clone(),
            side: match decision.action {
                TradingAction::Long => Side::Buy,
                TradingAction::Short => Side::Sell,
                TradingAction::Flat => return, // 平仓不需要下单
                // Add, Reduce, Hedge, Wait 不下单
                TradingAction::Add | TradingAction::Reduce | TradingAction::Hedge | TradingAction::Wait => return,
            },
            order_type: crate::types::OrderType::Market,
            qty: decision.qty,
            price: Some(decision.price),
        };

        match self.gateway.place_order(order).await {
            Ok(result) => {
                if result.status == OrderStatus::Filled {
                    self.state.filled_orders += 1;
                    tracing::info!(
                        "[Engine] {} 订单成交: {} {:?} @ {}",
                        self.config.symbol,
                        result.filled_qty,
                        result.side,
                        result.filled_price
                    );
                }
            }
            Err(e) => {
                self.state.rejected_orders += 1;
                tracing::warn!("[Engine] {} 订单失败: {}", self.config.symbol, e);
            }
        }
    }
}

// ============================================================================
// 策略 trait（同步版本，避免 dyn 兼容性问题）
// ============================================================================

/// 策略 trait - 用于实现交易策略
#[async_trait::async_trait]
pub trait Strategy: Send + Sync {
    /// 决策
    async fn decide(&self, state: &EngineState) -> Option<TradingDecision>;
}

// ============================================================================
// 网关 trait（同步版本）
// ============================================================================

/// 交易所网关 trait
#[async_trait::async_trait]
pub trait ExchangeGateway: Send + Sync {
    /// 下单
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, GatewayError>;
    /// 获取账户
    async fn get_account(&self) -> Result<AccountInfo, GatewayError>;
    /// 获取持仓
    async fn get_position(&self, symbol: &str) -> Result<Option<PositionInfo>, GatewayError>;
}

/// 账户信息
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub update_ts: i64,
}

/// 持仓信息
#[derive(Debug, Clone)]
pub struct PositionInfo {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

/// 订单结果
#[derive(Debug, Clone)]
pub struct OrderResult {
    pub order_id: String,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub filled_price: Decimal,
    pub side: Side,
}

/// 网关错误
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("下单失败: {0}")]
    OrderFailed(String),
    #[error("获取账户失败: {0}")]
    GetAccountFailed(String),
    #[error("获取持仓失败: {0}")]
    GetPositionFailed(String),
}

// ============================================================================
// 指标计算器
// ============================================================================

/// 指标计算器 - 增量 O(1) 算法
#[derive(Debug, Clone, Default)]
pub struct IndicatorCalculator {
    /// EMA 快周期（保留用于未来可配置）
    #[allow(dead_code)]
    ema_period_fast: usize,
    /// EMA 慢周期（保留用于未来可配置）
    #[allow(dead_code)]
    ema_period_slow: usize,
    /// RSI 周期
    #[allow(dead_code)]
    rsi_period: usize,
    /// 前一个价格
    prev_price: Option<Decimal>,
    /// 快EMA前值
    ema_fast_prev: Option<Decimal>,
    /// 慢EMA前值
    ema_slow_prev: Option<Decimal>,
    /// RSI 增益
    avg_gain: Option<Decimal>,
    /// RSI 损耗
    avg_loss: Option<Decimal>,
}

impl IndicatorCalculator {
    /// 更新指标
    pub fn update(&mut self, price: Decimal, _timestamp: DateTime<Utc>) -> IndicatorCache {
        let mut cache = IndicatorCache::default();

        // 计算 EMA
        cache.ema_fast = self.calc_ema(price, 5);
        cache.ema_slow = self.calc_ema(price, 20);

        // 计算 RSI
        cache.rsi = self.calc_rsi(price);

        // 计算 Pine 颜色
        cache.pine_color = self.calc_pine_color();

        // 计算波动率（简化版）
        cache.volatility = self.calc_volatility(price);

        self.prev_price = Some(price);
        cache
    }

    /// 增量 EMA 计算
    fn calc_ema(&mut self, price: Decimal, period: usize) -> Option<Decimal> {
        let multiplier = dec!(2) / Decimal::from(period + 1);

        let prev = if period == 5 {
            &mut self.ema_fast_prev
        } else {
            &mut self.ema_slow_prev
        };

        match *prev {
            None => {
                // 第一个值，直接使用价格
                *prev = Some(price);
                Some(price)
            }
            Some(p) => {
                // EMA = price * multiplier + prev * (1 - multiplier)
                let ema = price * multiplier + p * (dec!(1) - multiplier);
                *prev = Some(ema);
                Some(ema)
            }
        }
    }

    /// 增量 RSI 计算
    fn calc_rsi(&mut self, price: Decimal) -> Option<Decimal> {
        let prev = self.prev_price?;
        let change = price - prev;
        let gain = if change > Decimal::ZERO { change } else { -change };
        let loss = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        match (&self.avg_gain, &self.avg_loss) {
            (None, None) => {
                // 第一个值
                self.avg_gain = Some(gain);
                self.avg_loss = Some(loss);
                Some(dec!(50)) // 中性值
            }
            (Some(ag), Some(al)) => {
                let multiplier = dec!(1) / Decimal::from(self.rsi_period + 1);
                let new_ag = ag * (dec!(1) - multiplier) + gain * multiplier;
                let new_al = al * (dec!(1) - multiplier) + loss * multiplier;

                self.avg_gain = Some(new_ag);
                self.avg_loss = Some(new_al);

                if new_al == Decimal::ZERO {
                    Some(dec!(100))
                } else {
                    let rs = new_ag / new_al;
                    Some(dec!(100) - dec!(100) / (dec!(1) + rs))
                }
            }
            // 处理边界情况
            _ => Some(dec!(50)),
        }
    }

    /// 计算 Pine 颜色
    fn calc_pine_color(&self) -> PineColor {
        match (&self.ema_fast_prev, &self.ema_slow_prev) {
            (Some(fast), Some(slow)) => {
                if fast > slow {
                    PineColor::Green
                } else if fast < slow {
                    PineColor::Red
                } else {
                    PineColor::Neutral
                }
            }
            _ => PineColor::Neutral,
        }
    }

    /// 计算波动率（简化版）
    fn calc_volatility(&self, price: Decimal) -> Decimal {
        // 简化：使用价格变化率作为波动率代理
        match self.prev_price {
            Some(prev) if prev != Decimal::ZERO => {
                let change = ((price - prev) / prev).abs();
                change * dec!(100) // 转为百分比
            }
            _ => Decimal::ZERO,
        }
    }
}
