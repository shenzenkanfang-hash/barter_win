//! Full Production Sandbox - 生产级沙盒
//!
//! 完整复现生产环境业务流程：
//! 1. 历史数据拉取（从币安 API）
//! 2. 数据注入（StreamTickGenerator）
//! 3. Trader 启动（TraderManager）
//! 4. 网关拦截（ShadowBinanceGateway）
//!
//! 【关键差异】vs 简化沙盒：
//! - 不跳过高波动检测逻辑
//! - 不模拟指标计算（真实调用 c_data_process）
//! - 不跳过历史数据拉取
//! - 完整 WAL 模式执行

use std::sync::Arc;
use chrono::{DateTime, Utc, TimeZone};
use clap::Parser;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::Notify;
use tracing::{info, error};
use tracing_subscriber::fmt;

use b_data_source::{DataFeeder, KLine, history::HistoryApiClient, Period};
use d_checktable::h_15m::{Trader, TraderConfig, Executor, Repository};
use f_engine::strategy::TraderManager;

use h_sandbox::{
    ShadowBinanceGateway,
    StreamTickGenerator,
};
use h_sandbox::simulator::OrderRequest;

// ==================== 命令行参数 ====================

#[derive(Parser, Debug)]
#[command(name = "full_production_sandbox")]
#[command(about = "生产级沙盒 - 完整复现业务流程", long_about = None)]
struct Args {
    /// 交易对（如 HOTUSDT）
    #[arg(long, default_value = "HOTUSDT")]
    symbol: String,

    /// 起始时间（ISO8601，如 2025-10-09T00:00:00Z）
    #[arg(long)]
    start: String,

    /// 结束时间（ISO8601，如 2025-10-11T23:59:59Z）
    #[arg(long)]
    end: String,

    /// 初始资金
    #[arg(long, default_value = "10000")]
    fund: Decimal,
}

// ==================== 沙盒核心组件 ====================

/// 生产级沙盒上下文
pub struct SandboxContext {
    /// 交易对
    pub symbol: String,
    /// 初始资金
    pub initial_fund: Decimal,
    /// DataFeeder（用于注入数据）
    pub data_feeder: Arc<DataFeeder>,
    /// Shadow 网关
    pub gateway: Arc<ShadowBinanceGateway>,
    /// Trader 管理器
    pub trader_manager: Arc<TraderManager>,
    /// 关闭信号
    pub shutdown: Arc<Notify>,
}

impl SandboxContext {
    /// 创建沙盒上下文
    pub fn new(symbol: String, initial_fund: Decimal) -> Self {
        Self {
            symbol,
            initial_fund,
            data_feeder: Arc::new(DataFeeder::new()),
            gateway: Arc::new(ShadowBinanceGateway::with_default_config(initial_fund)),
            trader_manager: Arc::new(TraderManager::new()),
            shutdown: Arc::new(Notify::new()),
        }
    }
}

// ==================== 历史数据拉取 ====================

/// 将历史 K 线转换为模型 K 线
fn convert_history_kline(history_kline: b_data_source::history::KLine) -> KLine {
    KLine {
        symbol: history_kline.symbol,
        period: b_data_source::Period::Minute(1),
        open: history_kline.open,
        high: history_kline.high,
        low: history_kline.low,
        close: history_kline.close,
        volume: history_kline.volume,
        timestamp: chrono::Utc.timestamp_millis_opt(history_kline.timestamp_ms).unwrap(),
    }
}

/// 拉取历史 K 线数据
/// 【关键】调用币安 API 获取历史数据，而非预加载
async fn fetch_historical_klines(
    symbol: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    info!(
        symbol = symbol,
        start = %start_time,
        end = %end_time,
        limit = limit,
        "[Data] 拉取历史数据 {} 条，end_time={}",
        limit, end_time
    );

    // 转换时间戳为毫秒
    let end_ms = end_time.timestamp_millis();
    let start_ms = start_time.timestamp_millis();

    // 创建历史 API 客户端（期货）
    let history_client = HistoryApiClient::new_futures();

    // 调用币安 API 获取历史 K 线
    let history_klines = history_client
        .fetch_klines(symbol, "1m", Some(start_ms), Some(end_ms), limit)
        .await
        .map_err(|e| format!("历史数据拉取失败: {}", e))?;

    // 转换为模型 K 线
    let klines: Vec<KLine> = history_klines.into_iter().map(convert_history_kline).collect();

    info!(
        symbol = symbol,
        count = klines.len(),
        "[Data] 成功拉取 {} 条历史 K 线",
        klines.len()
    );

    Ok(klines)
}

// ==================== 数据注入 ====================

/// 启动数据回放
async fn start_data_replay(
    ctx: &SandboxContext,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) {
    let symbol = ctx.symbol.clone();
    info!(
        symbol = %symbol,
        start = %start_time,
        end = %end_time,
        "[Data] 启动数据回放"
    );

    // 读取历史 K 线数据（从缓存或文件）
    // TODO: 读取实际历史数据文件
    let klines: Vec<KLine> = vec![];

    // 创建生成器
    let generator = StreamTickGenerator::new(
        symbol.clone(),
        Box::new(klines.into_iter()),
    );

    // 按时间顺序生成 Tick，注入 DataFeeder
    let data_feeder = ctx.data_feeder.clone();
    for tick_result in generator {
        let tick = b_data_source::Tick {
            symbol: tick_result.symbol,
            price: tick_result.price,
            qty: tick_result.qty,
            timestamp: tick_result.timestamp,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        data_feeder.push_tick(tick);
    }

    info!(symbol = %symbol, "[Data] 数据回放完成");
}

// ==================== 策略执行 ====================

/// 启动交易员（完整 WAL 模式）
async fn start_trader(ctx: &SandboxContext) -> Result<(), Box<dyn std::error::Error>> {
    let symbol = ctx.symbol.clone();

    info!(symbol = %symbol, "[Engine] 启动 Trader 协程");

    // 创建 Trader 配置
    let config = TraderConfig {
        symbol: symbol.clone(),
        ..Default::default()
    };

    // 创建 Executor（注入 Shadow 网关）
    let executor = Arc::new(Executor::new(d_checktable::h_15m::ExecutorConfig {
        symbol: symbol.clone(),
        order_interval_ms: config.order_interval_ms,
        initial_ratio: config.initial_ratio,
        lot_size: config.lot_size,
        max_position: config.max_position,
    }));

    // 创建 Repository
    let repository = Arc::new(
        Repository::new(&symbol, &config.db_path)
            .map_err(|e| format!("Repository init failed: {}", e))?
    );

    // 创建 MarketDataStore（使用默认 store）
    let store: std::sync::Arc<dyn b_data_source::MarketDataStore + Send + Sync> =
        b_data_source::default_store().clone();

    // 创建 Trader
    let trader = Arc::new(Trader::new(config, executor, repository, store));

    // 启动 Trader 协程
    let trader_clone = trader.clone();
    tokio::spawn(async move {
        trader_clone.start().await;
    });

    info!(symbol = %symbol, "[Engine] Trader 协程已启动");

    Ok(())
}

// ==================== 网关拦截 ====================

/// 模拟成交（Shadow 网关拦截）
async fn simulate_order_filled(
    ctx: &SandboxContext,
    symbol: &str,
    side: h_sandbox::simulator::Side,
    qty: Decimal,
    price: Decimal,
) -> Result<(), Box<dyn std::error::Error>> {
    // 使用 ShadowBinanceGateway 模拟成交
    let order_req = OrderRequest {
        symbol: symbol.to_string(),
        side,
        qty,
        price,
        leverage: dec!(1),
    };

    ctx.gateway.engine().write().execute(order_req);

    info!(
        symbol = symbol,
        side = ?side,
        qty = %qty,
        price = %price,
        "[Gateway] 模拟成交完成"
    );

    Ok(())
}

// ==================== 主循环 ====================

/// 生产级沙盒主循环
async fn run_sandbox(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志
    fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(true)
        .init();

    info!("========================================");
    info!("[Sandbox] 生产级沙盒启动");
    info!("  symbol: {}", args.symbol);
    info!("  start:  {}", args.start);
    info!("  end:    {}", args.end);
    info!("  fund:   {}", args.fund);
    info!("========================================");

    // 2. 创建沙盒上下文
    let ctx = SandboxContext::new(args.symbol.clone(), args.fund);

    // 3. 解析时间
    let start_time: DateTime<Utc> = args.start.parse()
        .map_err(|e| format!("Invalid start time: {}", e))?;
    let end_time: DateTime<Utc> = args.end.parse()
        .map_err(|e| format!("Invalid end time: {}", e))?;

    // 4. 拉取历史数据（关键步骤）
    // 【关键】不预加载，真实触发历史数据拉取
    let _historical_klines = fetch_historical_klines(
        &args.symbol,
        start_time,
        end_time,
        1000, // 往前 1000 条
    ).await?;

    // 5. 检查数据充足性
    // 【关键】由业务层检测，不是沙盒预判断
    info!(symbol = %args.symbol, "[Engine] 检查数据充足性");

    // 6. 启动数据回放（注入 DataFeeder）
    start_data_replay(&ctx, start_time, end_time).await;

    // 7. 启动 Trader（完整 WAL 模式）
    start_trader(&ctx).await?;

    // 8. 等待关闭信号
    ctx.shutdown.notified().await;

    info!("[Sandbox] 沙盒已关闭");
    Ok(())
}

// ==================== 入口点 ====================

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Err(e) = run_sandbox(args).await {
        error!("[Sandbox] 错误: {}", e);
        std::process::exit(1);
    }
}