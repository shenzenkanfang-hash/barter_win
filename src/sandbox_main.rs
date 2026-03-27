//! Sandbox Mode - 事件驱动完全重构版
//!
//! ## 核心设计
//! - 单事件循环：所有 Tick 串行处理，无竞态
//! - 零轮询：recv().await 阻塞等待，无 sleep
//! - 沙盒纯注入：只负责 push_tick，不计算指标
//! - 引擎内聚：引擎内部包含完整处理链
//!
//! ## 数据流
//! ```
//! StreamTickGenerator
//!         ↓ tick_tx.send()
//!     mpsc::channel
//!         ↓ tick_rx.recv().await
//! TradingEngine.run()  ← 唯一事件循环
//!         ↓
//!     1. on_tick(tick) ← 串行处理，无 spawn
//!     2. update_store() ← 内存更新
//!     3. calc_indicators() ← 增量计算
//!     4. decide_strategy() ← 策略决策
//!     5. check_risk() ← 风控
//!     6. submit_order() ← 异步下单
//! ```
//!
//! ## 关键约束
//! - tokio::spawn: 0 个（全部直接 await）
//! - tokio::sleep: 0 个（事件驱动，无轮询）
//! - ws_get_1m(): 0 次（引擎内部直接访问）
//! - Arc<RwLock>: 最小化（DashMap 替代）

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};
use dashmap::DashMap;

use a_common::volatility::{VolatilityCalc, KLineInput};
use b_data_source::{KLine, Period, Tick};
use h_sandbox::{
    ShadowBinanceGateway, ShadowRiskChecker,
    historical_replay::{StreamTickGenerator, TickToWsConverter},
};

mod multi_engine;
use multi_engine::{TickRouter, ArcTick};
use f_engine::types::{OrderRequest, OrderType, Side};
use f_engine::interfaces::RiskChecker;

// ============================================================================
// 常量配置
// ============================================================================

const DEFAULT_SYMBOL: &str = "HOTUSDT";
const DEFAULT_FUND: f64 = 10000.0;
const VOLATILITY_THRESHOLD: Decimal = dec!(0.02);  // 2% 波动率阈值

// ============================================================================
// 背压模式
// ============================================================================

/// 背压模式 - 决定 channel 满时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureMode {
    /// 回放模式：阻塞生产者，保时间线一致性
    Replay,
    /// 实盘模式：非阻塞，channel 满了弹最旧的、留最新的
    Realtime,
}

impl Default for BackpressureMode {
    fn default() -> Self {
        BackpressureMode::Replay  // 默认回放模式
    }
}

// ============================================================================
// SandboxConfig
// ============================================================================

#[derive(Debug, Clone)]
struct SandboxConfig {
    symbols: Vec<String>,  // 支持多品种
    initial_fund: Decimal,
    duration_secs: u64,
    start_date: String,
    end_date: String,
    /// 背压模式
    backpressure_mode: BackpressureMode,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            symbols: vec![DEFAULT_SYMBOL.to_string()],  // 默认单品种
            initial_fund: dec!(10000),
            duration_secs: 60,
            start_date: "2025-10-10".to_string(),
            end_date: "2025-10-12".to_string(),
            backpressure_mode: BackpressureMode::Replay,
        }
    }
}

// ============================================================================
// 指标计算（引擎内部，无锁）
// ============================================================================

/// 最大价格历史窗口
/// 
/// 计算所需的最大窗口：
/// - EMA20 需要 20 个数据点
/// - RSI14 需要 14 个数据点
/// - 留 20% 冗余
const MAX_PRICE_HISTORY: usize = 50;  // 50 * Decimal ≈ 几百字节

#[derive(Debug, Clone)]
struct Indicators {
    rsi: Option<Decimal>,
    ema5: Option<Decimal>,
    ema20: Option<Decimal>,
    volatility: Decimal,
    price_history: Vec<Decimal>,
    volatility_calc: VolatilityCalc,
    vol_15m: Decimal,
}

impl Default for Indicators {
    fn default() -> Self {
        Self {
            rsi: None,
            ema5: None,
            ema20: None,
            volatility: Decimal::ZERO,
            price_history: Vec::with_capacity(MAX_PRICE_HISTORY),
            volatility_calc: VolatilityCalc::new(),
            vol_15m: Decimal::ZERO,
        }
    }
}

impl Indicators {
    /// 增量更新指标 - O(1)
    fn update(&mut self, tick: &Tick) {
        let price = tick.price;

        // 1. 添加到价格历史（严格限制窗口大小，防止内存泄漏）
        self.price_history.push(price);
        if self.price_history.len() > MAX_PRICE_HISTORY {
            self.price_history.remove(0);
        }

        // 2. 计算波动率（O(20)）
        if self.price_history.len() >= 20 {
            let n = self.price_history.len();
            let recent = &self.price_history[n - 20..];
            let sum: Decimal = recent.iter().sum();
            let mean = sum / Decimal::from(20);
            let variance: Decimal = recent.iter()
                .map(|p| (*p - mean) * (*p - mean))
                .sum::<Decimal>() / Decimal::from(20);
            self.volatility = variance.sqrt().unwrap_or(Decimal::ZERO);
        }

        // 3. 计算RSI（14周期）
        if self.price_history.len() >= 14 {
            let mut gains = Decimal::ZERO;
            let mut losses = Decimal::ZERO;
            for window in self.price_history.windows(2) {
                let change = window[1] - window[0];
                if change > Decimal::ZERO {
                    gains += change;
                } else {
                    losses -= change;
                }
            }
            let avg_gain = gains / dec!(14);
            let avg_loss = losses / dec!(14);
            if avg_loss.is_zero() {
                self.rsi = Some(dec!(100));
            } else {
                let rs = avg_gain / avg_loss;
                self.rsi = Some(dec!(100) - dec!(100) / (dec!(1) + rs));
            }
        }

        // 4. 计算EMA
        if self.price_history.len() >= 5 {
            self.ema5 = Some(Self::calc_ema(&self.price_history, 5));
        }
        if self.price_history.len() >= 20 {
            self.ema20 = Some(Self::calc_ema(&self.price_history, 20));
        }

        // 5. 15m波动率（仅K线闭合时）
        if let Some(ref kline) = tick.kline_1m {
            if kline.is_closed {
                let input = KLineInput {
                    open: kline.open,
                    high: kline.high,
                    low: kline.low,
                    close: kline.close,
                    timestamp: chrono::Utc::now(),
                };
                let stats = self.volatility_calc.update(input);
                self.vol_15m = stats.vol_15m;
            }
        }
    }

    fn calc_ema(prices: &[Decimal], period: usize) -> Decimal {
        if prices.is_empty() {
            return Decimal::ZERO;
        }
        let k = dec!(2) / Decimal::from(period + 1);
        let mut ema = prices[0];
        for price in prices.iter().skip(1) {
            ema = *price * k + ema * (Decimal::ONE - k);
        }
        ema
    }
}

// ============================================================================
// 检查点数据（用于崩溃恢复）
// ============================================================================

/// 检查点数据 - 可序列化用于持久化
#[derive(Debug, Clone)]
struct Checkpoint {
    /// 最后处理的序列号
    last_sequence_id: u64,
    /// 检查点时间戳（毫秒）
    timestamp_ms: i64,
}

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            last_sequence_id: 0,
            timestamp_ms: Utc::now().timestamp_millis(),
        }
    }
}

/// 检查点管理器接口（为将来 WAL 集成预留）
trait CheckpointManager {
    fn save_checkpoint(&self, checkpoint: &Checkpoint);
    fn load_checkpoint(&self) -> Option<Checkpoint>;
}

/// 内存检查点管理器（沙盒用，不持久化）
struct MemoryCheckpointManager {
    latest: std::sync::Mutex<Checkpoint>,
}

impl MemoryCheckpointManager {
    fn new() -> Self {
        Self {
            latest: std::sync::Mutex::new(Checkpoint::default()),
        }
    }
}

impl CheckpointManager for MemoryCheckpointManager {
    fn save_checkpoint(&self, checkpoint: &Checkpoint) {
        let mut latest = self.latest.lock().unwrap();
        *latest = checkpoint.clone();
        tracing::debug!(
            seq = %checkpoint.last_sequence_id,
            ts = %checkpoint.timestamp_ms,
            "[Checkpoint] Saved"
        );
    }

    fn load_checkpoint(&self) -> Option<Checkpoint> {
        let latest = self.latest.lock().unwrap();
        if latest.last_sequence_id > 0 {
            tracing::info!(
                seq = %latest.last_sequence_id,
                "[Checkpoint] Restored from memory"
            );
            Some(latest.clone())
        } else {
            None
        }
    }
}

// ============================================================================
// 引擎状态（引擎内部，无锁共享）
// ============================================================================

struct EngineState {
    indicators: DashMap<String, Indicators>,  // 无锁
    position: DashMap<String, PositionState>, // 无锁
    stats: EngineStats,
    /// 上次策略执行时间戳（毫秒）- AtomicI64 保证多线程安全
    last_strategy_run_ts: AtomicI64,
    /// 策略执行间隔（毫秒）
    strategy_interval_ms: i64,
}

#[derive(Debug, Clone)]
struct PositionState {
    has_position: bool,
    entry_price: Decimal,
    side: Option<Side>,
}

impl Default for PositionState {
    fn default() -> Self {
        Self {
            has_position: false,
            entry_price: Decimal::ZERO,
            side: None,
        }
    }
}

/// 慢 Tick 告警阈值（毫秒）
const SLOW_TICK_THRESHOLD_MS: u64 = 10;
/// 策略执行默认间隔（毫秒）
const DEFAULT_STRATEGY_INTERVAL_MS: i64 = 100;
/// 检查点保存间隔（每 N 个 Tick 保存一次）
const CHECKPOINT_INTERVAL: u64 = 1000;

#[derive(Debug, Default)]
struct EngineStats {
    total_orders: u32,
    total_trades: u32,
    total_errors: u32,
    /// 总耗时累计
    total_duration_ms: u64,
    /// 慢 Tick 计数
    slow_tick_count: u32,
    /// 最大单次处理耗时（毫秒）
    max_tick_duration_ms: u64,
}

impl EngineState {
    fn new() -> Self {
        Self {
            indicators: DashMap::new(),
            position: DashMap::new(),
            stats: EngineStats::default(),
            last_strategy_run_ts: AtomicI64::new(0),  // 初始化为 0
            strategy_interval_ms: DEFAULT_STRATEGY_INTERVAL_MS,
        }
    }

    fn get_indicators(&self, symbol: &str) -> Option<Indicators> {
        self.indicators.get(symbol).map(|r| r.clone())
    }

    fn update_indicators(&self, tick: &Tick) {
        let symbol = tick.symbol.clone();
        let mut ind = self.indicators.entry(symbol)
            .or_insert_with(Indicators::default);
        ind.update(tick);
    }

    fn get_volatility(&self, symbol: &str) -> Decimal {
        self.indicators.get(symbol)
            .map(|r| r.vol_15m)
            .unwrap_or(Decimal::ZERO)
    }

    fn get_position(&self, symbol: &str) -> PositionState {
        self.position.get(symbol).map(|r| r.clone()).unwrap_or_default()
    }

    fn set_position(&self, symbol: &str, state: PositionState) {
        self.position.insert(symbol.to_string(), state);
    }
}

// ============================================================================
// TradingEngine - 单事件循环引擎
// ============================================================================

struct TradingEngine {
    symbol: String,
    state: EngineState,
    gateway: Arc<ShadowBinanceGateway>,
    risk_checker: Arc<ShadowRiskChecker>,
    /// 最后处理的序列号（用于幂等性去重）
    last_processed_seq: u64,
    /// 检查点管理器（用于崩溃恢复）
    checkpoint_manager: Arc<MemoryCheckpointManager>,
}

impl TradingEngine {
    fn new(
        symbol: String,
        gateway: Arc<ShadowBinanceGateway>,
        risk_checker: Arc<ShadowRiskChecker>,
    ) -> Self {
        Self {
            symbol,
            state: EngineState::new(),
            gateway,
            risk_checker,
            last_processed_seq: 0,
            checkpoint_manager: Arc::new(MemoryCheckpointManager::new()),
        }
    }

    /// 单事件循环 - 核心
    /// 
    /// 关键设计：
    /// - 直接 await，不 spawn
    /// - 每个 Tick 处理完才接收下一个
    /// - 无 sleep，无轮询
    /// - 幂等性：跳过重复的 Tick
    async fn run(&mut self, mut tick_rx: mpsc::Receiver<Tick>) {
        tracing::info!("[Engine] {} 事件循环启动", self.symbol);

        while let Some(tick) = tick_rx.recv().await {
            // 幂等性检查：跳过重复 Tick
            if tick.sequence_id <= self.last_processed_seq {
                tracing::debug!(
                    symbol = %tick.symbol,
                    seq = %tick.sequence_id,
                    last_seq = %self.last_processed_seq,
                    "[Engine] 跳过重复 Tick"
                );
                continue;
            }
            self.last_processed_seq = tick.sequence_id;
            
            // 处理单个 Tick - 串行，无并发
            self.on_tick(tick).await;
            
            // === 定期保存检查点 ===
            if self.last_processed_seq % CHECKPOINT_INTERVAL == 0 {
                let checkpoint = Checkpoint {
                    last_sequence_id: self.last_processed_seq,
                    timestamp_ms: Utc::now().timestamp_millis(),
                };
                self.checkpoint_manager.save_checkpoint(&checkpoint);
            }
        }

        tracing::info!("[Engine] {} 事件循环结束", self.symbol);
    }

    /// 处理单个 Tick - 完整处理链
    async fn on_tick(&mut self, tick: Tick) {
        let total_start = Instant::now();
        let symbol = tick.symbol.clone();

        // ===== 步骤1: 更新指标（增量计算，O(1)）=====
        let indicator_start = Instant::now();
        self.state.update_indicators(&tick);
        let indicator_ms = indicator_start.elapsed().as_millis() as u64;

        // ===== 步骤2: 策略决策 =====
        let decide_start = Instant::now();
        if let Some(decision) = self.decide(&tick) {
            let decide_ms = decide_start.elapsed().as_millis() as u64;
            
            // ===== 步骤3: 风控检查 =====
            let risk_start = Instant::now();
            if let Some(order) = self.check_risk(&decision) {
                let risk_ms = risk_start.elapsed().as_millis() as u64;
                
                // ===== 步骤4: 异步下单（不阻塞循环）=====
                let submit_start = Instant::now();
                self.submit_order(order).await;
                let submit_ms = submit_start.elapsed().as_millis() as u64;
                
                tracing::trace!(
                    symbol = %symbol,
                    seq = %tick.sequence_id,
                    indicator_ms = %indicator_ms,
                    decide_ms = %decide_ms,
                    risk_ms = %risk_ms,
                    submit_ms = %submit_ms,
                    "[Engine] Tick 处理完成"
                );
            }
        }

        // ===== 步骤5: 更新持仓状态（如果成交）=====
        self.update_position_from_trade(&tick);
        
        // ===== 统计耗时 =====
        let total_ms = total_start.elapsed().as_millis() as u64;
        self.state.stats.total_duration_ms += total_ms;
        
        // 更新最大耗时
        if total_ms > self.state.stats.max_tick_duration_ms {
            self.state.stats.max_tick_duration_ms = total_ms;
        }
        
        // 慢 Tick 告警
        if total_ms > SLOW_TICK_THRESHOLD_MS {
            self.state.stats.slow_tick_count += 1;
            tracing::warn!(
                symbol = %symbol,
                seq = %tick.sequence_id,
                total_ms = %total_ms,
                indicator_ms = %indicator_ms,
                "[Engine] 慢 Tick 告警"
            );
        }
    }

    /// 策略决策 - EMA 金叉/死叉（带间隔控制）
    fn decide(&self, tick: &Tick) -> Option<TradingDecision> {
        let symbol = &tick.symbol;
        
        // === 策略间隔检查 ===
        let current_ts = tick.timestamp.timestamp_millis();
        let last_ts = self.state.last_strategy_run_ts.load(Ordering::Acquire);
        let interval = self.state.strategy_interval_ms;
        
        if current_ts - last_ts < interval {
            tracing::trace!(
                symbol = %symbol,
                current_ts = %current_ts,
                last_ts = %last_ts,
                interval_ms = %interval,
                "Skip: strategy interval not reached"
            );
            return None;
        }
        
        let indicators = match self.state.get_indicators(symbol) {
            Some(ind) => ind,
            None => {
                tracing::trace!(symbol = %symbol, tick_ts = %tick.timestamp, "Skip tick: no indicators yet");
                return None;
            }
        };
        
        let position = self.state.get_position(symbol);
        let price = tick.price;
        
        let (ema5, ema20, rsi) = match (indicators.ema5, indicators.ema20, indicators.rsi) {
            (Some(e5), Some(e20), Some(r)) => (e5, e20, r),
            (None, _, _) => {
                tracing::trace!(symbol = %symbol, tick_ts = %tick.timestamp, "Skip tick: EMA5 not ready");
                return None;
            }
            (_, None, _) => {
                tracing::trace!(symbol = %symbol, tick_ts = %tick.timestamp, "Skip tick: EMA20 not ready");
                return None;
            }
            (_, _, None) => {
                tracing::trace!(symbol = %symbol, tick_ts = %tick.timestamp, "Skip tick: RSI not ready");
                return None;
            }
        };

        // 无持仓 -> 检查买入条件
        if !position.has_position {
            if ema5 > ema20 && rsi < dec!(70) && rsi > dec!(30) {
                // 更新策略执行时间戳（Release ordering）
                self.state.last_strategy_run_ts.store(current_ts, Ordering::Release);
                return Some(TradingDecision {
                    symbol: symbol.clone(),
                    action: TradingAction::Long,
                    price,
                    qty: dec!(0.01),
                    reason: "EMA金叉".to_string(),
                });
            }
        }
        // 有持仓 -> 检查卖出条件
        else {
            if ema5 < ema20 || rsi > dec!(70) {
                // 更新策略执行时间戳（Release ordering）
                self.state.last_strategy_run_ts.store(current_ts, Ordering::Release);
                return Some(TradingDecision {
                    symbol: symbol.clone(),
                    action: TradingAction::Flat,
                    price,
                    qty: dec!(0.01),
                    reason: "EMA死叉或RSI超买".to_string(),
                });
            }
        }

        None
    }

    /// 风控检查
    fn check_risk(&mut self, decision: &TradingDecision) -> Option<OrderRequest> {
        let account = match self.gateway.get_account() {
            Ok(acc) => acc,
            Err(e) => {
                tracing::warn!(
                    symbol = %decision.symbol,
                    action = ?decision.action,
                    error = %e,
                    "[Risk] 获取账户失败，跳过该决策"
                );
                return None;
            }
        };

        let order = OrderRequest {
            symbol: decision.symbol.clone(),
            side: match decision.action {
                TradingAction::Long => Side::Buy,
                TradingAction::Flat | TradingAction::Short => Side::Sell,
            },
            order_type: OrderType::Market,
            qty: decision.qty,
            price: Some(decision.price),
        };

        let risk_result = self.risk_checker.pre_check(&order, &account);
        if risk_result.pre_failed() {
            tracing::warn!(
                symbol = %order.symbol,
                qty = %order.qty,
                reason = ?risk_result,
                "[Risk] 风控拦截，跳过下单"
            );
            return None;
        }

        Some(order)
    }

    /// 异步下单 - 不阻塞主循环
    async fn submit_order(&mut self, order: OrderRequest) {
        let symbol = order.symbol.clone();
        let side = order.side;
        let qty = order.qty;

        match self.gateway.place_order(order) {
            Ok(result) => {
                self.state.stats.total_orders += 1;
                if side == Side::Buy {
                    self.state.stats.total_trades += 1;
                }
                tracing::info!(
                    symbol = %symbol,
                    side = ?side,
                    qty = %result.filled_qty,
                    price = %result.filled_price,
                    order_id = %result.order_id,
                    "[Order] 下单成功"
                );
            }
            Err(e) => {
                self.state.stats.total_errors += 1;
                tracing::error!(
                    symbol = %symbol,
                    side = ?side,
                    qty = %qty,
                    error = %e,
                    "[Order] 下单失败"
                );
            }
        }
    }

    /// 从成交更新持仓状态
    fn update_position_from_trade(&self, tick: &Tick) {
        // 实际应从 OrderResponse 更新，这里简化处理
    }
}

// ============================================================================
// 交易决策
// ============================================================================

struct TradingDecision {
    symbol: String,
    action: TradingAction,
    price: Decimal,
    qty: Decimal,
    reason: String,
}

#[derive(Debug, Clone, Copy)]
enum TradingAction {
    Long,
    Short,
    Flat,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 发送 Tick（根据背压模式选择策略）
async fn send_tick_with_backpressure(
    tx: &mpsc::Sender<Tick>,
    tick: Tick,
    mode: BackpressureMode,
) -> Result<(), ()> {
    match mode {
        BackpressureMode::Replay => {
            // 回放模式：阻塞保时间线
            if tx.send(tick).await.is_err() {
                tracing::info!("通道关闭，停止注入");
                return Err(());
            }
            Ok(())
        }
        BackpressureMode::Realtime => {
            // 实盘模式：非阻塞
            // 注：tokio mpsc Sender 不支持 try_recv，无法主动弹旧数据
            // 所以满了就丢弃最新的 Tick，保留队列中更早的
            let symbol = tick.symbol.clone();
            let seq = tick.sequence_id;
            match tx.try_send(tick) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => {
                    tracing::warn!(
                        symbol = %symbol,
                        seq = %seq,
                        "[Producer] Channel full, drop newest tick to avoid latency"
                    );
                    Ok(())
                }
                Err(TrySendError::Closed(_)) => {
                    tracing::info!("通道关闭，停止注入");
                    Err(())
                }
            }
        }
    }
}

fn parse_args() -> SandboxConfig {
    let args: Vec<String> = std::env::args().collect();
    // 简化版，实际应解析命令行参数
    SandboxConfig::default()
}

async fn fetch_klines_from_api(
    symbol: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<KLine>> {
    let start_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 00:00:00", start_date), "%Y-%m-%d %H:%M:%S"
    )?;
    let end_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 23:59:59", end_date), "%Y-%m-%d %H:%M:%S"
    )?;

    let start_ms = start_dt.and_utc().timestamp_millis();
    let end_ms = end_dt.and_utc().timestamp_millis();

    let mut all_klines = Vec::new();
    let mut current_start = start_ms;
    let client = reqwest::Client::new();

    loop {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval=1m&limit=1000&startTime={}&endTime={}",
            symbol.to_uppercase(),
            current_start,
            end_ms
        );

        let response = client.get(&url).send().await?.text().await?;
        let raw_klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&response)?;

        if raw_klines.is_empty() {
            break;
        }

        for arr in raw_klines {
            let open_time_ms = arr.get(0).and_then(|v| v.as_i64()).ok_or_else(|| anyhow::anyhow!("open_time parse error"))?;
            let timestamp = chrono::Utc.timestamp_millis_opt(open_time_ms).single().ok_or_else(|| anyhow::anyhow!("timestamp parse error"))?;

            let parse_decimal = |idx: usize| -> Result<Decimal> {
                let s = arr.get(idx).and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("index {} not found", idx))?;
                let f: f64 = s.parse().map_err(|_| anyhow::anyhow!("parse f64 error"))?;
                Decimal::from_f64_retain(f).ok_or_else(|| anyhow::anyhow!("decimal conversion error"))
            };

            all_klines.push(KLine {
                symbol: symbol.to_string(),
                period: Period::Minute(1),
                open: parse_decimal(1)?,
                high: parse_decimal(2)?,
                low: parse_decimal(3)?,
                close: parse_decimal(4)?,
                volume: parse_decimal(5)?,
                timestamp,
                is_closed: false,
            });
        }

        if let Some(last) = all_klines.last() {
            if let Some(close_time) = last.timestamp.timestamp_millis().checked_add(1) {
                current_start = close_time;
            }
        }

        if all_klines.len() >= 1000 * 100 {
            break; // 最多1000根K线
        }
    }

    Ok(all_klines)
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(LevelFilter::INFO)
        .init();

    let config = parse_args();

    tracing::info!("========================================");
    tracing::info!("  沙盒模式（事件驱动完全重构版）");
    tracing::info!("========================================");
    tracing::info!("配置: {:?}", config.symbols);
    tracing::info!("========================================");

    // ===== 1. 创建核心组件 =====
    let gateway = Arc::new(ShadowBinanceGateway::with_default_config(config.initial_fund));
    let risk_checker = Arc::new(ShadowRiskChecker::new());

    tracing::info!("✅ 网关和风控创建成功");

    // ===== 2. 拉取历史数据 =====
    tracing::info!("正在拉取历史K线...");
    let default_symbol = DEFAULT_SYMBOL.to_string();
    let primary_symbol = config.symbols.first().unwrap_or(&default_symbol);
    let klines = fetch_klines_from_api(primary_symbol, &config.start_date, &config.end_date).await?;
    tracing::info!("✅ K线数据准备完成 ({} 根)", klines.len());

    // ===== 3. 创建事件通道 =====
    let (tick_tx, tick_rx) = mpsc::channel(1024);

    // ===== 4. 创建引擎（单事件循环）=====
    let mut engine = TradingEngine::new(
        primary_symbol.clone(),
        gateway.clone(),
        risk_checker.clone(),
    );

    tracing::info!("✅ 引擎创建成功");

    // ===== 5. 数据注入任务（生产者）=====
    // 注意：这里仍然是 tokio::spawn，但这是沙盒的责任边界
    // 沙盒只负责生成数据，不负责计算
    let tick_tx_clone = tick_tx.clone();
    let symbol_clone = primary_symbol.clone();
    let backpressure_mode = config.backpressure_mode;  // 获取背压模式
    let ws_converter = TickToWsConverter::new(primary_symbol.clone(), "1m".to_string());

    let tick_gen = StreamTickGenerator::from_loader(symbol_clone.clone(), klines.into_iter());

    // ===== 6. 启动数据注入和引擎（并发执行）=====
    // 两个任务并发运行：
    // - 生产者：生成 Tick，发送到 channel
    // - 消费者：引擎从 channel 接收，串行处理
    
    let (tx_result, rx_result) = tokio::join! {
        // 生产者任务
        async {
            let mut generator = tick_gen;
            let mut tick_count = 0u64;
            let mut tick_index = 0u8;

            while let Some(tick_data) = generator.next() {
                let is_last_tick = tick_data.is_last_in_kline;
                let ws_msg = ws_converter.convert(&tick_data, tick_index, is_last_tick);
                let is_closed = ws_msg.kline.is_closed;

                let tick = Tick {
                    symbol: tick_data.symbol.clone(),
                    price: tick_data.price,
                    qty: tick_data.qty,
                    timestamp: tick_data.timestamp,
                    sequence_id: tick_data.sequence_id,
                    kline_1m: Some(KLine {
                        symbol: tick_data.symbol.clone(),
                        period: Period::Minute(1),
                        open: tick_data.open,
                        high: tick_data.high,
                        low: tick_data.low,
                        close: tick_data.price,
                        volume: tick_data.volume,
                        timestamp: tick_data.kline_timestamp,
                        is_closed,
                    }),
                    kline_15m: None,
                    kline_1d: None,
                };

                if is_last_tick {
                    tick_index = 0;
                } else {
                    tick_index += 1;
                }

                // 根据背压模式发送 Tick
                if send_tick_with_backpressure(&tick_tx_clone, tick, backpressure_mode).await.is_err() {
                    break;
                }

                tick_count += 1;
            }

            tracing::info!("数据注入完成: {} ticks", tick_count);
        },
        
        // 消费者任务（引擎主循环）
        async {
            engine.run(tick_rx).await;
        }
    };

    // tx_result is () from producer, rx_result is () from engine
    let _ = tx_result; // suppress unused warning
    tracing::info!("引擎退出完成");

    // ===== 7. 输出结果 =====
    tracing::info!("========================================");
    tracing::info!("  测试结果");
    tracing::info!("========================================");
    
    if let Ok(account) = gateway.get_account() {
        tracing::info!("账户: equity={}, available={}", 
            account.total_equity, account.available);
    }

    tracing::info!("========================================");
    tracing::info!("  完成");
    tracing::info!("========================================");

    Ok(())
}
