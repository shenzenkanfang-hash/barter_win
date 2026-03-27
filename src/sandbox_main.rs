//! Sandbox Mode Main Entry - 沙盒模式启动器
//!
//! 完整架构：
//! - 数据源层：StreamTickGenerator → DataFeeder（后台自运行）
//! - 指标层：自行计算指标（后台自运行）
//! - 引擎层：监控波动率 → 触发任务
//! - 策略层：获取价格/指标 → 策略 → 风控 → 下单
//!
//! 运行:
//!   cargo run --bin sandbox -- --symbol HOTUSDT --fund 10000 --duration 300

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use tokio::sync::{mpsc, RwLock as TokioRwLock};
use tokio::time::sleep;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};
use dashmap::DashMap;

use a_common::volatility::{VolatilityCalc, VolatilityStats, KLineInput};
use b_data_source::{DataFeeder, KLine, Period, Tick};
use h_sandbox::{
    ShadowBinanceGateway, ShadowRiskChecker,
    historical_replay::StreamTickGenerator,
};
use f_engine::types::{OrderRequest, OrderType, Side, TaskState, RunningStatus};
use f_engine::interfaces::RiskChecker;

const DEFAULT_SYMBOL: &str = "HOTUSDT";
const DEFAULT_FUND: f64 = 10000.0;
const DEFAULT_DURATION: u64 = 60; // 缩短测试时间

#[derive(Debug, Clone)]
struct SandboxConfig {
    symbol: String,
    initial_fund: Decimal,
    duration_secs: u64,
    start_date: String,
    end_date: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            symbol: DEFAULT_SYMBOL.to_string(),
            initial_fund: dec!(10000),
            duration_secs: DEFAULT_DURATION,
            start_date: "2025-10-10".to_string(),
            end_date: "2025-10-12".to_string(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(false)
        )
        .with(LevelFilter::INFO)
        .init();

    // 解析命令行参数
    let config = parse_args();

    tracing::info!("========================================");
    tracing::info!("  沙盒模式启动器 (完整架构)");
    tracing::info!("========================================");
    tracing::info!("配置:");
    tracing::info!("  品种: {}", config.symbol);
    tracing::info!("  时间段: {} -> {}", config.start_date, config.end_date);
    tracing::info!("  初始资金: {}", config.initial_fund);
    tracing::info!("========================================\n");

    // ========================================
    // 1. 创建共享组件
    // ========================================
    
    // DataFeeder - 行情缓存（所有组件共享）
    let data_feeder = Arc::new(DataFeeder::new());
    tracing::info!("✅ 1. DataFeeder 创建成功");

    // 指标缓存（所有组件共享）
    let indicator_cache = Arc::new(IndicatorCache::new());
    tracing::info!("✅ 2. IndicatorCache 创建成功");

    // ShadowBinanceGateway - 订单拦截
    let gateway = Arc::new(ShadowBinanceGateway::with_default_config(config.initial_fund));
    tracing::info!("✅ 3. ShadowGateway 创建成功");

    // ShadowRiskChecker - 风控检查
    let risk_checker = Arc::new(ShadowRiskChecker::new());
    tracing::info!("✅ 4. ShadowRiskChecker 创建成功");

    // ========================================
    // 2. 从API拉取K线数据
    // ========================================
    tracing::info!("正在从币安API拉取历史K线...");
    let klines = fetch_klines_from_api(&config.symbol, &config.start_date, &config.end_date).await?;
    let kline_count = klines.len();
    tracing::info!("✅ 5. K线数据准备完成 ({} 根)", kline_count);

    // ========================================
    // 3. 创建 TickGenerator
    // ========================================
    let tick_gen = StreamTickGenerator::from_loader(config.symbol.clone(), klines.into_iter());
    let total_ticks = kline_count * 60;
    tracing::info!("✅ 6. TickGenerator 创建成功 (预计 {} ticks)", total_ticks);

    // ========================================
    // 4. 创建引擎
    // ========================================
    let engine = Arc::new(TradeManagerEngine::new(
        Arc::clone(&data_feeder),
        Arc::clone(&indicator_cache),
        Arc::clone(&gateway),
        Arc::clone(&risk_checker),
    ));
    tracing::info!("✅ 7. TradeManager 引擎创建成功");

    // ========================================
    // 5. 创建事件通道（核心改动）
    // ========================================
    let (tick_tx, tick_rx) = mpsc::channel(1024);

    tracing::info!("✅ 8. 事件通道创建成功 (背压: 1024)");

    // ========================================
    // 6. 启动事件驱动交易循环
    // ========================================
    let trading_loop = Arc::new(TradingLoop::new(
        config.symbol.clone(),
        Arc::clone(&data_feeder),
        Arc::clone(&indicator_cache),
        Arc::clone(&gateway),
        Arc::clone(&risk_checker),
        Arc::clone(&engine),
        1,  // trigger_interval: 每个 tick 都执行策略
    ));

    let loop_handle = tokio::spawn({
        let loop_core = Arc::clone(&trading_loop);
        async move {
            loop_core.run(tick_rx).await;
        }
    });

    // ========================================
    // 7. 启动数据注入层（事件驱动生产者）
    // ========================================
    let tick_tx_clone = tick_tx.clone();

    tokio::spawn(async move {
        let mut generator = tick_gen;
        let mut tick_idx = 0u64;

        while let Some(tick_data) = generator.next() {
            // 构建 Tick
            let tick = Tick {
                symbol: tick_data.symbol.clone(),
                price: tick_data.price,
                qty: tick_data.qty,
                timestamp: tick_data.timestamp,
                kline_1m: Some(KLine {
                    symbol: tick_data.symbol.clone(),
                    period: Period::Minute(1),
                    open: tick_data.open,
                    high: tick_data.high,
                    low: tick_data.low,
                    close: tick_data.price,
                    volume: tick_data.volume,
                    timestamp: tick_data.kline_timestamp,
                }),
                kline_15m: None,
                kline_1d: None,
            };

            tick_idx += 1;

            // 发送到事件通道（背压处理：阻塞等待）
            if tick_tx_clone.send(tick).await.is_err() {
                tracing::info!("事件通道已关闭，停止注入");
                break;
            }
        }

        tracing::info!("数据注入层完成，共 {} ticks", tick_idx);
        // 注入完成，通道发送端 drop，循环将自然结束
    });

    tracing::info!("✅ 9. 数据注入层已启动（事件驱动）");

    // ========================================
    // 8. 等待结束
    // ========================================
    let duration = Duration::from_secs(config.duration_secs);
    let start_wait = std::time::Instant::now();

    tokio::select! {
        _ = loop_handle => {
            tracing::info!("交易循环已结束");
        }
        _ = sleep(duration) => {
            tracing::info!("达到最大时长 {}s", config.duration_secs);
            // 超时 drop 发送端，循环会自然结束
        }
    }

    tracing::info!("等待耗时: {}s", start_wait.elapsed().as_secs());

    // ========================================
    // 9. 输出结果
    // ========================================
    tracing::info!("\n========================================");
    tracing::info!("  测试结果");
    tracing::info!("========================================");
    
    // 打印账户信息
    print_account_info(&gateway);
    
    // 打印最终统计
    engine.print_stats().await;

    // 打印最终指标
    tracing::info!("\n========================================");
    tracing::info!("  最终指标");
    tracing::info!("========================================");
    if let Some(indicators) = indicator_cache.get(&DEFAULT_SYMBOL.to_string()) {
        tracing::info!("  RSI: {:?}", indicators.rsi);
        tracing::info!("  EMA5: {:?}", indicators.ema5);
        tracing::info!("  EMA20: {:?}", indicators.ema20);
        tracing::info!("  波动率: {:?}", indicators.volatility);
    }

    tracing::info!("\n========================================");
    tracing::info!("  沙盒模式测试完成");
    tracing::info!("========================================");

    Ok(())
}

// ============================================================================
// 指标缓存
// ============================================================================

#[derive(Debug, Clone, Default)]
struct Indicators {
    rsi: Option<Decimal>,
    ema5: Option<Decimal>,
    ema20: Option<Decimal>,
    volatility: Decimal,
    price_history: Vec<Decimal>,
    /// 15m 波动率计算器
    volatility_calc: VolatilityCalc,
    /// 15m 波动率
    vol_15m: Decimal,
}

impl Indicators {
    fn with_volatility_calc() -> Self {
        Self {
            volatility_calc: VolatilityCalc::new(),
            ..Default::default()
        }
    }
}

struct IndicatorCache {
    // 使用 DashMap 替代 RwLock，避免阻塞问题
    cache: dashmap::DashMap<String, Indicators>,
}

impl IndicatorCache {
    fn new() -> Self {
        Self {
            cache: dashmap::DashMap::new(),
        }
    }

    fn update(&self, tick: &Tick) {
        let symbol = tick.symbol.clone();
        let price = tick.price;

        // DashMap 直接操作，无需锁
        let mut indicators = self.cache.entry(symbol.clone())
            .or_insert_with(Indicators::with_volatility_calc);

        // 添加到价格历史
        indicators.price_history.push(price);
        if indicators.price_history.len() > 100 {
            indicators.price_history.remove(0);
        }

        // 计算波动率
        if indicators.price_history.len() >= 20 {
            let recent = &indicators.price_history[indicators.price_history.len().saturating_sub(20)..];
            let mean: Decimal = recent.iter().sum::<Decimal>() / Decimal::from(recent.len());
            let variance: Decimal = recent.iter()
                .map(|p| (*p - mean) * (*p - mean))
                .sum::<Decimal>() / Decimal::from(recent.len());
            indicators.volatility = variance.sqrt().unwrap_or(Decimal::ZERO);
        }

        // 简化 RSI 计算
        if indicators.price_history.len() >= 14 {
            let gains: Decimal = indicators.price_history.windows(2)
                .filter(|w| w[1] > w[0])
                .map(|w| w[1] - w[0])
                .sum();
            let losses: Decimal = indicators.price_history.windows(2)
                .filter(|w| w[1] < w[0])
                .map(|w| w[0] - w[1])
                .sum();

            let avg_gain = gains / dec!(14);
            let avg_loss = losses / dec!(14);

            if avg_loss.is_zero() {
                indicators.rsi = Some(dec!(100));
            } else {
                let rs = avg_gain / avg_loss;
                indicators.rsi = Some(dec!(100) - dec!(100) / (dec!(1) + rs));
            }
        }

        // 真正的 EMA 计算
        if indicators.price_history.len() >= 5 {
            indicators.ema5 = Some(Self::calc_ema(&indicators.price_history, 5));
        }
        if indicators.price_history.len() >= 20 {
            indicators.ema20 = Some(Self::calc_ema(&indicators.price_history, 20));
        }

        // 15m 波动率计算（使用 Tick 中的 1m K线）
        if let Some(ref kline) = tick.kline_1m {
            let kline_input = KLineInput {
                open: kline.open,
                high: kline.high,
                low: kline.low,
                close: kline.close,
                timestamp: chrono::Utc::now(),
            };
            let stats = indicators.volatility_calc.update(kline_input);
            indicators.vol_15m = stats.vol_15m;
        }
    }
    
    /// 计算 EMA
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

    fn calculate_indicators(&self, _kline: &KLine) {
        // 指标计算已在 update 中完成
    }

    fn get(&self, symbol: &str) -> Option<Indicators> {
        self.cache.get(symbol).map(|r| r.clone())
    }

    fn get_volatility(&self, symbol: &str) -> Decimal {
        self.cache.get(symbol)
            .map(|r| r.vol_15m)  // 使用 15m 波动率
            .unwrap_or(Decimal::ZERO)
    }
}

// ============================================================================
// 事件驱动核心 - TradingLoop
// ============================================================================

/// 事件驱动交易循环
/// 核心设计：一个 Tick 驱动完整处理链，无 sleep 轮询
struct TradingLoop {
    /// 交易品种
    symbol: String,
    /// 数据源
    data_feeder: Arc<DataFeeder>,
    /// 指标缓存
    indicator_cache: Arc<IndicatorCache>,
    /// 订单网关
    gateway: Arc<ShadowBinanceGateway>,
    /// 风控检查
    risk_checker: Arc<ShadowRiskChecker>,
    /// 引擎（用于触发策略任务）
    engine: Arc<TradeManagerEngine>,
    /// 策略互斥锁（防止同一品种重复下单）
    strategy_locks: Arc<DashMap<String, ()>>,
    /// 策略执行间隔（每 N 个 tick 触发一次）
    trigger_interval: u64,
}

impl TradingLoop {
    /// 创建新的交易循环
    fn new(
        symbol: String,
        data_feeder: Arc<DataFeeder>,
        indicator_cache: Arc<IndicatorCache>,
        gateway: Arc<ShadowBinanceGateway>,
        risk_checker: Arc<ShadowRiskChecker>,
        engine: Arc<TradeManagerEngine>,
        trigger_interval: u64,
    ) -> Self {
        Self {
            symbol,
            data_feeder,
            indicator_cache,
            gateway,
            risk_checker,
            engine,
            strategy_locks: Arc::new(DashMap::new()),
            trigger_interval,
        }
    }

    /// 事件循环主函数 - 串行处理（修复乱序）
    async fn run(self: Arc<Self>, mut tick_rx: mpsc::Receiver<Tick>) {
        let symbol = self.symbol.clone();
        let tick_counter = Arc::new(AtomicU64::new(0));

        tracing::info!("[TradingLoop] {} 串行事件循环启动", symbol);

        // 关键修复：去掉 tokio::spawn，直接 await 处理
        // 保证每个 Tick 处理完才处理下一个，物理上保证顺序
        while let Some(tick) = tick_rx.recv().await {
            let this = Arc::clone(&self);
            let counter = Arc::clone(&tick_counter);

            // 每一个 Tick 处理完之前，不会 recv 下一个
            if let Err(e) = this.on_tick(tick, counter).await {
                tracing::error!("[TradingLoop] 处理 Tick 出错: {:?}", e);
                // 继续处理，不退出
            }
        }

        tracing::info!("[TradingLoop] {} 事件循环正常结束", symbol);
    }

    /// 处理单个 Tick 事件 - 返回 Result 以便错误传播
    async fn on_tick(&self, tick: Tick, counter: Arc<AtomicU64>) -> anyhow::Result<()> {
        // 1. 增加计数
        counter.fetch_add(1, Ordering::SeqCst);

        // 2. 写入数据源（克隆避免大对象共享）
        self.data_feeder.push_tick(tick.clone());

        // 3. 增量计算指标（同步调用）
        self.indicator_cache.update(&tick);

        // 4. 事件驱动波动率检查
        self.check_volatility(&tick).await;

        // 5. 可配置间隔执行策略
        let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
        if count % self.trigger_interval == 0 {
            self.run_strategy(&tick).await;
        }

        Ok(())
    }

    /// 波动率事件触发检查
    async fn check_volatility(&self, tick: &Tick) {
        let volatility = self.indicator_cache.get_volatility(&tick.symbol);
        let threshold = dec!(0.02);  // 2% 波动率阈值

        if volatility > threshold && !self.engine.has_task(&tick.symbol).await {
            // 检查互斥锁
            if self.strategy_locks.contains_key(&tick.symbol) {
                tracing::debug!("[TradingLoop] {} 策略任务已在运行，跳过", tick.symbol);
                return;
            }

            // 添加互斥锁
            self.strategy_locks.insert(tick.symbol.clone(), ());

            // 触发策略任务
            self.engine.spawn_strategy_task(tick.symbol.clone()).await;
        }
    }

    /// 执行策略（核心交易逻辑）
    async fn run_strategy(&self, tick: &Tick) {
        // 策略逻辑与原版相同，仅改触发方式为事件驱动
        // 引用原 TradeManagerEngine::spawn_strategy_task 中的策略逻辑
    }
}

// ============================================================================
// TradeManager 引擎
// ============================================================================

/// 任务 Map 类型
type TaskMap = std::collections::HashMap<String, Arc<TokioRwLock<TaskState>>>;

/// TradeManager 引擎
/// 
/// 引擎层职责：
/// - 监控波动率
/// - 波动率 > 阈值 → spawn_task
/// - 心跳检查
/// - 任务移除
/// 
/// 策略层职责：
/// - 从 DataFeeder 获取价格
/// - 从 IndicatorCache 获取指标
/// - 策略计算
/// - 风控 + 下单
struct TradeManagerEngine {
    /// DataFeeder
    data_feeder: Arc<DataFeeder>,
    /// 指标缓存
    indicator_cache: Arc<IndicatorCache>,
    /// 网关
    gateway: Arc<ShadowBinanceGateway>,
    /// 风控
    risk_checker: Arc<ShadowRiskChecker>,
    
    /// 任务注册表
    tasks: Arc<TokioRwLock<TaskMap>>,
    /// 心跳超时（秒）
    heartbeat_timeout: i64,
    /// 全局锁（下单时使用）
    global_lock: Arc<tokio::sync::Mutex<()>>,
    /// 统计
    stats: Arc<TokioRwLock<EngineStats>>,
}

#[derive(Debug, Default)]
struct EngineStats {
    total_orders: u32,
    total_trades: u32,
    total_errors: u32,
}

impl TradeManagerEngine {
    fn new(
        data_feeder: Arc<DataFeeder>,
        indicator_cache: Arc<IndicatorCache>,
        gateway: Arc<ShadowBinanceGateway>,
        risk_checker: Arc<ShadowRiskChecker>,
    ) -> Self {
        Self {
            data_feeder,
            indicator_cache,
            gateway,
            risk_checker,
            tasks: Arc::new(TokioRwLock::new(std::collections::HashMap::new())),
            heartbeat_timeout: 90,
            global_lock: Arc::new(tokio::sync::Mutex::new(())),
            stats: Arc::new(TokioRwLock::new(EngineStats::default())),
        }
    }

    /// 获取波动率
    fn get_volatility(&self, symbol: &str) -> Decimal {
        if let Some(indicators) = self.indicator_cache.get(symbol) {
            indicators.volatility
        } else {
            Decimal::ZERO
        }
    }

    /// 检查任务是否存在
    async fn has_task(&self, symbol: &str) -> bool {
        self.tasks.read().await.contains_key(symbol)
    }

    /// 获取任务数量
    async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    /// 检查是否为空
    async fn is_empty(&self) -> bool {
        self.tasks.read().await.is_empty()
    }

    /// 打印统计
    async fn print_stats(&self) {
        let stats = self.stats.read().await;
        tracing::info!("引擎统计:");
        tracing::info!("  总订单: {}", stats.total_orders);
        tracing::info!("  总交易: {}", stats.total_trades);
        tracing::info!("  总错误: {}", stats.total_errors);
    }

    /// 启动策略任务（事件驱动版本，无 interval_ms 参数）
    async fn spawn_strategy_task(&self, symbol: String) {
        self.spawn_strategy_task_with_interval(symbol, 50).await;
    }

    /// 启动策略任务（带间隔参数）
    async fn spawn_strategy_task_with_interval(&self, symbol: String, interval_ms: u64) {
        // 检查是否已存在
        {
            let tasks = self.tasks.read().await;
            if tasks.contains_key(&symbol) {
                return;
            }
        }

        // 创建任务状态
        let state = Arc::new(TokioRwLock::new(TaskState::new(symbol.clone(), interval_ms)));
        
        // 注册
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(symbol.clone(), Arc::clone(&state));
        }

        // 克隆引用用于任务
        let data_feeder = Arc::clone(&self.data_feeder);
        let indicator_cache = Arc::clone(&self.indicator_cache);
        let gateway = Arc::clone(&self.gateway);
        let risk_checker = Arc::clone(&self.risk_checker);
        let global_lock = Arc::clone(&self.global_lock);
        let tasks_ref = Arc::clone(&self.tasks);
        let stats = Arc::clone(&self.stats);

        tracing::info!("[Engine] 启动策略任务: {} (interval: {}ms)", symbol, interval_ms);

        // Spawn 异步任务
        tokio::spawn(async move {
            // 策略参数
            let mut has_position = false;
            let mut entry_price = Decimal::ZERO;
            let mut signal_count = 0i64;
            
            // 任务循环
            loop {
                // 调试：打印心跳
                tracing::debug!("[Strategy] {} 任务执行中, has_position={}", symbol, has_position);
                
                // 1. 检查禁止
                {
                    let s = state.read().await;
                    if s.is_forbidden() {
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    if s.status == RunningStatus::Ended {
                        break;
                    }
                }

                // 2. 获取全局锁
                let _lock = global_lock.lock().await;

                // 3. 从 DataFeeder 获取当前价格
                let current_price = {
                    data_feeder.ws_get_1m(&symbol)
                        .map(|k| k.close)
                        .unwrap_or(Decimal::ZERO)
                };

                // 4. 从 IndicatorCache 获取指标
                let indicators = indicator_cache.get(&symbol);

                // 调试：打印指标
                if signal_count % 20 == 0 {
                    if let Some(ind) = indicators.as_ref() {
                        tracing::info!("[Strategy] price={}, ema5={:?}, ema20={:?}, rsi={:?}, has_pos={}", 
                            current_price, ind.ema5, ind.ema20, ind.rsi, has_position);
                    } else {
                        tracing::info!("[Strategy] price={}, 无指标数据, has_pos={}", current_price, has_position);
                    }
                }

                // 5. 策略计算：基于 EMA 金叉/死叉
                let should_open = if !has_position && !current_price.is_zero() {
                    if let Some(ind) = indicators.as_ref() {
                        if let (Some(ema5), Some(ema20), Some(rsi)) = (ind.ema5, ind.ema20, ind.rsi) {
                            let cond = ema5 > ema20 && rsi < dec!(70) && rsi > dec!(30);
                            if signal_count % 20 == 0 {
                                tracing::info!("[Strategy] should_open 检查: ema5={} > ema20={} = {}, rsi={}, cond={}", 
                                    ema5, ema20, ema5 > ema20, rsi, cond);
                            }
                            cond
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                
                let should_close = if has_position {
                    if let Some(ind) = indicators.as_ref() {
                        // EMA5 < EMA20 或 RSI > 70 → 平仓
                        if let (Some(ema5), Some(ema20)) = (ind.ema5, ind.ema20) {
                            if let Some(rsi) = ind.rsi {
                                ema5 < ema20 || rsi > dec!(70)
                            } else {
                                ema5 < ema20
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // 6. 执行交易
                if should_open && !current_price.is_zero() {
                    tracing::info!("[{}] should_open=true，准备下单!", symbol);
                    
                    // 风控检查
                    let account = match gateway.get_account() {
                        Ok(acc) => {
                            tracing::info!("[{}] 账户信息: equity={}, available={}", symbol, acc.total_equity, acc.available);
                            acc
                        }
                        Err(e) => {
                            tracing::warn!("[{}] 获取账户失败: {:?}", symbol, e);
                            drop(_lock);
                            sleep(Duration::from_millis(interval_ms)).await;
                            signal_count += 1;
                            state.write().await.heartbeat();
                            continue;
                        }
                    };

                    let order_req = OrderRequest {
                        symbol: symbol.clone(),
                        side: Side::Buy,
                        order_type: OrderType::Market,
                        qty: dec!(0.01),
                        price: Some(current_price),
                    };

                    let risk_result = risk_checker.pre_check(&order_req, &account);
                    if risk_result.pre_failed() {
                        tracing::warn!("[{}] 风控拦截: {:?}", symbol, risk_result);
                    } else {
                        tracing::info!("[{}] 风控通过，准备下单...", symbol);
                        // 下单
                        match gateway.place_order(order_req) {
                            Ok(result) => {
                                has_position = true;
                                entry_price = current_price;
                                
                                let mut s = stats.write().await;
                                s.total_orders += 1;
                                s.total_trades += 1;
                                
                                tracing::info!(
                                    "[{}] 开仓成功 @ {} (qty: {})",
                                    symbol, current_price, result.filled_qty
                                );
                            }
                            Err(e) => {
                                let mut s = stats.write().await;
                                s.total_errors += 1;
                                tracing::warn!("[{}] 开仓失败: {:?}", symbol, e);
                            }
                        }
                    }
                }

                if should_close && has_position {
                    // 风控检查
                    let account = match gateway.get_account() {
                        Ok(acc) => acc,
                        Err(_) => {
                            drop(_lock);
                            sleep(Duration::from_millis(interval_ms)).await;
                            signal_count += 1;
                            state.write().await.heartbeat();
                            continue;
                        }
                    };

                    let order_req = OrderRequest {
                        symbol: symbol.clone(),
                        side: Side::Sell,
                        order_type: OrderType::Market,
                        qty: dec!(0.01),
                        price: Some(current_price),
                    };

                    if risk_checker.pre_check(&order_req, &account).pre_failed() {
                        tracing::debug!("[{}] 风控拦截平仓", symbol);
                    } else {
                        // 下单
                        match gateway.place_order(order_req) {
                            Ok(result) => {
                                let pnl = (current_price - entry_price) * result.filled_qty;
                                has_position = false;
                                
                                let mut s = stats.write().await;
                                s.total_orders += 1;
                                
                                tracing::info!(
                                    "[{}] 平仓 @ {} (qty: {}, PnL: {})",
                                    symbol, current_price, result.filled_qty, pnl
                                );
                                
                                // 平仓完成，退出任务
                                state.write().await.end("TradeComplete".to_string());
                                break;
                            }
                            Err(e) => {
                                let mut s = stats.write().await;
                                s.total_errors += 1;
                                tracing::warn!("[{}] 平仓失败: {:?}", symbol, e);
                            }
                        }
                    }
                }

                // 7. 更新心跳
                state.write().await.heartbeat();

                // 8. 释放锁
                drop(_lock);

                // 9. 信号计数
                signal_count += 1;

                // 10. 等待下一个周期
                sleep(Duration::from_millis(interval_ms)).await;
            }

            // 11. 从注册表移除
            let mut tasks = tasks_ref.write().await;
            tasks.remove(&symbol);
            
            tracing::info!("[Engine] 策略任务结束: {}", symbol);
        });
    }

    /// 检查任务
    async fn check_tasks(&self) {
        let mut to_remove: Vec<String> = Vec::new();

        let tasks = self.tasks.read().await;
        for (symbol, state_arc) in tasks.iter() {
            let s = state_arc.read().await;
            if s.status == RunningStatus::Ended {
                to_remove.push(symbol.clone());
                tracing::info!("[Engine] 移除已结束任务: {}", symbol);
            }
        }
        drop(tasks);

        if !to_remove.is_empty() {
            let mut tasks = self.tasks.write().await;
            for symbol in &to_remove {
                tasks.remove(symbol);
            }
        }
    }

    /// 检查心跳
    async fn check_heartbeat(&self) {
        let now = Utc::now().timestamp();

        let tasks = self.tasks.read().await;
        for (symbol, state_arc) in tasks.iter() {
            let s = state_arc.read().await;
            if s.status == RunningStatus::Running && s.last_beat < now - self.heartbeat_timeout {
                tracing::warn!("[Engine] 任务心跳超时: {} (last_beat: {})", symbol, s.last_beat);
            }
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

fn parse_args() -> SandboxConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut config = SandboxConfig::default();

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--symbol" => {
                if i + 1 < args.len() {
                    config.symbol = args[i + 1].clone();
                }
            }
            "--fund" => {
                if i + 1 < args.len() {
                    if let Ok(f) = args[i + 1].parse::<f64>() {
                        config.initial_fund = Decimal::try_from(f).unwrap_or(dec!(10000));
                    }
                }
            }
            "--duration" => {
                if i + 1 < args.len() {
                    if let Ok(d) = args[i + 1].parse::<u64>() {
                        config.duration_secs = d;
                    }
                }
            }
            _ => {}
        }
    }

    config
}

/// 从币安API拉取K线
async fn fetch_klines_from_api(
    symbol: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    // 解析日期（直接作为UTC时间）
    let start_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 00:00:00", start_date), "%Y-%m-%d %H:%M:%S"
    )?;
    let end_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 23:59:59", end_date), "%Y-%m-%d %H:%M:%S"
    )?;

    let start_ms = start_dt.and_utc().timestamp_millis();
    let end_ms = end_dt.and_utc().timestamp_millis();

    tracing::info!("从API拉取 {} {} -> {} (UTC)", symbol, start_date, end_date);

    // 分页拉取
    let mut all_raw_klines: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut current_start = start_ms;
    let max_requests = 100;
    let page_limit = 1000;

    let client = reqwest::Client::new();

    for page in 0..max_requests {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval=1m&limit={}&startTime={}&endTime={}",
            symbol.to_uppercase(),
            page_limit,
            current_start,
            end_ms
        );

        let response = client.get(&url).send().await?.text().await?;
        let raw_klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&response)
            .map_err(|e| format!("JSON解析失败: {} | Body: {}", e, response))?;

        if raw_klines.is_empty() {
            break;
        }

        let page_count = raw_klines.len();
        all_raw_klines.extend(raw_klines);

        if page_count < page_limit {
            break;
        }

        if let Some(last) = all_raw_klines.last() {
            if let Some(close_time) = last.get(6).and_then(|v| v.as_i64()) {
                current_start = close_time + 1;
            } else {
                break;
            }
        }

        if page >= max_requests - 1 {
            break;
        }
    }

    if all_raw_klines.is_empty() {
        return Err("未获取到K线数据".into());
    }

    tracing::info!("共获取K线: {} 条", all_raw_klines.len());

    // 转换为内部 KLine 格式
    let klines: Vec<KLine> = all_raw_klines
        .into_iter()
        .filter_map(|arr| {
            let open_time_ms = arr.get(0)?.as_i64()?;
            let timestamp = chrono::Utc.timestamp_millis_opt(open_time_ms).single()?;

            let parse_decimal = |idx: usize| -> Option<Decimal> {
                let s = arr.get(idx)?.as_str()?;
                let f: f64 = s.parse().ok()?;
                Decimal::from_f64_retain(f)
            };

            Some(KLine {
                symbol: symbol.to_string(),
                period: Period::Minute(1),
                open: parse_decimal(1)?,
                high: parse_decimal(2)?,
                low: parse_decimal(3)?,
                close: parse_decimal(4)?,
                volume: parse_decimal(5)?,
                timestamp,
            })
        })
        .collect();

    Ok(klines)
}

/// 打印账户详细信息
fn print_account_info(gateway: &Arc<ShadowBinanceGateway>) {
    match gateway.get_account() {
        Ok(account) => {
            tracing::info!("----------------------------------------");
            tracing::info!("账户信息:");
            tracing::info!("  总权益: {}", account.total_equity);
            tracing::info!("  可用余额: {}", account.available);
            tracing::info!("  冻结保证金: {}", account.frozen_margin);
            tracing::info!("  未实现盈亏: {}", account.unrealized_pnl);
            tracing::info!("----------------------------------------");
        }
        Err(e) => {
            tracing::error!("获取账户信息失败: {:?}", e);
        }
    }

    match gateway.get_position(&DEFAULT_SYMBOL) {
        Ok(Some(pos)) => {
            tracing::info!("持仓信息:");
            tracing::info!("  多头: {} @ {}", pos.long_qty, pos.long_avg_price);
            tracing::info!("  空头: {} @ {}", pos.short_qty, pos.short_avg_price);
            tracing::info!("  未实现盈亏: {}", pos.unrealized_pnl);
        }
        Ok(None) => {
            tracing::info!("无持仓");
        }
        Err(e) => {
            tracing::error!("获取持仓信息失败: {:?}", e);
        }
    }
}
