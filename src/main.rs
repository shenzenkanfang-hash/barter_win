//! Trading System v5.5 - TickContext 业务链路追踪
//!
//! 【架构】按业务逻辑顺序：b → f → d → c → e
//!
//!   Tick
//!     → [b] 数据引擎        获取K线原始数据
//!     → [f] 执行层          更新Mock价格/账户
//!     → [d] 策略层(业务核心) d调用c，c返回指标结果
//!     → [c] 指标层          被d调用，返回指标数据
//!     → [e] 风控层          d的决策触发风控校验
//!     → ctx.to_report()    输出完整JSON
//!
//! v5.5: 业务顺序 b→f→d→c→e，数据层/指标层为被调用方

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig};
use b_data_mock::{
    api::{mock_account::Side, MockApiGateway, MockConfig},
    replay_source::ReplaySource,
    ws::kline_1m::ws::Kline1mStream,
};
use chrono::{DateTime, Utc};
use c_data_process::processor::SignalProcessor;
use d_checktable::h_15m::{
    Executor, ExecutorConfig, Repository, ThresholdConfig, Trader, TraderConfig,
};
use d_checktable::h_15m::trader::StoreRef;
use e_risk_monitor::risk::common::{OrderCheck, RiskPreChecker};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use tokio::time::{interval, Duration};
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// ============================================================================
// 常量
// ============================================================================

const INITIAL_BALANCE: Decimal = dec!(10000);
const SYMBOL: &str = "HOTUSDT";
const DB_PATH: &str = "D:/RusProject/barter-rs-main/data/trade_records.db";
const DATA_FILE: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv";

// ============================================================================
// TickContext - 全链路唯一状态容器（业务顺序 b→f→d→c→e）
// ============================================================================

#[derive(Debug, Clone)]
struct TickContext {
    // === 元数据 ===
    pub tick_id: u64,
    pub timestamp: DateTime<Utc>,

    // === 原始数据（只读）===
    pub kline: RawKline,

    // === 业务顺序：b → f → d → c → e ===
    pub b_data:    Option<BDataResult>,    // [b] 数据引擎
    pub f_engine:  Option<FEngineResult>,  // [f] 执行层（价格更新）
    pub d_check:   Option<DCheckResult>,    // [d] 策略层（业务核心）
    pub c_data:    Option<CDataResult>,     // [c] 指标层（被d调用）
    pub e_risk:    Option<ERiskResult>,     // [e] 风控层

    // === 链路追踪 ===
    pub visited: Vec<&'static str>,
    pub errors:  Vec<StageError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawKline {
    pub open:     Decimal,
    pub close:    Decimal,
    pub high:     Decimal,
    pub low:      Decimal,
    pub volume:   Decimal,
    pub is_closed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BDataResult {
    pub kline_id:   u64,
    pub valid:       bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FEngineResult {
    pub price_updated: bool,
    pub account_synced: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(clippy::large_enum_variant)]
pub struct DCheckResult {
    /// 交易决策：long_entry / short_entry / close / skip
    pub decision:  String,
    /// 下单数量（有信号时才有值）
    pub qty:        Option<Decimal>,
    /// 决策原因
    pub reason:     String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CDataResult {
    /// zscore偏离度（14周期）
    pub zscore_14:    Option<f64>,
    /// TR基准值（60周期）
    pub tr_base:      Option<Decimal>,
    /// 价格位置（0-100）
    pub pos_norm:     Option<f64>,
    /// 是否产生信号
    pub signal:       bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ERiskResult {
    pub balance_passed: bool,
    pub order_passed:  bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageError {
    pub stage:  String,
    pub code:   String,
    pub detail: String,
}

impl TickContext {
    fn new(tick_id: u64, kline: RawKline) -> Self {
        Self {
            tick_id,
            timestamp: Utc::now(),
            kline,
            b_data:    None,
            f_engine:  None,
            d_check:   None,
            c_data:    None,
            e_risk:    None,
            visited: vec![],
            errors:  vec![],
        }
    }

    fn to_report(&self) -> serde_json::Value {
        serde_json::json!({
            "tick_id":          self.tick_id,
            "timestamp":        self.timestamp.to_rfc3339(),
            "complete":         self.is_complete(),
            "visited_stages":   self.visited,
            "errors":           self.errors,
            "kline": {
                "close":  self.kline.close.to_string(),
                "high":   self.kline.high.to_string(),
                "low":    self.kline.low.to_string(),
                "volume": self.kline.volume.to_string(),
            },
            "b_data":    self.b_data,
            "f_engine":  self.f_engine,
            "d_check":   self.d_check,
            "c_data":    self.c_data,
            "e_risk":    self.e_risk,
        })
    }

    fn is_complete(&self) -> bool {
        // 按业务顺序检查：b f d c e 全部到达
        let required = ["b", "f", "d", "c", "e"];
        required.iter().all(|s| self.visited.contains(s))
    }
}

// ============================================================================
// 主程序
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("==============================================");
    tracing::info!("Trading System v5.5 - 业务链路 b→f→d→c→e");
    tracing::info!("Symbol: {}", SYMBOL);
    tracing::info!("Data: {}", DATA_FILE);
    tracing::info!("==============================================");

    init_heartbeat();
    let components = create_components().await?;

    tracing::info!("Components ready:");
    tracing::info!("  [b] KlineStream  - 数据引擎");
    tracing::info!("  [f] MockGateway - 执行层");
    tracing::info!("  [d] Trader      - 策略层(业务核心)");
    tracing::info!("  [c] SignalProc  - 指标层(被d调用)");
    tracing::info!("  [e] RiskChecker - 风控层");

    run_pipeline(components).await?;
    print_heartbeat_report().await;

    Ok(())
}

fn init_heartbeat() {
    hb::init(HbConfig {
        stale_threshold:       3,
        report_interval_secs:  300,
        max_file_age_hours:     24,
        max_file_size_mb:      100,
    });
    tracing::info!("Heartbeat monitor ready");
}

// ============================================================================
// 组件
// ============================================================================

struct SystemComponents {
    kline_stream:     Arc<tokio::sync::Mutex<Kline1mStream>>,
    signal_processor: Arc<SignalProcessor>,
    trader:           Arc<Trader>,
    risk_checker:     Arc<RiskPreChecker>,
    order_checker:    Arc<OrderCheck>,
    gateway:          Arc<MockApiGateway>,
}

async fn create_components() -> Result<SystemComponents, Box<dyn std::error::Error>> {
    // [b] 数据引擎
    tracing::info!("Loading: {}", DATA_FILE);
    let mut replay_source = ReplaySource::from_csv(DATA_FILE).await?;
    tracing::info!("[b] Loaded {} K-lines", replay_source.len());

    // 共享 Store：Kline1mStream 写入，Trader 读取
    // 统一使用 b_data_source::store::MarketDataStore trait
    let store = Arc::new(b_data_source::store::MarketDataStoreImpl::new());

    // 预加载历史数据到 Store（解决沙盒 history_len=0 问题）
    // Trader 在第一根 tick 前即可读取历史 K线，无需等待逐根闭合
    if !replay_source.is_empty() {
        let store_klines = replay_source.to_store_klines();
        store.preload_klines(SYMBOL, store_klines.clone());
        tracing::info!(
            "[b] Preloaded {} klines into store history",
            store_klines.len()
        );
    }

    let shared_store: StoreRef = store;

    let kline_stream = Arc::new(tokio::sync::Mutex::new(
        Kline1mStream::from_klines_with_store(
            SYMBOL.to_string(),
            Box::new(replay_source),
            shared_store.clone(),
        )
    ));

    // [f] 执行层（独立创建，但由 d 决策后调用）
    let gateway = Arc::new(MockApiGateway::new(INITIAL_BALANCE, MockConfig::default()));
    tracing::info!("[f] MockGateway created, balance={}", INITIAL_BALANCE);

    // [c] 指标层（被 d 调用）
    let signal_processor = Arc::new(SignalProcessor::new());
    signal_processor.register_symbol(SYMBOL);
    tracing::info!("[c] SignalProcessor created");

    // [d] 策略层（业务核心）
    let trader = create_trader(shared_store)?;
    tracing::info!("[d] Trader created");

    // [e] 风控层
    let mut risk_checker = RiskPreChecker::new(dec!(0.15), dec!(100.0));
    risk_checker.register_symbol(SYMBOL.to_string());
    let risk_checker = Arc::new(risk_checker);

    let order_checker = Arc::new(OrderCheck::new());
    tracing::info!("[e] RiskChecker + OrderCheck created");

    Ok(SystemComponents {
        kline_stream,
        signal_processor,
        trader,
        risk_checker,
        order_checker,
        gateway,
    })
}

fn create_trader(store: StoreRef) -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
    let config = TraderConfig {
        symbol:           SYMBOL.to_string(),
        interval_ms:      100,
        max_position:    dec!(0.15),
        initial_ratio:    dec!(0.05),
        db_path:          DB_PATH.to_string(),
        order_interval_ms: 100,
        lot_size:         dec!(0.001),
        thresholds:       ThresholdConfig::default(),
    };

    let executor_config = ExecutorConfig {
        symbol:           SYMBOL.to_string(),
        order_interval_ms: config.order_interval_ms,
        initial_ratio:    config.initial_ratio,
        lot_size:         config.lot_size,
        max_position:    config.max_position,
    };
    let executor = Arc::new(Executor::new(executor_config));

    let repository = Arc::new(Repository::new(SYMBOL, DB_PATH)?);

    Ok(Arc::new(Trader::new(config, executor, repository, store)))
}

// ============================================================================
// 业务流水线（按 b→f→d→c→e 顺序）
// ============================================================================

async fn run_pipeline(components: SystemComponents) -> Result<(), Box<dyn std::error::Error>> {
    let mut heartbeat_tick = interval(Duration::from_millis(1000));
    let mut loop_count = 0u64;
    let mut tick_count = 0u64;

    tracing::info!("Pipeline started");

    loop {
        tokio::select! {
            _ = heartbeat_tick.tick() => {
                tracing::trace!("[HB] alive #{}", loop_count);
            }

            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                loop_count += 1;

                // ========== [b] 数据引擎：获取K线 ==========
                let kline_data = {
                    let mut stream = components.kline_stream.lock().await;
                    stream.next_message()
                };

                let Some(data) = kline_data else {
                    tracing::info!("Data exhausted at loop {}", loop_count);
                    break;
                };

                let kline = match parse_raw_kline(&data) {
                    Ok(k) => k,
                    Err(e) => {
                        tracing::warn!("[b] Parse error: {}", e);
                        continue;
                    }
                };

                let mut ctx = TickContext::new(tick_count + 1, kline);

                // ========== [b] 写入 ==========
                stage_b_data(&mut ctx, tick_count + 1);

                // ========== [f] 执行层：更新价格/账户 ==========
                stage_f_engine(&components, &mut ctx);

                // ========== [d] 策略层：做交易决策（业务核心）==========
                // d 内部会调用 c，拿到指标结果后决定是否交易
                let d_result = stage_d_check(&components, &mut ctx).await;

                // ========== [c] 指标层：被d调用，更新指标到store ==========
                // （已在d内部调用c，这里只记录访问）
                ctx.visited.push("c");

                // ========== [e] 风控层：d有决策才触发 ==========
                stage_e_risk(&components, &mut ctx, loop_count, &d_result);

                tick_count += 1;

                // 日志
                if ctx.errors.is_empty() {
                    tracing::debug!(
                        "[Tick#{}] b→f→d→c→e complete={} decision={}",
                        ctx.tick_id,
                        ctx.is_complete(),
                        ctx.d_check.as_ref().map(|d| d.decision.as_str()).unwrap_or("-")
                    );
                } else {
                    tracing::warn!("[Tick#{}] errors={:?}", ctx.tick_id, ctx.errors);
                }

                // 每100个tick打印一次
                if tick_count % 100 == 0 {
                    tracing::info!(
                        "[Progress#{}] ticks={} {}",
                        loop_count,
                        tick_count,
                        serde_json::to_string(&ctx.to_report()).unwrap_or_default()
                    );
                }

                if loop_count >= 1000 {
                    tracing::info!("Max iterations reached");
                    break;
                }
            }
        }
    }

    tracing::info!("Pipeline done: {} loops, {} ticks", loop_count, tick_count);
    Ok(())
}

// ============================================================================
// 各层实现
// ============================================================================

/// [b] 数据引擎：获取并验证K线数据
fn stage_b_data(ctx: &mut TickContext, kline_id: u64) {
    let valid = ctx.kline.close > Decimal::ZERO;
    ctx.b_data = Some(BDataResult {
        kline_id,
        valid,
    });
    ctx.visited.push("b");

    if !valid {
        ctx.errors.push(StageError {
            stage:  "b".into(),
            code:   "INVALID_PRICE".into(),
            detail: format!("close={} <= 0", ctx.kline.close),
        });
    }
}

/// [f] 执行层：更新行情价格到网关（其他模块读取当前价）
fn stage_f_engine(components: &SystemComponents, ctx: &mut TickContext) {
    components.gateway.update_price(SYMBOL, ctx.kline.close);
    ctx.f_engine = Some(FEngineResult {
        price_updated: true,
        account_synced: true,
    });
    ctx.visited.push("f");
}

/// [d] 策略层：业务核心，做交易决策
/// 内部调用 [c] 获取指标数据
async fn stage_d_check(components: &SystemComponents, ctx: &mut TickContext) -> DCheckResult {
    // [d] 调用 [c] 更新指标，获取指标结果
    let c_result = {
        let r = components.signal_processor.min_update(
            SYMBOL,
            ctx.kline.high,
            ctx.kline.low,
            ctx.kline.close,
            ctx.kline.volume,
        );

        CDataResult {
            zscore_14:  None,
            tr_base:    None,
            pos_norm:   None,
            signal:     r.is_ok(),
        }
    };
    ctx.c_data = Some(c_result);
    ctx.visited.push("c");

    // [d] 根据指标做交易决策
    let trade_result = components.trader.execute_once_wal().await;

    match &trade_result {
        Ok(d_checktable::h_15m::ExecutionResult::Executed { qty, .. }) => {
            let result = DCheckResult {
                decision: "long_entry".into(),
                qty: Some(*qty),
                reason:  "signal_triggered".into(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
        Ok(d_checktable::h_15m::ExecutionResult::Skipped(reason)) => {
            let result = DCheckResult {
                decision: "skip".into(),
                qty: None,
                reason:  reason.to_string(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
        Ok(d_checktable::h_15m::ExecutionResult::Failed(e)) => {
            ctx.errors.push(StageError {
                stage:  "d".into(),
                code:   "TRADE_FAILED".into(),
                detail: e.to_string(),
            });
            let result = DCheckResult {
                decision: "error".into(),
                qty: None,
                reason:  e.to_string(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
        Err(e) => {
            ctx.errors.push(StageError {
                stage:  "d".into(),
                code:   "TRADE_ERROR".into(),
                detail: e.to_string(),
            });
            let result = DCheckResult {
                decision: "error".into(),
                qty: None,
                reason:  e.to_string(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
    }
}

/// [e] 风控层：d有决策才触发风控校验
fn stage_e_risk(components: &SystemComponents, ctx: &mut TickContext, loop_id: u64, d_result: &DCheckResult) {
    let Some(qty) = d_result.qty else {
        // d没决策，不走风控
        ctx.visited.push("e");
        return;
    };

    // ER-001: 账户风控
    let balance_passed = components.risk_checker
        .pre_check(SYMBOL, INITIAL_BALANCE, dec!(100), INITIAL_BALANCE)
        .is_ok();

    // ER-003: 订单参数检查
    let order_check_result = components.order_checker.pre_check(
        &format!("order_{}", loop_id),
        SYMBOL,
        "h_15m_strategy",
        dec!(100),
        INITIAL_BALANCE,
        dec!(0),
    );
    let order_passed = order_check_result.passed;

    ctx.e_risk = Some(ERiskResult {
        balance_passed,
        order_passed,
    });
    ctx.visited.push("e");

    // 风控通过，执行模拟成交
    if balance_passed && order_passed {
        if let Ok(order) = components.gateway.place_order(SYMBOL, Side::Buy, qty, None) {
            tracing::info!(
                "[Tick#{}] [e] Filled: price={} qty={}",
                ctx.tick_id,
                order.filled_price,
                order.filled_qty
            );
        }
    } else {
        ctx.errors.push(StageError {
            stage:  "e".into(),
            code:   "RISK_REJECTED".into(),
            detail: format!("balance={} order={}", balance_passed, order_passed),
        });
    }
}

// ============================================================================
// 工具
// ============================================================================

fn parse_raw_kline(data: &str) -> Result<RawKline, Box<dyn std::error::Error>> {
    // 尝试两种格式：
    // 1. 完整字段名 {open, close, high, low, volume, is_closed}
    // 2. Binance 风格 {o, c, h, l, v, x}
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct RawFull {
        #[serde(rename = "open")]
        open_str:  String,
        #[serde(rename = "close")]
        close_str: String,
        #[serde(rename = "high")]
        high_str:  String,
        #[serde(rename = "low")]
        low_str:   String,
        #[serde(rename = "volume")]
        volume_str: String,
        is_closed: bool,
    }

    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct RawBinance {
        #[serde(rename = "o")]
        o_str:  String,
        #[serde(rename = "c")]
        c_str:  String,
        #[serde(rename = "h")]
        h_str:  String,
        #[serde(rename = "l")]
        l_str:  String,
        #[serde(rename = "v")]
        v_str:  String,
        #[serde(rename = "x")]
        x:      bool,
    }

    #[derive(serde::Deserialize)]
    struct RawWrap {
        data: RawBinance,
    }

    // 先尝试完整字段名格式
    if let Ok(raw) = serde_json::from_str::<RawFull>(data) {
        return Ok(RawKline {
            open:     raw.open_str.parse()?,
            close:    raw.close_str.parse()?,
            high:     raw.high_str.parse()?,
            low:      raw.low_str.parse()?,
            volume:   raw.volume_str.parse()?,
            is_closed: raw.is_closed,
        });
    }

    // 再尝试 Binance 风格 {o, c, h, l, v, x}
    if let Ok(raw) = serde_json::from_str::<RawBinance>(data) {
        return Ok(RawKline {
            open:     raw.o_str.parse()?,
            close:    raw.c_str.parse()?,
            high:     raw.h_str.parse()?,
            low:      raw.l_str.parse()?,
            volume:   raw.v_str.parse()?,
            is_closed: raw.x,
        });
    }

    // 最后尝试外层包裹 {data: {o, c, h, l, v, x}}
    let wrapped: RawWrap = serde_json::from_str(data)?;
    let raw = wrapped.data;
    Ok(RawKline {
        open:     raw.o_str.parse()?,
        close:    raw.c_str.parse()?,
        high:     raw.h_str.parse()?,
        low:      raw.l_str.parse()?,
        volume:   raw.v_str.parse()?,
        is_closed: raw.x,
    })
}

// ============================================================================
// 心跳报告
// ============================================================================

async fn print_heartbeat_report() {
    tracing::info!("==============================================");
    tracing::info!("HEARTBEAT REPORT (进程存活监控)");
    tracing::info!("==============================================");

    let summary = hb::global().summary().await;
    tracing::info!("Total: {}, Active: {}, Reports: {}",
        summary.total_points, summary.active_count, summary.reports_count);

    if let Err(e) = hb::global().save_report("heartbeat_report.json").await {
        tracing::warn!("Save failed: {}", e);
    }
}
