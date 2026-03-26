//! Sandbox Mode Main Entry - 沙盒模式启动器
//!
//! TradeManager 架构：
//! - 引擎层：触发器扫描、任务注册、心跳检查、持久化
//! - 策略层：每个任务独立循环（策略计算 + 风控 + 下单）
//!
//! 运行:
//!   cargo run --bin sandbox -- --symbol HOTUSDT --fund 10000 --duration 300

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::{mpsc, RwLock as TokioRwLock};
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
    tracing::info!("  沙盒模式启动器 (TradeManager 架构)");
    tracing::info!("========================================");
    tracing::info!("配置:");
    tracing::info!("  品种: {}", config.symbol);
    tracing::info!("  时间段: {} -> {}", config.start_date, config.end_date);
    tracing::info!("  初始资金: {}", config.initial_fund);
    tracing::info!("  测试时长: {}s", config.duration_secs);
    tracing::info!("  模式: {}", if config.fast_mode { "快速" } else { "实时" });
    tracing::info!("========================================\n");

    // ========================================
    // 1. 创建共享组件
    // ========================================
    
    // DataFeeder - 行情缓存
    let data_feeder = Arc::new(DataFeeder::new());
    tracing::info!("✅ 1. DataFeeder 创建成功");

    // ShadowBinanceGateway - 订单拦截
    let gateway = Arc::new(ShadowBinanceGateway::with_default_config(config.initial_fund));
    tracing::info!("✅ 2. ShadowGateway 创建成功");

    // ShadowRiskChecker - 风控检查
    let risk_checker = Arc::new(ShadowRiskChecker::new());
    tracing::info!("✅ 3. ShadowRiskChecker 创建成功");

    // ========================================
    // 2. 从API拉取K线数据
    // ========================================
    tracing::info!("正在从币安API拉取历史K线...");
    let klines = fetch_klines_from_api(&config.symbol, &config.start_date, &config.end_date).await?;
    let kline_count = klines.len();
    tracing::info!("✅ 4. K线数据准备完成 ({} 根)", kline_count);

    // ========================================
    // 3. 创建 TickGenerator
    // ========================================
    let tick_gen = StreamTickGenerator::from_loader(config.symbol.clone(), klines.into_iter());
    let total_ticks = kline_count * 60;
    tracing::info!("✅ 5. TickGenerator 创建成功 (预计 {} ticks)", total_ticks);

    // ========================================
    // 4. 创建 TradeManager 引擎
    // ========================================
    let engine = Arc::new(TradeManagerEngine::new(
        Arc::clone(&data_feeder),
        Arc::clone(&gateway),
        risk_checker,
    ));
    tracing::info!("✅ 6. TradeManager 引擎创建成功");

    // ========================================
    // 5. 创建 Tick 通道
    // ========================================
    let (tx, mut rx) = mpsc::channel::<(u64, Tick)>(1000);

    // TickGenerator 运行在独立任务
    let tick_gen_handle = tokio::spawn({
        let mut generator = tick_gen;
        let gateway_for_tick = Arc::clone(&gateway);
        
        async move {
            let mut tick_idx = 0u64;
            
            while let Some(tick_data) = generator.next() {
                // 更新网关价格
                gateway_for_tick.update_price(&tick_data.symbol, tick_data.price);
                
                // 构建 KLine
                let kline_1m = KLine {
                    symbol: tick_data.symbol.clone(),
                    period: Period::Minute(1),
                    open: tick_data.open,
                    high: tick_data.high,
                    low: tick_data.low,
                    close: tick_data.price,
                    volume: tick_data.volume,
                    timestamp: tick_data.kline_timestamp,
                };

                // 构建 Tick
                let tick = Tick {
                    symbol: tick_data.symbol,
                    price: tick_data.price,
                    qty: tick_data.qty,
                    timestamp: tick_data.timestamp,
                    kline_1m: Some(kline_1m),
                    kline_15m: None,
                    kline_1d: None,
                };

                tick_idx += 1;

                // 发送 Tick
                if tx.send((tick_idx, tick)).await.is_err() {
                    tracing::warn!("Tick channel closed");
                    break;
                }
            }
            
            tracing::info!("TickGenerator 完成，共 {} ticks", tick_idx);
        }
    });

    // ========================================
    // 6. 创建策略监控任务
    // ========================================
    let strategy_handle = tokio::spawn({
        let config = config.clone();
        let engine = Arc::clone(&engine);
        
        async move {
            let mut last_signal_price = Decimal::ZERO;
            let signal_interval = 10u64; // 每10个tick检测一次
            let max_orders = 10u32;
            let min_tick_gap = 50u64;
            let mut last_order_tick = 0u64;
            let mut order_count = 0u32;

            loop {
                // 等待下一个 tick
                let (tick_idx, tick) = match rx.recv().await {
                    Some(t) => t,
                    None => {
                        tracing::info!("Tick 流结束，策略退出");
                        break;
                    }
                };

                // 更新 DataFeeder
                engine.data_feeder.push_tick(tick.clone());

                // 更新引擎的最新价格
                engine.update_price(&config.symbol, tick.price);

                // 策略信号检测
                if tick_idx % signal_interval == 0 {
                    // 计算价格变化
                    let price_change = if last_signal_price.is_zero() {
                        Decimal::ZERO
                    } else {
                        ((tick.price - last_signal_price) / last_signal_price).abs()
                    };

                    // 触发条件：价格变化 > 0.1% 且未超过最大订单数 且间隔足够
                    if price_change > dec!(0.001) 
                        && order_count < max_orders 
                        && tick_idx - last_order_tick >= min_tick_gap 
                    {
                        // 检查任务是否已存在
                        if !engine.has_task_sync(&config.symbol) {
                            // 创建新任务
                            engine.spawn_strategy_task(
                                config.symbol.clone(),
                                tick.price,
                                50, // 50ms 间隔
                            ).await;
                            
                            order_count += 1;
                            last_order_tick = tick_idx;
                            tracing::info!(
                                "[Tick {:04}] 📝 触发策略 @ {} (change: {:.2}%)",
                                tick_idx,
                                tick.price,
                                price_change * dec!(100)
                            );
                        }
                    }

                    last_signal_price = tick.price;
                }

                // 检查是否达到最大 tick 数
                if config.fast_mode && tick_idx >= total_ticks as u64 {
                    tracing::info!("达到最大 tick 数，策略退出");
                    break;
                }
            }
        }
    });

    // ========================================
    // 7. 引擎主循环
    // ========================================
    let engine_handle = tokio::spawn({
        let config = config.clone();
        let engine = Arc::clone(&engine);
        
        async move {
            tracing::info!("引擎主循环启动");
            
            loop {
                // 1. 检查任务状态
                engine.check_tasks().await;
                
                // 2. 检查心跳
                engine.check_heartbeat().await;
                
                // 3. 检查是否全部任务结束
                if engine.is_empty().await {
                    tracing::info!("所有任务已结束");
                    break;
                }
                
                // 4. 打印状态
                let count = engine.task_count().await;
                if count > 0 {
                    tracing::debug!("引擎状态: {} 个任务运行中", count);
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
        _ = tick_gen_handle => {
            tracing::info!("TickGenerator 已结束");
        }
        _ = strategy_handle => {
            tracing::info!("策略任务已结束");
        }
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

    // 测试 DataFeeder
    tracing::info!("\n========================================");
    tracing::info!("  DataFeeder 查询测试");
    tracing::info!("========================================");
    if let Some(kline) = data_feeder.ws_get_1m(&config.symbol) {
        tracing::info!("✅ DataFeeder: O={} H={} L={} C={}",
            kline.open, kline.high, kline.low, kline.close);
    }

    tracing::info!("\n========================================");
    tracing::info!("  沙盒模式测试完成");
    tracing::info!("========================================");

    Ok(())
}

// ============================================================================
// TradeManager 引擎
// ============================================================================

/// 任务 Map 类型
type TaskMap = std::collections::HashMap<String, Arc<TokioRwLock<TaskState>>>;
type PriceMap = std::collections::HashMap<String, Decimal>;

/// TradeManager 引擎 - 精简版
/// 
/// 引擎层职责：
/// - 任务注册表
/// - 心跳检查
/// - 任务移除
/// - 持久化（这里只打印日志）
/// 
/// 策略层职责：
/// - 策略计算
/// - 风控检查
/// - 下单执行
/// - 平仓完成 → 自己退出
struct TradeManagerEngine {
    /// DataFeeder
    data_feeder: Arc<DataFeeder>,
    /// 网关
    gateway: Arc<ShadowBinanceGateway>,
    /// 风控
    risk_checker: Arc<ShadowRiskChecker>,
    
    /// 任务注册表
    tasks: Arc<TokioRwLock<TaskMap>>,
    /// 最新价格
    prices: Arc<TokioRwLock<PriceMap>>,
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
        gateway: Arc<ShadowBinanceGateway>,
        risk_checker: Arc<ShadowRiskChecker>,
    ) -> Self {
        Self {
            data_feeder,
            gateway,
            risk_checker,
            tasks: Arc::new(TokioRwLock::new(std::collections::HashMap::new())),
            prices: Arc::new(TokioRwLock::new(std::collections::HashMap::new())),
            heartbeat_timeout: 90,
            global_lock: Arc::new(tokio::sync::Mutex::new(())),
            stats: Arc::new(TokioRwLock::new(EngineStats::default())),
        }
    }

    /// 更新价格
    fn update_price(&self, symbol: &str, price: Decimal) {
        let symbol = symbol.to_string();
        let prices = Arc::clone(&self.prices);
        tokio::spawn(async move {
            prices.write().await.insert(symbol, price);
        });
    }

    /// 同步检查任务是否存在
    fn has_task_sync(&self, _symbol: &str) -> bool {
        // 简化：允许重复创建
        // 实际应该检查任务表
        false
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
    async fn spawn_strategy_task(&self, symbol: String, current_price: Decimal, interval_ms: u64) {
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
        let gateway = Arc::clone(&self.gateway);
        let risk_checker = Arc::clone(&self.risk_checker);
        let prices = Arc::clone(&self.prices);
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

                // 3. 获取当前价格
                let current_price = {
                    let prices = prices.read().await;
                    prices.get(&symbol).copied().unwrap_or(current_price)
                };

                // 4. 策略计算
                let should_open = !has_position && signal_count % 20 == 10;
                let should_close = has_position && signal_count % 30 == 20;

                // 5. 执行交易
                if should_open {
                    // 风控检查
                    let account = match gateway.get_account() {
                        Ok(acc) => acc,
                        Err(_) => {
                            let mut s = stats.write().await;
                            s.total_errors += 1;
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

                // 6. 更新心跳
                state.write().await.heartbeat();

                // 7. 释放锁
                drop(_lock);

                // 8. 信号计数
                signal_count += 1;

                // 9. 等待下一个周期
                sleep(Duration::from_millis(interval_ms)).await;
            }

            // 10. 从注册表移除
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
    use b_data_source::Period;

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
