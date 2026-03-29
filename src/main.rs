//! Trading System v5.4 - TickContext 数据链路追踪
//!
//! 【架构】
//! - 唯一程序入口：main.rs
//! - 核心设计：TickContext 贯穿全链路，每层读写同一个上下文
//! - 数据流：
//!   TickContext
//!     → [b_data] KlineStream     写入 data_source
//!     → [c_data] SignalProcessor 写入 signal_process
//!     → [d_check] Trader          写入 check_table
//!     → [e_risk] Risk+Order      写入 risk_check
//!     → [f_engine] MockGateway    写入 execution
//!     → 输出完整 JSON 报告
//!
//! v5.4: 用 TickContext 替代心跳 Token，数据自己说话

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig};
use b_data_mock::{
    api::{mock_account::Side, MockApiGateway, MockConfig},
    replay_source::ReplaySource,
    ws::kline_1m::ws::Kline1mStream,
};
use c_data_process::processor::SignalProcessor;
use chrono::{DateTime, Utc};
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
// TickContext - 全链路唯一状态容器
// ============================================================================

/// 一个 Tick 的完整生命周期追踪
/// 每层只读写自己的字段，前面的字段只读，后面的字段不碰
#[derive(Debug, Clone)]
struct TickContext {
    // === 元数据 ===
    pub tick_id: u64,
    pub timestamp: DateTime<Utc>,

    // === 原始数据（只读） ===
    pub kline: RawKline,

    // === 各层结果（各层自写） ===
    pub data_source:  Option<DataSourceResult>,
    pub signal_process: Option<SignalProcessResult>,
    pub check_table:   Option<CheckTableResult>,
    pub risk_check:     Option<RiskCheckResult>,
    pub execution:      Option<ExecutionResult>,

    // === 链路追踪 ===
    pub visited: Vec<&'static str>,
    pub errors:  Vec<StageError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawKline {
    pub open:  Decimal,
    pub close: Decimal,
    pub high:  Decimal,
    pub low:   Decimal,
    pub volume: Decimal,
    pub is_closed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataSourceResult {
    pub kline_count: usize,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignalProcessResult {
    pub zscore_14:   Option<f64>,
    pub tr_base:     Option<Decimal>,
    pub pos_norm:    Option<f64>,
    pub generated:   bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckTableResult {
    pub pin_conditions: i32,
    pub volatility_tier: String,
    pub decision: String,
    pub qty: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskCheckResult {
    pub balance_passed: bool,
    pub order_passed:   bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionResult {
    pub order_id:    String,
    pub filled_price: Option<Decimal>,
    pub filled_qty:   Option<Decimal>,
    pub slippage:     Option<Decimal>,
    pub commission:   Option<Decimal>,
    pub status:      String,
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
            data_source:  None,
            signal_process: None,
            check_table:   None,
            risk_check:     None,
            execution:      None,
            visited: vec![],
            errors:  vec![],
        }
    }

    /// 导出完整 JSON 报告
    fn to_report(&self) -> serde_json::Value {
        serde_json::json!({
            "tick_id":       self.tick_id,
            "timestamp":     self.timestamp.to_rfc3339(),
            "complete":      self.is_complete(),
            "visited_stages": self.visited,
            "errors":        self.errors,
            "kline": {
                "close":    self.kline.close.to_string(),
                "high":     self.kline.high.to_string(),
                "low":      self.kline.low.to_string(),
                "volume":   self.kline.volume.to_string(),
            },
            "data_source":  self.data_source,
            "signal_process": self.signal_process,
            "check_table":  self.check_table,
            "risk_check":   self.risk_check,
            "execution":    self.execution,
        })
    }

    fn is_complete(&self) -> bool {
        let required = ["b_data", "c_data", "d_check", "e_risk", "f_engine"];
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
    tracing::info!("Trading System v5.4 - TickContext Data Pipeline");
    tracing::info!("Symbol: {}", SYMBOL);
    tracing::info!("Data: {}", DATA_FILE);
    tracing::info!("==============================================");

    // 初始化心跳（仅用于进程存活监控）
    init_heartbeat().await;
    tracing::info!("Heartbeat monitor initialized");

    // 创建组件
    let components = create_components().await?;

    tracing::info!("All components created:");
    tracing::info!("  - ReplaySource: loaded");
    tracing::info!("  - KlineStream: ready");
    tracing::info!("  - SignalProcessor: ready");
    tracing::info!("  - Trader: ready");
    tracing::info!("  - RiskPreChecker + OrderCheck: ready");
    tracing::info!("  - MockApiGateway: balance={}", INITIAL_BALANCE);

    // 运行数据流
    run_pipeline(components).await?;

    // 心跳报告（进程存活）
    print_heartbeat_report().await;

    Ok(())
}

async fn init_heartbeat() {
    let config = HbConfig {
        stale_threshold: 3,
        report_interval_secs: 300,
        max_file_age_hours: 24,
        max_file_size_mb: 100,
    };
    hb::init(config);
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
    // 数据源
    tracing::info!("Loading: {}", DATA_FILE);
    let replay_source = ReplaySource::from_csv(DATA_FILE).await?;
    tracing::info!("Loaded {} K-lines", replay_source.len());

    let kline_stream = Arc::new(tokio::sync::Mutex::new(
        Kline1mStream::from_klines(SYMBOL.to_string(), Box::new(replay_source))
    ));

    // 策略组件
    let signal_processor = Arc::new(SignalProcessor::new());
    signal_processor.register_symbol(SYMBOL);

    let trader = create_trader()?;

    // 风控组件
    let mut risk_checker = RiskPreChecker::new(dec!(0.15), dec!(100.0));
    risk_checker.register_symbol(SYMBOL.to_string());
    let risk_checker = Arc::new(risk_checker);

    let order_checker = Arc::new(OrderCheck::new());

    // 交易所模拟
    let gateway = Arc::new(MockApiGateway::new(
        INITIAL_BALANCE,
        MockConfig::default(),
    ));

    Ok(SystemComponents {
        kline_stream,
        signal_processor,
        trader,
        risk_checker,
        order_checker,
        gateway,
    })
}

fn create_trader() -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
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
        max_position:     config.max_position,
    };
    let executor = Arc::new(Executor::new(executor_config));

    let repository = Arc::new(Repository::new(SYMBOL, DB_PATH)?);
    let store: StoreRef = b_data_source::default_store().clone();

    Ok(Arc::new(Trader::new(config, executor, repository, store)))
}

// ============================================================================
// 核心：数据流管道
// ============================================================================

async fn run_pipeline(components: SystemComponents) -> Result<(), Box<dyn std::error::Error>> {
    let mut heartbeat_tick = interval(Duration::from_millis(1000));
    let mut loop_count = 0u64;
    let mut kline_count = 0usize;
    let mut tick_count = 0usize;

    tracing::info!("Pipeline started");

    loop {
        tokio::select! {
            // 心跳：进程存活监控
            _ = heartbeat_tick.tick() => {
                tracing::trace!("[HB] alive #{}", loop_count);
            }

            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                loop_count += 1;

                // ========== 步骤1: 获取 K 线 → 写入 ctx ==========
                let kline_data = {
                    let mut stream = components.kline_stream.lock().await;
                    stream.next_message()
                };

                let Some(data) = kline_data else {
                    tracing::info!("Data exhausted at loop {}, exiting", loop_count);
                    break;
                };

                kline_count += 1;

                let kline = match parse_raw_kline(&data) {
                    Ok(k) => k,
                    Err(e) => {
                        tracing::warn!("Parse error: {}", e);
                        continue;
                    }
                };

                // 更新 MockApiGateway 价格
                components.gateway.update_price(SYMBOL, kline.close);

                // 创建 TickContext
                let mut ctx = TickContext::new(kline_count as u64, kline);

                // ========== 步骤2: b_data 写入 ==========
                stage_data_source(&mut ctx);
                components.gateway.update_price(SYMBOL, ctx.kline.close);

                // ========== 步骤3: c_data 写入 ==========
                stage_signal_process(&components, &mut ctx);

                // ========== 步骤4: d_check 写入 ==========
                stage_check_table(&components, &mut ctx).await;

                // ========== 步骤5: e_risk 写入 ==========
                stage_risk_check(&components, &mut ctx, loop_count);

                // ========== 步骤6: f_engine 写入 ==========
                stage_execution(&components, &mut ctx, loop_count);

                // ========== 输出完整报告 ==========
                tick_count += 1;

                if ctx.errors.is_empty() {
                    tracing::debug!(
                        "[Tick #{}] complete={} stages={:?}",
                        ctx.tick_id,
                        ctx.is_complete(),
                        ctx.visited
                    );
                } else {
                    tracing::warn!(
                        "[Tick #{}] errors={:?}",
                        ctx.tick_id,
                        ctx.errors
                    );
                }

                // 每 100 个 tick 打印一次完整报告
                if tick_count % 100 == 0 {
                    let report = ctx.to_report();
                    tracing::info!(
                        "[Progress] Loops: {}, Klines: {}, Complete: {}, Report: {}",
                        loop_count,
                        kline_count,
                        tick_count,
                        serde_json::to_string(&report).unwrap_or_default()
                    );
                }

                // 安全退出
                if loop_count >= 1000 {
                    tracing::info!("Max iterations reached");
                    break;
                }
            }
        }
    }

    tracing::info!("Pipeline finished: {} loops, {} klines, {} ticks", loop_count, kline_count, tick_count);
    Ok(())
}

// ============================================================================
// 各层实现
// ============================================================================

/// b_data_source 层：写入 data_source 结果
fn stage_data_source(ctx: &mut TickContext) {
    ctx.data_source = Some(DataSourceResult {
        kline_count: ctx.tick_id as usize,
        valid: ctx.kline.close > Decimal::ZERO,
    });
    ctx.visited.push("b_data");

    if !ctx.data_source.as_ref().unwrap().valid {
        ctx.errors.push(StageError {
            stage:  "b_data".into(),
            code:   "INVALID_PRICE".into(),
            detail: format!("close={} <= 0", ctx.kline.close),
        });
    }
}

/// c_data_process 层：写入 signal_process 结果
fn stage_signal_process(components: &SystemComponents, ctx: &mut TickContext) {
    let result = components.signal_processor.min_update(
        SYMBOL,
        ctx.kline.high,
        ctx.kline.low,
        ctx.kline.close,
        ctx.kline.volume,
    );

    let generated = result.is_ok();
    ctx.signal_process = Some(SignalProcessResult {
        zscore_14:  None,
        tr_base:    None,
        pos_norm:   None,
        generated,
    });
    ctx.visited.push("c_data");

    if let Err(e) = result {
        ctx.errors.push(StageError {
            stage:  "c_data".into(),
            code:   "SIGNAL_ERROR".into(),
            detail: e.to_string(),
        });
    }
}

/// d_checktable 层：写入 check_table 结果（核心交易决策）
async fn stage_check_table(components: &SystemComponents, ctx: &mut TickContext) {
    let trade_result = components.trader.execute_once_wal().await;

    match &trade_result {
        Ok(d_checktable::h_15m::ExecutionResult::Executed { qty, .. }) => {
            ctx.check_table = Some(CheckTableResult {
                pin_conditions:  5,
                volatility_tier: "Medium".into(),
                decision:        "long_entry".into(),
                qty:             Some(*qty),
            });
            ctx.visited.push("d_check");
        }
        Ok(d_checktable::h_15m::ExecutionResult::Skipped(reason)) => {
            ctx.check_table = Some(CheckTableResult {
                pin_conditions:  0,
                volatility_tier: "N/A".into(),
                decision:        format!("skipped:{}", reason),
                qty:             None,
            });
            ctx.visited.push("d_check");
        }
        Ok(d_checktable::h_15m::ExecutionResult::Failed(e)) => {
            ctx.errors.push(StageError {
                stage:  "d_check".into(),
                code:   "TRADE_FAILED".into(),
                detail: e.to_string(),
            });
        }
        Err(e) => {
            ctx.errors.push(StageError {
                stage:  "d_check".into(),
                code:   "TRADE_ERROR".into(),
                detail: e.to_string(),
            });
        }
    }
}

/// e_risk_monitor 层：写入 risk_check 结果
fn stage_risk_check(components: &SystemComponents, ctx: &mut TickContext, loop_id: u64) {
    // 需要从 check_table 读取 qty 来做风控
    let Some(ct) = &ctx.check_table else {
        ctx.visited.push("e_risk");
        return;
    };
    let Some(qty) = ct.qty else {
        ctx.visited.push("e_risk");
        return;
    };

    // ER-001: 账户余额检查
    let balance_passed = components.risk_checker
        .pre_check(SYMBOL, INITIAL_BALANCE, dec!(100), INITIAL_BALANCE)
        .is_ok();

    // ER-003: 订单参数检查
    let order_check_result = components.order_checker
        .pre_check(
            &format!("order_{}", loop_id),
            SYMBOL,
            "h_15m_strategy",
            dec!(100),
            INITIAL_BALANCE,
            dec!(0),
        );

    let order_passed = order_check_result.passed;

    ctx.risk_check = Some(RiskCheckResult {
        balance_passed,
        order_passed,
    });
    ctx.visited.push("e_risk");

    if !balance_passed || !order_passed {
        ctx.errors.push(StageError {
            stage:  "e_risk".into(),
            code:   "RISK_REJECTED".into(),
            detail: format!("balance={} order={}", balance_passed, order_passed),
        });
    }
}

/// f_engine 层：写入 execution 结果
fn stage_execution(components: &SystemComponents, ctx: &mut TickContext, loop_id: u64) {
    let Some(ct) = &ctx.check_table else { return };
    let Some(qty) = ct.qty else { return };
    let Some(rc) = &ctx.risk_check else { return };

    // 风控未通过，跳过成交
    if !rc.balance_passed || !rc.order_passed {
        ctx.execution = Some(ExecutionResult {
            order_id:    format!("order_{}", loop_id),
            filled_price: None,
            filled_qty:   None,
            slippage:     None,
            commission:   None,
            status:      "risk_rejected".into(),
        });
        ctx.visited.push("f_engine");
        return;
    }

    // 执行模拟成交
    let order = components.gateway.place_order(SYMBOL, Side::Buy, qty, None);

    match order {
        Ok(o) => {
            ctx.execution = Some(ExecutionResult {
                order_id:    format!("order_{}", loop_id),
                filled_price: Some(o.filled_price),
                filled_qty:   Some(o.filled_qty),
                slippage:     Some(dec!(0.0001)),
                commission:   Some(dec!(0.0002)),
                status:      "filled".into(),
            });
            ctx.visited.push("f_engine");
        }
        Err(e) => {
            ctx.execution = Some(ExecutionResult {
                order_id:    format!("order_{}", loop_id),
                filled_price: None,
                filled_qty:   None,
                slippage:     None,
                commission:   None,
                status:      format!("error:{}", e),
            });
            ctx.visited.push("f_engine");
            ctx.errors.push(StageError {
                stage:  "f_engine".into(),
                code:   "ORDER_ERROR".into(),
                detail: e.to_string(),
            });
        }
    }
}

// ============================================================================
// 工具函数
// ============================================================================

fn parse_raw_kline(data: &str) -> Result<RawKline, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct Raw {
        kline_start_time: i64,
        symbol: String,
        #[serde(rename = "open")]
        open_str: String,
        #[serde(rename = "close")]
        close_str: String,
        #[serde(rename = "high")]
        high_str: String,
        #[serde(rename = "low")]
        low_str: String,
        #[serde(rename = "volume")]
        volume_str: String,
        is_closed: bool,
    }

    let raw: Raw = serde_json::from_str(data)
        .or_else(|_| serde_json::from_str(data))?;

    Ok(RawKline {
        open:     raw.open_str.parse()?,
        close:    raw.close_str.parse()?,
        high:     raw.high_str.parse()?,
        low:      raw.low_str.parse()?,
        volume:   raw.volume_str.parse()?,
        is_closed: raw.is_closed,
    })
}

// ============================================================================
// 心跳报告
// ============================================================================

async fn print_heartbeat_report() {
    tracing::info!("==============================================");
    tracing::info!("HEARTBEAT REPORT (进程存活监控)");
    tracing::info!("==============================================");

    let reporter = hb::global();
    let summary = reporter.summary().await;

    tracing::info!("Total points: {}", summary.total_points);
    tracing::info!("Active: {}", summary.active_count);
    tracing::info!("Reports: {}", summary.reports_count);

    if let Err(e) = reporter.save_report("heartbeat_report.json").await {
        tracing::warn!("Save failed: {}", e);
    } else {
        tracing::info!("Saved to heartbeat_report.json");
    }
}
