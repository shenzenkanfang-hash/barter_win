//! 系统组件创建

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig};
use b_data_mock::{
    api::{MockApiGateway, MockConfig},
    replay_source::ReplaySource,
    store::MarketDataStoreImpl,
    ws::kline_1m::ws::Kline1mStream,
};
use b_data_source::store::PipelineStore;
use c_data_process::processor::SignalProcessor;
use d_checktable::h_15m::{
    Executor, ExecutorConfig, Repository, ThresholdConfig, Trader, TraderConfig,
};
use e_risk_monitor::risk::common::{OrderCheck, RiskPreChecker};
use e_risk_monitor::trade_lock::TradeLock;
use rust_decimal::Decimal;

use crate::tick_context::{DB_PATH, DATA_FILE, INITIAL_BALANCE, SYMBOL};
use crate::utils::{convert_store_indicator_to_market_indicators, StoreRef};

/// SystemComponents - Send-safe 系统组件（排除 Kline1mStream）
///
/// Kline1mStream 含非 Send RNG（ThreadRng），单独提取为 DataLayer。
#[derive(Clone)]
pub struct SystemComponents {
    pub signal_processor: Arc<SignalProcessor>,
    pub trader: Arc<Trader>,
    pub risk_checker: Arc<RiskPreChecker>,
    pub order_checker: Arc<OrderCheck>,
    pub gateway: Arc<MockApiGateway>,
    #[allow(dead_code)]
    pub pipeline_store: Arc<PipelineStore>,
    pub trade_lock: Arc<TradeLock>,
}

/// DataLayer - Kline1mStream 数据层（非 Send，驱动专有）
///
/// Kline1mStream 含 rand::ThreadRng（Rc<UnsafeCell<...>>），非 Send。
/// 仅在驱动协程内使用，不跨越 await。
#[derive(Clone)]
pub struct DataLayer {
    pub kline_stream: Arc<tokio::sync::Mutex<Kline1mStream>>,
}

pub fn init_heartbeat() {
    hb::init(HbConfig {
        stale_threshold: 3,
        report_interval_secs: 300,
        max_file_age_hours: 24,
        max_file_size_mb: 100,
    });
    tracing::info!("Heartbeat monitor ready");
}

/// 创建所有系统组件（返回 Send-safe SystemComponents + 非 Send DataLayer）
///
/// # 返回
/// - `Ok((SystemComponents, DataLayer))` - Send-safe 组件 + 数据层
pub async fn create_components() -> Result<(SystemComponents, DataLayer), Box<dyn std::error::Error>> {
    tracing::info!("Loading: {}", DATA_FILE);
    let replay_source = ReplaySource::from_csv(DATA_FILE).await?;
    tracing::info!("[b] Loaded {} K-lines", replay_source.len());

    let store: Arc<MarketDataStoreImpl> = Arc::new(MarketDataStoreImpl::new());

    if !replay_source.is_empty() {
        let store_klines = replay_source.to_store_klines();
        store.preload_klines(SYMBOL, store_klines.clone());
        tracing::info!("[b] Preloaded {} klines into store history", store_klines.len());
    }

    let shared_store: StoreRef = store;

    let pipeline_store = Arc::new(PipelineStore::new());
    tracing::info!("[pipeline] PipelineStore created");

    let kline_stream = Arc::new(tokio::sync::Mutex::new(Kline1mStream::from_klines_with_pipeline(
        SYMBOL.to_string(),
        Box::new(replay_source),
        shared_store.clone(),
        pipeline_store.clone(),
    )));
    tracing::info!("[b] KlineStream created with pipeline_store");

    let gateway = Arc::new(MockApiGateway::new(INITIAL_BALANCE, MockConfig::default()));
    tracing::info!("[f] MockGateway created, balance={}", INITIAL_BALANCE);

    let signal_processor = Arc::new(SignalProcessor::with_pipeline(pipeline_store.clone()));
    signal_processor.set_market_store(shared_store.clone());
    signal_processor.register_symbol(SYMBOL);
    tracing::info!("[c] SignalProcessor created with pipeline_store + market_store");

    let trader = create_trader(shared_store.clone(), pipeline_store.clone())?;
    tracing::info!("[d] Trader created");

    let mut risk_checker = RiskPreChecker::new(Decimal::try_from(0.15).unwrap(), Decimal::try_from(100.0).unwrap());
    risk_checker.register_symbol(SYMBOL.to_string());
    let risk_checker = Arc::new(risk_checker);

    let order_checker = Arc::new(OrderCheck::new());
    tracing::info!("[e] RiskChecker + OrderCheck created");

    let trade_lock = Arc::new(TradeLock::new());
    tracing::info!("[e] TradeLock created");

    let components = SystemComponents {
        signal_processor,
        trader,
        risk_checker,
        order_checker,
        gateway,
        pipeline_store,
        trade_lock,
    };

    let data_layer = DataLayer { kline_stream };

    Ok((components, data_layer))
}

fn create_trader(
    store: StoreRef,
    pipeline_store: Arc<PipelineStore>,
) -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
    let config = TraderConfig {
        symbol: SYMBOL.to_string(),
        interval_ms: 100,
        max_position: Decimal::try_from(0.15).unwrap(),
        initial_ratio: Decimal::try_from(0.05).unwrap(),
        db_path: DB_PATH.to_string(),
        order_interval_ms: 100,
        lot_size: Decimal::try_from(0.001).unwrap(),
        thresholds: ThresholdConfig::default(),
    };

    let executor_config = ExecutorConfig {
        symbol: SYMBOL.to_string(),
        order_interval_ms: config.order_interval_ms,
        initial_ratio: config.initial_ratio,
        lot_size: config.lot_size,
        max_position: config.max_position,
    };
    let executor = Arc::new(Executor::new(executor_config));

    let repository = Arc::new(Repository::new(SYMBOL, DB_PATH)?);

    Ok(Arc::new(Trader::new_with_pipeline(
        config,
        executor,
        repository,
        store.clone(),
        pipeline_store,
    )
    .with_indicator_calculator(Box::new(move |symbol: String| {
        let store = store.clone();
        Box::pin(async move { convert_store_indicator_to_market_indicators(&store, &symbol) })
    }))))
}

/// 打印心跳报告
pub async fn print_heartbeat_report() {
    tracing::info!("==============================================");
    tracing::info!("HEARTBEAT REPORT (进程存活监控)");
    tracing::info!("==============================================");

    let summary = hb::global().summary().await;
    tracing::info!(
        "Total: {}, Active: {}, Reports: {}",
        summary.total_points,
        summary.active_count,
        summary.reports_count
    );

    if let Err(e) = hb::global().save_report("heartbeat_report.json").await {
        tracing::warn!("Save failed: {}", e);
    }
}
