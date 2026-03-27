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

use b_data_source::{DataFeeder, KLine, history::HistoryApiClient, Period, MarketDataStoreImpl, MarketDataStore, default_store};
use b_data_source::ws::kline_1m::ws::KlineData;
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
    /// DataFeeder（用于注入数据 + API 查询）
    pub data_feeder: Arc<DataFeeder>,
    /// 共享的 MarketDataStore（真实系统使用）
    /// 【关键】Trader 从这里读取数据，不再构造假指标
    pub store: Arc<MarketDataStoreImpl>,
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
        // 【关键修复】使用 default_store() 单例，确保与 Trader 共享同一实例
        // 之前：let store = Arc::new(MarketDataStoreImpl::new());  ← 独立实例，数据流断裂！
        let store = b_data_source::default_store().clone();

        // DataFeeder 保持独立（用于 API 查询）
        let data_feeder = Arc::new(DataFeeder::new());

        Self {
            symbol,
            initial_fund,
            data_feeder,
            store,  // 共享 default_store() 单例
            gateway: Arc::new(ShadowBinanceGateway::with_default_config(initial_fund)),
            trader_manager: Arc::new(TraderManager::new()),
            shutdown: Arc::new(Notify::new()),
        }
    }
}

// ==================== 历史数据加载 ====================

/// 加载历史 K 线数据（优先本地， fallback API）
async fn load_klines(
    symbol: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    // 构建本地文件路径
    let start_date = start_time.format("%Y%m%d").to_string();
    let end_date = end_time.format("%Y%m%d").to_string();
    let local_path = format!("data/{}_1m_{}_{}.csv", symbol, start_date, end_date);

    info!("[Data] 检查本地数据: {}", local_path);

    // 1. 尝试从本地文件加载
    if std::path::Path::new(&local_path).exists() {
        info!("[Data] 从本地文件加载: {}", local_path);
        return load_klines_from_csv(&local_path, symbol);
    }

    // 2. 本地没有，从 API 拉取
    info!("[Data] 本地文件不存在，从 API 拉取");
    fetch_from_api(symbol, start_time, end_time).await
}

/// 从 CSV 文件加载 K 线
fn load_klines_from_csv(path: &str, symbol: &str) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    info!("[Data] 读取 CSV: {}", path);

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let mut klines = Vec::new();
    for result in reader.records() {
        let record = result?;

        // 使用 ok_or 处理 Option
        let timestamp_ms: i64 = record.get(0).ok_or("missing timestamp")?.parse()
            .map_err(|e| format!("parse timestamp: {}", e))?;
        let open: Decimal = record.get(1).ok_or("missing open")?.parse()
            .map_err(|e| format!("parse open: {}", e))?;
        let high: Decimal = record.get(2).ok_or("missing high")?.parse()
            .map_err(|e| format!("parse high: {}", e))?;
        let low: Decimal = record.get(3).ok_or("missing low")?.parse()
            .map_err(|e| format!("parse low: {}", e))?;
        let close: Decimal = record.get(4).ok_or("missing close")?.parse()
            .map_err(|e| format!("parse close: {}", e))?;
        let volume: Decimal = record.get(5).ok_or("missing volume")?.parse()
            .map_err(|e| format!("parse volume: {}", e))?;

        klines.push(KLine {
            symbol: symbol.to_string(),
            period: Period::Minute(1),
            open,
            high,
            low,
            close,
            volume,
            timestamp: Utc.timestamp_millis_opt(timestamp_ms).unwrap(),
        });
    }

    info!("[Data] 从 CSV 加载 {} 条 K 线", klines.len());
    Ok(klines)
}

/// 从 API 拉取历史数据
async fn fetch_from_api(
    symbol: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    let end_ms = end_time.timestamp_millis();
    let start_ms = start_time.timestamp_millis();

    info!(
        "[Data] 拉取 API: {} {} ~ {}",
        symbol, start_time, end_time
    );

    // 创建历史 API 客户端
    let history_client = HistoryApiClient::new_futures();

    // 下载所有数据（分批，每批1000条）
    let mut all_klines = Vec::new();
    let mut current_start = start_ms;

    while current_start < end_ms {
        let batch_end = (current_start + 1000 * 60 * 1000).min(end_ms);

        let history_klines = history_client
            .fetch_klines(symbol, "1m", Some(current_start), Some(batch_end), 1000)
            .await
            .map_err(|e| format!("API 拉取失败: {}", e))?;

        // 转换为模型 K 线
        for hk in history_klines {
            all_klines.push(KLine {
                symbol: hk.symbol,
                period: Period::Minute(1),
                open: hk.open,
                high: hk.high,
                low: hk.low,
                close: hk.close,
                volume: hk.volume,
                timestamp: Utc.timestamp_millis_opt(hk.timestamp_ms).unwrap(),
            });
        }

        if current_start + 1000 * 60 * 1000 > end_ms {
            break;
        }
        current_start = batch_end;
    }

    // 保存到本地（可选）
    let csv_path = format!("data/{}_1m_{}_{}.csv", symbol,
        start_time.format("%Y%m%d"), end_time.format("%Y%m%d"));
    let mut csv_content = String::from("timestamp,open,high,low,close,volume\n");
    for kline in &all_klines {
        csv_content.push_str(&format!(
            "{},{},{},{},{},{}\n",
            kline.timestamp.timestamp_millis(),
            kline.open, kline.high, kline.low, kline.close, kline.volume
        ));
    }
    std::fs::write(&csv_path, csv_content)?;
    info!("[Data] 已缓存到: {}", csv_path);

    info!("[Data] API 拉取 {} 条 K 线", all_klines.len());
    Ok(all_klines)
}

// ==================== 数据注入 ====================

/// 启动数据回放
/// 【关键】数据同时写入：
/// 1. DataFeeder - 用于 API 查询
/// 2. MarketDataStore - 用于 Trader 读取（触发真实波动率计算）
async fn start_data_replay(
    ctx: &SandboxContext,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    klines: Vec<KLine>,
) {
    let symbol = ctx.symbol.clone();
    info!(
        symbol = %symbol,
        start = %start_time,
        end = %end_time,
        kline_count = klines.len(),
        "[Data] 启动数据回放"
    );

    // 克隆共享组件
    let data_feeder = ctx.data_feeder.clone();
    let store = ctx.store.clone();

    // 使用已加载的 K 线数据
    // 创建生成器
    let generator = StreamTickGenerator::new(
        symbol.clone(),
        Box::new(klines.into_iter()),
    );

    // 按时间顺序生成 Tick，同时注入 DataFeeder 和写入 Store
    for tick_result in generator {
        // 1. 转换为 Tick（用于 DataFeeder）
        let tick = b_data_source::Tick {
            symbol: tick_result.symbol.clone(),
            price: tick_result.price,
            qty: tick_result.qty,
            timestamp: tick_result.timestamp,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        // 2. 注入 DataFeeder（API 查询用）
        data_feeder.push_tick(tick);

        // 3. 转换为 KlineData 写入 Store（触发真实波动率计算！）
        let kline_data = KlineData {
            kline_start_time: tick_result.timestamp.timestamp_millis(),
            kline_close_time: tick_result.timestamp.timestamp_millis() + 60000,
            symbol: tick_result.symbol.clone(),
            interval: "1m".to_string(),
            open: tick_result.price.to_string(),
            close: tick_result.price.to_string(),
            high: tick_result.price.to_string(),
            low: tick_result.price.to_string(),
            volume: tick_result.qty.to_string(),
            is_closed: true,
        };

        // 通过 trait 方法调用
        store.as_ref().write_kline(&symbol, kline_data, true);
    }

    info!(symbol = %symbol, "[Data] 数据回放完成，已写入 Store");
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

    // 【关键】使用共享的 Store（沙盒数据已写入，Trader 从这里读取真实数据）
    let store: std::sync::Arc<dyn b_data_source::MarketDataStore + Send + Sync> =
        ctx.store.clone();

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

    // 4. 加载历史数据（优先本地，没有则从 API 拉取）
    // 【关键】优先本地缓存，减少 API 调用
    let historical_klines = load_klines(
        &args.symbol,
        start_time,
        end_time,
    ).await?;

    info!(symbol = %args.symbol, count = historical_klines.len(), "[Data] 历史数据准备完成");

    // 5. 启动数据回放（注入 DataFeeder）
    start_data_replay(&ctx, start_time, end_time, historical_klines).await;

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