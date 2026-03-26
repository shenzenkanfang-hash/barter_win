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

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use tokio::sync::RwLock as TokioRwLock;
use tokio::time::sleep;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};

use b_data_source::{DataFeeder, KLine, Period, Tick};
use h_sandbox::{
    ShadowBinanceGateway, ShadowRiskChecker,
    historical_replay::StreamTickGenerator,
};
use f_engine::core::engine::{TaskState, RunningStatus};
use f_engine::types::{OrderRequest, OrderType, Side};
use f_engine::RiskChecker;

const DEFAULT_SYMBOL: &str = "HOTUSDT";
const DEFAULT_FUND: f64 = 10000.0;
const DEFAULT_DURATION: u64 = 300;

#[derive(Debug, Clone)]
struct SandboxConfig {
    symbol: String,
    initial_fund: Decimal,
    duration_secs: u64,
    fast_mode: bool,
    start_date: String,
    end_date: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            symbol: DEFAULT_SYMBOL.to_string(),
            initial_fund: dec!(10000),
            duration_secs: DEFAULT_DURATION,
            fast_mode: false,
            start_date: "2025-10-09".to_string(),
            end_date: "2025-10-11".to_string(),
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
        risk_checker,
    ));
    tracing::info!("✅ 7. TradeManager 引擎创建成功");

    // ========================================
    // 5. 启动数据源层（后台自运行）
    // ========================================
    let data_feeder_for_gen = Arc::clone(&data_feeder);
    let indicator_cache_for_gen = Arc::clone(&indicator_cache);
    
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

            // 存入 DataFeeder
            data_feeder_for_gen.push_tick(tick.clone());
            
            // 更新指标缓存
            indicator_cache_for_gen.update(&tick);
        }
        
        tracing::info!("数据源层完成，共 {} ticks", tick_idx);
    });
    tracing::info!("✅ 8. 数据源层已启动");

    // ========================================
    // 6. 启动指标层（后台自运行）
    // ========================================
    let data_feeder_for_indicator = Arc::clone(&data_feeder);
    let indicator_cache_for_indicator = Arc::clone(&indicator_cache);
    
    tokio::spawn(async move {
        loop {
            // 从 DataFeeder 获取最新 K 线
            if let Some(kline) = data_feeder_for_indicator.ws_get_1m(&DEFAULT_SYMBOL.to_string()) {
                // 计算指标
                indicator_cache_for_indicator.calculate_indicators(&kline);
            }
            
            sleep(Duration::from_millis(50)).await;
        }
    });
    tracing::info!("✅ 9. 指标层已启动");

    // ========================================
    // 7. 引擎主循环（监控波动率 → 触发任务）
    // ========================================
    let engine_handle = tokio::spawn({
        let config = config.clone();
        let engine = Arc::clone(&engine);
        
        async move {
            tracing::info!("引擎主循环启动");
            
            // 波动率阈值
            let volatility_threshold = dec!(0.02); // 2%
            
            loop {
                // 1. 获取波动率
                let volatility = engine.get_volatility(&config.symbol).await;
                
                // 2. 检查是否超过阈值
                if volatility > volatility_threshold {
                    // 3. 检查任务是否已存在
                    if !engine.has_task(&config.symbol).await {
                        // 4. 触发策略任务
                        engine.spawn_strategy_task(
                            config.symbol.clone(),
                            50, // 50ms 间隔
                        ).await;
                        
                        tracing::info!(
                            "[Engine] 波动率 {} > {}，触发策略任务",
                            volatility, volatility_threshold
                        );
                    }
                }
                
                // 5. 检查任务状态
                engine.check_tasks().await;
                
                // 6. 检查心跳
                engine.check_heartbeat().await;
                
                // 7. 检查是否全部任务结束
                if engine.is_empty().await {
                    tracing::info!("所有任务已结束");
                    break;
                }

                sleep(Duration::from_secs(1)).await;
            }
            
            tracing::info!("引擎主循环结束");
        }
    });

    // ========================================
    // 8. 等待结束
    // ========================================
    let max_duration = if config.fast_mode {
        Some(Duration::from_secs(config.duration_secs))
    } else {
        None
    };

    tokio::select! {
        _ = engine_handle => {
            tracing::info!("引擎已结束");
        }
        _ = async {
            if let Some(dur) = max_duration {
                sleep(dur).await;
                tracing::info!("达到最大时长 {}s", config.duration_secs);
            }
        } => {}
    }

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
    if let Some(indicators) = indicator_cache.get(&DEFAULT_SYMBOL.to_string()).await {
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
}

struct IndicatorCache {
    cache: std::sync::Arc<TokioRwLock<std::collections::HashMap<String, Indicators>>>,
}

impl IndicatorCache {
    fn new() -> Self {
        Self {
            cache: std::sync::Arc::new(TokioRwLock::new(std::collections::HashMap::new())),
        }
    }

    fn update(&self, tick: &Tick) {
        let symbol = tick.symbol.clone();
        let price = tick.price;
        
        let cache = self.cache.clone();
        tokio::spawn(async move {
            let mut cache_guard = cache.write().await;
            let indicators = cache_guard
                .entry(symbol.clone())
                .or_insert_with(Indicators::default);
            
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
            
            // 简化 EMA 计算
            if indicators.price_history.len() >= 5 {
                indicators.ema5 = Some(price); // 简化
            }
            if indicators.price_history.len() >= 20 {
                indicators.ema20 = Some(price); // 简化
            }
        });
    }

    fn calculate_indicators(&self, _kline: &KLine) {
        // 指标计算已在 update 中完成
    }

    async fn get(&self, symbol: &str) -> Option<Indicators> {
        self.cache.read().await.get(symbol).cloned()
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
    async fn get_volatility(&self, symbol: &str) -> Decimal {
        if let Some(indicators) = self.indicator_cache.get(symbol).await {
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

    /// 启动策略任务
    async fn spawn_strategy_task(&self, symbol: String, interval_ms: u64) {
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
                let indicators = indicator_cache.get(&symbol).await;

                // 5. 策略计算
                let should_open = !has_position 
                    && signal_count % 20 == 10
                    && indicators.as_ref()
                        .and_then(|i| i.rsi)
                        .map(|rsi| rsi < dec!(70))
                        .unwrap_or(false);
                
                let should_close = has_position 
                    && signal_count % 30 == 20;

                // 6. 执行交易
                if should_open && !current_price.is_zero() {
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
                        side: Side::Buy,
                        order_type: OrderType::Market,
                        qty: dec!(0.01),
                        price: Some(current_price),
                    };

                    if risk_checker.pre_check(&order_req, &account).pre_failed() {
                        tracing::debug!("[{}] 风控拦截开仓", symbol);
                    } else {
                        // 下单
                        match gateway.place_order(order_req) {
                            Ok(result) => {
                                has_position = true;
                                entry_price = current_price;
                                
                                let mut s = stats.write().await;
                                s.total_orders += 1;
                                s.total_trades += 1;
                                
                                tracing::info!(
                                    "[{}] 开仓 @ {} (qty: {})",
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
            "--start" => {
                if i + 1 < args.len() {
                    config.start_date = args[i + 1].clone();
                }
            }
            "--end" => {
                if i + 1 < args.len() {
                    config.end_date = args[i + 1].clone();
                }
            }
            "--fast" => {
                config.fast_mode = true;
            }
            "--slow" => {
                config.fast_mode = false;
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
    // 解析日期
    let start_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 00:00:00", start_date), "%Y-%m-%d %H:%M:%S"
    )?;
    let end_dt = chrono::NaiveDateTime::parse_from_str(
        &format!("{} 00:00:00", end_date), "%Y-%m-%d %H:%M:%S"
    )?;

    let start_ms = chrono::Utc.from_utc_datetime(&start_dt).timestamp_millis();
    let end_ms = chrono::Utc.from_utc_datetime(&end_dt).timestamp_millis();

    tracing::info!("从API拉取 {} {} -> {}", symbol, start_date, end_date);

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
