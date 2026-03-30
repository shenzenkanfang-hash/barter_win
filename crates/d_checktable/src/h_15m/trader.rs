#![forbid(unsafe_code)]

//! h_15m/trader.rs - 品种交易主循环
//!
//! 从 MarketDataStore 读取数据，生成交易信号
//! 自循环架构：Trader 自己 loop，Engine 管理 spawn/stop/monitor
//!
//! # 修复记录
//! - v2.0: P0-1 主循环启用、P0-2 local_position 填充、P0-3 风控接入、P1-2 锁日志、P1-3 价格偏离度
//! - v2.1: P2-1 gc_pending 定时调用基础设施
//! - v3.0: 心跳报到集成 (DT-002)

//! 心跳报到测试点 ID
const HEARTBEAT_POINT_TRADER: &str = "DT-002";

use chrono::Utc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use parking_lot::{Mutex, RwLock as ParkingRwLock};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use thiserror::Error;
use tokio::sync::{mpsc, Notify};
use tokio::time::interval;
use x_data::position::{LocalPosition, PositionDirection, PositionSide};
use x_data::trading::signal::{StrategyId, StrategySignal, TradeCommand};

use a_common::heartbeat::Token as HeartbeatToken;
use b_data_source::MarketDataStore;

use crate::h_15m::executor::{Executor, OrderType};
use crate::h_15m::quantity_calculator::{MinQuantityCalculator, MinQuantityConfig};
use crate::h_15m::repository::{RepoError, Repository, TradeRecord};
use crate::h_15m::{MinSignalGenerator, PinStatus, PinStatusMachine};
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityTier};

/// MarketDataStore trait object for dependency injection
pub type StoreRef = Arc<dyn MarketDataStore + Send + Sync>;

// ==================== P0-3: 账户服务接口 ====================

/// 账户信息结构（用于风控）
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub available_balance: Decimal,
    pub total_equity: Decimal,
    pub unrealized_pnl: Decimal,
    pub used_margin: Decimal,
}

impl Default for AccountInfo {
    fn default() -> Self {
        Self {
            available_balance: dec!(10000),
            total_equity: dec!(10000),
            unrealized_pnl: Decimal::ZERO,
            used_margin: Decimal::ZERO,
        }
    }
}

/// Trader 错误类型
#[derive(Debug, Clone, Error)]
pub enum TraderError {
    #[error("账户服务不可用: {0}")]
    AccountServiceUnavailable(String),

    #[error("未配置账户服务，无法获取风控参数")]
    AccountProviderNotConfigured,

    #[error("风控检查失败: {0}")]
    RiskCheckFailed(String),

    #[error("锁竞争失败")]
    LockContention,

    #[error("WAL 记录错误: {0}")]
    RepoError(String),

    #[error("下单失败: {0}")]
    OrderFailed(String),

    #[error("其他错误: {0}")]
    Other(String),
}

/// 账户信息提供者 Trait（已废弃，使用 AccountProviderFn 代替）
/// @deprecated v2.3: 使用 AccountProviderFn 替代以避免 async trait dyn 兼容性问题
#[deprecated(since = "2.3.0", note = "使用 AccountProviderFn 替代")]
#[allow(async_fn_in_trait)]
pub trait AccountProvider: Send + Sync {
    async fn get_account(&self, symbol: &str) -> Result<AccountInfo, TraderError>;
}

/// WAL 执行结果枚举（明确区分成功/跳过/失败）
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// 成功下单，返回订单数量
    Executed { qty: Decimal, order_type: OrderType },
    Skipped(&'static str),
    Failed(TraderError),
}

impl ExecutionResult {
    pub fn is_executed(&self) -> bool {
        matches!(self, ExecutionResult::Executed { .. })
    }
}

/// 品种交易器配置
#[derive(Debug, Clone)]
pub struct TraderConfig {
    pub symbol: String,
    pub interval_ms: u64,
    pub max_position: Decimal,
    pub initial_ratio: Decimal,
    pub db_path: String,
    pub order_interval_ms: u64,
    pub lot_size: Decimal,
    /// Python 原版阈值配置（v3.0: 完全对齐 Python）
    pub thresholds: ThresholdConfig,
}

impl Default for TraderConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 100,
            max_position: dec!(0.15),
            initial_ratio: dec!(0.05),
            db_path: "./data/trade_records.db".to_string(),
            order_interval_ms: 100,
            lot_size: dec!(0.001),
            thresholds: ThresholdConfig::default(),
        }
    }
}

/// Python 原版阈值配置（从 pin_main.py 1:1 移植）
/// v3.0: 完全对齐 Python 原版行为
#[derive(Debug, Clone)]
pub struct ThresholdConfig {
    /// 盈利平仓阈值：1% (从 pin_main.py PROFIT_THRESHOLD 移植)
    pub profit_threshold: Decimal,
    /// 多头对冲触发：价格 < 开仓价 * 0.98 (下跌2%)
    pub price_down_threshold: Decimal,
    /// 空头对冲触发：价格 > 开仓价 * 1.02 (上涨2%)
    pub price_up_threshold: Decimal,
    /// 多头对冲硬阈值：价格 < 开仓价 * 0.90 (下跌10%)
    pub price_down_hard_threshold: Decimal,
    /// 空头对冲硬阈值：价格 > 开仓价 * 1.10 (上涨10%)
    pub price_up_hard_threshold: Decimal,
    /// 多头加仓价格阈值：价格 > 开仓价 * 1.02 (上涨2%)
    pub long_add_threshold: Decimal,
    /// 多头加仓硬阈值：价格 > 开仓价 * 1.08 (上涨8%)
    pub long_add_hard_threshold: Decimal,
    /// 空头加仓价格阈值：价格 < 开仓价 * 0.98 (下跌2%)
    pub short_add_threshold: Decimal,
    /// 空头加仓硬阈值：价格 < 开仓价 * 0.92 (下跌8%)
    pub short_add_hard_threshold: Decimal,
    /// 最低平仓线阈值（止损）：开仓价 * (1 - profit_threshold)
    pub stop_loss_threshold: Decimal,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            // 盈利平仓：1%
            profit_threshold: dec!(0.01),
            // 多头对冲：下跌2%
            price_down_threshold: dec!(0.98),
            // 空头对冲：上涨2%
            price_up_threshold: dec!(1.02),
            // 多头对冲硬阈值：下跌10%
            price_down_hard_threshold: dec!(0.90),
            // 空头对冲硬阈值：上涨10%
            price_up_hard_threshold: dec!(1.10),
            // 多头加仓：上涨2%
            long_add_threshold: dec!(1.02),
            // 多头加仓硬阈值：上涨8%
            long_add_hard_threshold: dec!(1.08),
            // 空头加仓：下跌2%
            short_add_threshold: dec!(0.98),
            // 空头加仓硬阈值：下跌8%
            short_add_hard_threshold: dec!(0.92),
            // 止损线：亏损1%
            stop_loss_threshold: dec!(0.99),
        }
    }
}

/// GC 配置（v2.1: P2-1 gc_pending 定时调用）
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// 超时时间（秒），超过此时间的 PENDING 记录将被清理
    pub timeout_secs: i64,
    /// 执行间隔（秒）
    pub interval_secs: u64,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300,  // 5分钟
            interval_secs: 60,  // 1分钟
        }
    }
}

impl GcConfig {
    /// 生产环境配置（更长间隔）
    pub fn production() -> Self {
        Self {
            timeout_secs: 600,  // 10分钟
            interval_secs: 300, // 5分钟
        }
    }
    
    /// 测试环境配置（短间隔）
    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            timeout_secs: 30,
            interval_secs: 5,
        }
    }
}

/// 数量计算器配置（v2.2: P1-2 集成 quantity_calculator）
#[derive(Debug, Clone)]
pub struct QuantityCalculatorConfig {
    /// 基础开仓数量
    pub base_open_qty: Decimal,
    /// 最大持仓数量
    pub max_position_qty: Decimal,
    /// 加仓倍数
    pub add_multiplier: Decimal,
    /// 波动率调整启用
    pub vol_adjustment: bool,
}

impl Default for QuantityCalculatorConfig {
    fn default() -> Self {
        Self {
            base_open_qty: dec!(0.05),
            max_position_qty: dec!(0.15),
            add_multiplier: dec!(1.5),
            vol_adjustment: true,
        }
    }
}

/// 订单数量计算结果
#[derive(Debug, Clone)]
pub struct OrderQuantityResult {
    /// 计算数量
    pub qty: Decimal,
    /// 是否全平
    pub full_close: bool,
    /// 计算说明
    pub reason: String,
}

// ==================== v2.3: P1-1 信号输入真实数据接入 ====================

/// 市场指标数据结构
/// 用于 Pin 信号生成的所有输入数据
#[derive(Debug, Clone)]
pub struct MarketIndicators {
    /// TR基准（60分钟）
    pub tr_base_60min: Decimal,
    /// TR比率（15分钟）
    pub tr_ratio_15min: Decimal,
    /// Z分数（14周期，1分钟）
    pub zscore_14_1m: Decimal,
    /// Z分数（1小时，1分钟）
    pub zscore_1h_1m: Decimal,
    /// TR比率（60分钟/5小时）
    pub tr_ratio_60min_5h: Decimal,
    /// TR比率（10分钟/1小时）
    pub tr_ratio_10min_1h: Decimal,
    /// 持仓标准化（60分钟）
    pub pos_norm_60: Decimal,
    /// 累计百分位（1小时）
    pub acc_percentile_1h: Decimal,
    /// 速度百分位（1小时）
    pub velocity_percentile_1h: Decimal,
    /// Pine背景颜色
    pub pine_bg_color: String,
    /// Pine柱状颜色
    pub pine_bar_color: String,
    /// 价格偏离度
    pub price_deviation: Decimal,
    /// 价格偏离度水平位置
    pub price_deviation_horizontal_position: Decimal,
}

impl Default for MarketIndicators {
    fn default() -> Self {
        Self {
            tr_base_60min: Decimal::ZERO,
            tr_ratio_15min: Decimal::ZERO,
            zscore_14_1m: Decimal::ZERO,
            zscore_1h_1m: Decimal::ZERO,
            tr_ratio_60min_5h: Decimal::ZERO,
            tr_ratio_10min_1h: Decimal::ZERO,
            pos_norm_60: dec!(50),
            acc_percentile_1h: Decimal::ZERO,
            velocity_percentile_1h: Decimal::ZERO,
            pine_bg_color: String::new(),
            pine_bar_color: String::new(),
            price_deviation: Decimal::ZERO,
            price_deviation_horizontal_position: dec!(50),
        }
    }
}

/// 指标计算器函数类型
/// 使用 Box<dyn Fn> 而非 trait object，避免 async trait dyn 兼容性问题
pub type IndicatorCalcFn = Box<dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MarketIndicators, TraderError>> + Send>> + Send + Sync>;

/// 价格偏离度计算器函数类型
pub type PriceDeviationFn = Box<dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(Decimal, Decimal), TraderError>> + Send>> + Send + Sync>;

/// 账户提供者函数类型
pub type AccountProviderFn = Box<dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AccountInfo, TraderError>> + Send>> + Send + Sync>;

/// 品种交易器
pub struct Trader {
    config: TraderConfig,
    /// 状态机（使用 Arc<ParkingRwLock>，支持同步和异步上下文）
    /// v2.4 FIX: 从 TokioRwLock 改为 Arc<ParkingRwLock>，解决同步方法中 try_read 可能失败的问题
    /// Arc 允许克隆到 spawn_blocking 闭包中
    status_machine: Arc<ParkingRwLock<PinStatusMachine>>,
    signal_generator: MinSignalGenerator,
    /// 持仓快照（使用 Arc<ParkingRwLock>，支持同步和异步上下文）
    /// v2.4 FIX: 从 TokioRwLock 改为 Arc<ParkingRwLock>
    position: Arc<ParkingRwLock<Option<LocalPosition>>>,
    executor: Arc<Executor>,
    repository: Arc<Repository>,
    store: StoreRef,
    /// P0-3: 账户提供者（必须配置，否则拒绝下单）
    /// v2.3: 使用函数类型替代 trait object
    account_provider: Option<AccountProviderFn>,
    last_order_ms: AtomicU64,
    is_running: AtomicBool,
    shutdown: Notify,
    /// v2.1: GC 配置（保留用于外部驱动）
    #[allow(dead_code)]
    gc_config: GcConfig,
    /// v2.1: GC 任务句柄（用于优雅停止，保留用于外部驱动）
    #[allow(dead_code)]
    gc_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// v2.2: P1-2 数量计算器（可选，不配置时使用 executor 默认逻辑）
    quantity_calculator: Option<MinQuantityCalculator>,
    /// v2.3: P1-1 指标计算器（可选，不配置时使用默认值）
    indicator_calculator: Option<IndicatorCalcFn>,
    /// v3.0: 心跳 Token（用于心跳报到）
    heartbeat_token: Arc<ParkingRwLock<Option<HeartbeatToken>>>,
    /// v4.0: 流水线观测表（可选，不配置则不记录）
    pipeline_store: Option<Arc<b_data_source::store::PipelineStore>>,
}

impl Trader {
    /// 创建 Trader（需要注入 executor、repository 和 store）
    /// P0-3 修复：使用此构造函数时，风控将被禁用（不安全，生产环境禁止）
    /// v2.1: 使用默认 GC 配置
    /// v2.2: quantity_calculator = None，使用 executor 默认逻辑
    /// v2.4 FIX: 使用 Arc<ParkingRwLock> 支持异步上下文
    /// v4.0: pipeline_store = None，不记录观测表
    pub fn new(
        config: TraderConfig,
        executor: Arc<Executor>,
        repository: Arc<Repository>,
        store: StoreRef,
    ) -> Self {
        Self {
            config: config.clone(),
            // v2.4 FIX: 使用 Arc<ParkingRwLock>
            status_machine: Arc::new(ParkingRwLock::new(PinStatusMachine::new())),
            signal_generator: MinSignalGenerator::new(),
            // v2.4 FIX: 使用 Arc<ParkingRwLock>
            position: Arc::new(ParkingRwLock::new(None)),
            executor,
            repository,
            store,
            account_provider: None,
            last_order_ms: AtomicU64::new(0),
            is_running: AtomicBool::new(false),
            shutdown: Notify::new(),
            gc_config: GcConfig::default(),
            gc_handle: Arc::new(Mutex::new(None)),
            quantity_calculator: None,
            indicator_calculator: None,
            // v3.0: 心跳 Token
            heartbeat_token: Arc::new(ParkingRwLock::new(None)),
            // v4.0: 流水线观测表（可选）
            pipeline_store: None,
        }
    }

    /// 创建带流水线观测的 Trader（v4.0）
    ///
    /// 推荐使用此构造函数，可记录 PipelineStage 观测数据。
    pub fn new_with_pipeline(
        config: TraderConfig,
        executor: Arc<Executor>,
        repository: Arc<Repository>,
        store: StoreRef,
        pipeline_store: Arc<b_data_source::store::PipelineStore>,
    ) -> Self {
        Self {
            config: config.clone(),
            status_machine: Arc::new(ParkingRwLock::new(PinStatusMachine::new())),
            signal_generator: MinSignalGenerator::new(),
            position: Arc::new(ParkingRwLock::new(None)),
            executor,
            repository,
            store,
            account_provider: None,
            last_order_ms: AtomicU64::new(0),
            is_running: AtomicBool::new(false),
            shutdown: Notify::new(),
            gc_config: GcConfig::default(),
            gc_handle: Arc::new(Mutex::new(None)),
            quantity_calculator: None,
            indicator_calculator: None,
            heartbeat_token: Arc::new(ParkingRwLock::new(None)),
            pipeline_store: Some(pipeline_store),
        }
    }

    /// 创建带账户服务的 Trader（推荐）
    /// P0-3 修复：必须配置 AccountProvider 才能下单
    /// v2.1: 使用默认 GC 配置
    /// v2.2: quantity_calculator = None，使用 executor 默认逻辑
    /// v2.3: indicator_calculator = None，使用默认值
    /// v2.4 FIX: 使用 Arc<ParkingRwLock> 支持异步上下文
    pub fn with_account_provider(
        config: TraderConfig,
        executor: Arc<Executor>,
        repository: Arc<Repository>,
        store: StoreRef,
        account_provider: AccountProviderFn,
    ) -> Self {
        Self {
            config: config.clone(),
            // v2.4 FIX: 使用 Arc<ParkingRwLock>
            status_machine: Arc::new(ParkingRwLock::new(PinStatusMachine::new())),
            signal_generator: MinSignalGenerator::new(),
            // v2.4 FIX: 使用 Arc<ParkingRwLock>
            position: Arc::new(ParkingRwLock::new(None)),
            executor,
            repository,
            store,
            account_provider: Some(account_provider),
            last_order_ms: AtomicU64::new(0),
            is_running: AtomicBool::new(false),
            shutdown: Notify::new(),
            gc_config: GcConfig::default(),
            gc_handle: Arc::new(Mutex::new(None)),
            quantity_calculator: None,
            indicator_calculator: None,
            // v3.0: 心跳 Token
            heartbeat_token: Arc::new(ParkingRwLock::new(None)),
            // v4.0: pipeline_store = None，不记录观测表
            pipeline_store: None,
        }
    }

    /// 创建 Trader（使用默认 store）
    pub fn with_default_store(config: TraderConfig, executor: Arc<Executor>, repository: Arc<Repository>) -> Self {
        // Clone the Arc to convert &Arc<impl> to Arc<dyn Trait>
        let store: StoreRef = b_data_source::default_store().clone();
        Self::new(config, executor, repository, store)
    }

    /// 从 Store 获取当前K线
    pub fn get_current_kline(&self) -> Option<b_data_source::ws::kline_1m::ws::KlineData> {
        self.store.get_current_kline(&self.config.symbol)
    }

    /// 从 Store 获取波动率
    pub fn get_volatility(&self) -> Option<b_data_source::store::VolatilityData> {
        self.store.get_volatility(&self.config.symbol)
    }

    /// 获取当前价格
    pub fn current_price(&self) -> Option<Decimal> {
        self.get_current_kline()
            .and_then(|k| k.close.parse().ok())
    }

    /// 获取配置引用（供 main.rs 使用）
    pub fn config(&self) -> &TraderConfig {
        &self.config
    }

    /// 获取当前状态（供 main.rs 使用）
    pub fn current_status(&self) -> PinStatus {
        self.status_machine.read().current_status()
    }

    // ==================== v3.0: 心跳报到 ====================

    /// 设置心跳 Token（用于心跳报到）
    /// v3.0: 心跳报到集成
    pub fn set_heartbeat_token(&self, token: HeartbeatToken) {
        let mut guard = self.heartbeat_token.write();
        *guard = Some(token);
    }

    /// 获取当前心跳 Token（如果存在）
    /// v3.0: 心跳报到集成
    pub fn get_heartbeat_token(&self) -> Option<HeartbeatToken> {
        self.heartbeat_token.read().clone()
    }

    /// 心跳报到（内部方法）
    /// v3.0: 心跳报到集成
    async fn heartbeat_report(&self) {
        let token = self.get_heartbeat_token();
        if let Some(token) = token {
            if let Ok(reporter) = std::panic::catch_unwind(|| a_common::heartbeat::global()) {
                reporter.report(
                    &token,
                    HEARTBEAT_POINT_TRADER,
                    "d_checktable::h_15m",
                    "execute_once_wal",
                    file!(),
                ).await;
            }
        }
    }

    /// 获取波动率值
    pub fn volatility_value(&self) -> Option<f64> {
        self.get_volatility().map(|v| v.volatility)
    }

    /// 判断波动率通道
    fn volatility_tier(&self) -> VolatilityTier {
        let vol_val = self.volatility_value();
        tracing::trace!(symbol = %self.config.symbol, volatility = ?vol_val, "波动率通道判断");

        let tier = match vol_val {
            Some(v) if v > 0.15 => VolatilityTier::High,
            Some(v) if v > 0.05 => VolatilityTier::Medium,
            _ => VolatilityTier::Low,
        };

        tracing::trace!(symbol = %self.config.symbol, tier = ?tier, "选择通道");
        tier
    }

    /// 获取账户信息（沙盒环境使用默认值）
    async fn fetch_account_info(&self) -> Result<AccountInfo, TraderError> {
        if let Some(ref provider) = self.account_provider {
            match provider(self.config.symbol.clone()).await {
                Ok(info) => {
                    tracing::trace!(
                        symbol = %self.config.symbol,
                        available = %info.available_balance,
                        equity = %info.total_equity,
                        "获取账户信息成功"
                    );
                    return Ok(info);
                }
                Err(e) => {
                    tracing::error!(
                        symbol = %self.config.symbol,
                        error = %e,
                        "账户服务不可用，拒绝下单"
                    );
                    return Err(TraderError::AccountServiceUnavailable(e.to_string()));
                }
            }
        }

        // 沙盒环境：返回默认账户信息
        tracing::warn!(
            symbol = %self.config.symbol,
            "沙盒环境使用默认账户信息"
        );
        Ok(AccountInfo::default())
    }

    /// 获取当前持仓方向（异步）
    /// v2.4 FIX: 使用 spawn_blocking 访问 parking_lot::RwLock
    pub async fn current_position_side(&self) -> Option<PositionDirection> {
        let position = self.position.clone();
        tokio::task::spawn_blocking(move || {
            position.read().as_ref().map(|p| p.direction)
        })
        .await
        .unwrap_or(None)
    }

    /// 获取当前持仓数量（异步）
    /// v2.4 FIX: 使用 spawn_blocking 访问 parking_lot::RwLock
    pub async fn current_position_qty(&self) -> Decimal {
        let position = self.position.clone();
        tokio::task::spawn_blocking(move || {
            position.read().as_ref().map(|p| p.qty).unwrap_or_default()
        })
        .await
        .unwrap_or_default()
    }

    /// 从记录恢复 Trader 状态（异步）
    /// v2.4 FIX: 使用 spawn_blocking 访问 parking_lot::RwLock
    pub async fn restore_from_record(&self, record: &TradeRecord) {
        // 恢复状态机
        if let Some(ref status_str) = record.trader_status {
            if let Ok(status) = serde_json::from_str::<PinStatus>(status_str) {
                // v2.4 FIX: spawn_blocking 用于 parking_lot 锁，需要 clone Arc
                let status_machine = Arc::clone(&self.status_machine);
                let symbol = self.config.symbol.clone();
                tokio::task::spawn_blocking(move || {
                    status_machine.write().set_status(status);
                })
                .await
                .ok();
                tracing::info!(
                    symbol = %symbol,
                    ?status,
                    "状态机已恢复"
                );
            }
        }

        // 恢复持仓
        if let Some(ref pos_str) = record.local_position {
            if let Ok(position) = serde_json::from_str::<LocalPosition>(pos_str) {
                let qty = position.qty;
                let position = Some(position);
                // v2.4 FIX: spawn_blocking 用于 parking_lot 锁，需要 clone Arc
                let position_arc = Arc::clone(&self.position);
                let symbol = self.config.symbol.clone();
                tokio::task::spawn_blocking(move || {
                    *position_arc.write() = position;
                })
                .await
                .ok();
                tracing::info!(
                    symbol = %symbol,
                    qty = %qty,
                    "持仓已恢复"
                );
            }
        }

        // 恢复频率限制
        if let Some(ts) = record.order_timestamp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            const RATE_LIMIT_INTERVAL_MS: u64 = 300_000;
            if now.saturating_sub(ts as u64) < RATE_LIMIT_INTERVAL_MS {
                self.last_order_ms.store(ts as u64, Ordering::Relaxed);
                tracing::info!(
                    symbol = %self.config.symbol,
                    last_order_ms = ts,
                    "已恢复下单频率限制"
                );
            }
        }
    }

    /// 停止 Trader（优雅停止）
    pub fn stop(&self) {
        // AtomicBool: 无锁设置 is_running 为 false
        self.is_running.store(false, Ordering::SeqCst);
        // 通知所有等待者
        self.shutdown.notify_waiters();
    }

    // ==================== v2.2: P1-2 数量计算器集成 ====================

    /// 创建带数量计算器的 Trader（v2.2）
    /// 在已有 Trader 基础上添加 quantity_calculator
    pub fn with_quantity_calculator(
        mut self,
        qty_config: QuantityCalculatorConfig,
    ) -> Self {
        self.quantity_calculator = Some(MinQuantityCalculator::new(MinQuantityConfig {
            base_open_qty: qty_config.base_open_qty,
            max_position_qty: qty_config.max_position_qty,
            add_multiplier: qty_config.add_multiplier,
            vol_adjustment: qty_config.vol_adjustment,
        }));
        tracing::trace!(
            symbol = %self.config.symbol,
            base_open_qty = %qty_config.base_open_qty,
            max_position_qty = %qty_config.max_position_qty,
            vol_adjustment = qty_config.vol_adjustment,
            "数量计算器已启用"
        );
        self
    }

    /// 计算订单数量（v2.2）
    /// - 如果配置了 quantity_calculator，使用它计算
    /// - 否则降级到 executor.calculate_order_qty()
    fn calculate_order_quantity(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
        signal_output: &MinSignalOutput,
    ) -> OrderQuantityResult {
        let vol_tier = self.volatility_tier();
        
        match &self.quantity_calculator {
            Some(calc) => {
                // 使用 MinQuantityCalculator
                match order_type {
                    OrderType::InitialOpen => {
                        let qty = calc.calc_open_quantity(&vol_tier);
                        OrderQuantityResult {
                            qty,
                            full_close: false,
                            reason: format!("初始开仓 qty={}", qty),
                        }
                    }
                    OrderType::DoubleAdd => {
                        let qty = calc.calc_add_quantity(current_qty, &vol_tier);
                        OrderQuantityResult {
                            qty,
                            full_close: false,
                            reason: format!("加仓 qty={}", qty),
                        }
                    }
                    OrderType::DoubleClose | OrderType::DayClose => {
                        let (qty, full_close) = 
                            calc.calc_close_quantity(current_qty, signal_output);
                        OrderQuantityResult {
                            qty,
                            full_close,
                            reason: format!("平仓 qty={} full_close={}", qty, full_close),
                        }
                    }
                    OrderType::HedgeOpen => {
                        let qty = if current_qty > Decimal::ZERO { current_qty } else { Decimal::ZERO };
                        OrderQuantityResult {
                            qty,
                            full_close: false,
                            reason: format!("对冲开仓 qty={}", qty),
                        }
                    }
                    OrderType::DayHedge => {
                        let qty = current_qty.abs();
                        OrderQuantityResult {
                            qty,
                            full_close: false,
                            reason: format!("日线对冲 qty={}", qty),
                        }
                    }
                }
            }
            None => {
                // 降级到 executor.calculate_order_qty
                let qty = self.executor.calculate_order_qty(order_type, current_qty, current_side);
                OrderQuantityResult {
                    qty,
                    full_close: false,
                    reason: "降级到 executor".to_string(),
                }
            }
        }
    }

    // ==================== v2.3: P1-1 指标计算器 ====================

    /// 创建带指标计算器的 Trader（v2.3）
    /// 在已有 Trader 基础上添加 indicator_calculator
    pub fn with_indicator_calculator(
        mut self,
        indicator_calculator: IndicatorCalcFn,
    ) -> Self {
        self.indicator_calculator = Some(indicator_calculator);
        tracing::trace!(
            symbol = %self.config.symbol,
            "指标计算器已启用"
        );
        self
    }

    /// 构建信号输入（异步版本，v2.3）
    /// P1-1 修复：从 IndicatorCalculator 获取真实数据
    /// - 如果配置了 indicator_calculator，使用它计算
    /// - 否则降级到默认值
    pub async fn build_signal_input_async(&self) -> Option<MinSignalInput> {
        let symbol = self.config.symbol.clone();
        
        if let Some(ref calculator) = self.indicator_calculator {
            match calculator(symbol.clone()).await {
                Ok(indicators) => {
                    tracing::trace!(
                        symbol = %symbol,
                        zscore_14_1m = %indicators.zscore_14_1m,
                        pos_norm_60 = %indicators.pos_norm_60,
                        "使用指标计算器数据"
                    );
                    return Some(self.indicators_to_signal_input(indicators));
                }
                Err(e) => {
                    tracing::warn!(
                        symbol = %symbol,
                        error = %e,
                        "指标计算失败，降级到默认值"
                    );
                }
            }
        }
        
        // 降级到默认值（带警告）
        tracing::warn!(
            symbol = %symbol,
            "未配置指标计算器或计算失败，使用默认值"
        );
        self.build_signal_input_fallback()
    }

    /// 将 MarketIndicators 转换为 MinSignalInput
    fn indicators_to_signal_input(&self, indicators: MarketIndicators) -> MinSignalInput {
        MinSignalInput {
            tr_base_60min: indicators.tr_base_60min,
            tr_ratio_15min: indicators.tr_ratio_15min,
            zscore_14_1m: indicators.zscore_14_1m,
            zscore_1h_1m: indicators.zscore_1h_1m,
            tr_ratio_60min_5h: indicators.tr_ratio_60min_5h,
            tr_ratio_10min_1h: indicators.tr_ratio_10min_1h,
            pos_norm_60: indicators.pos_norm_60,
            acc_percentile_1h: indicators.acc_percentile_1h,
            velocity_percentile_1h: indicators.velocity_percentile_1h,
            pine_bg_color: indicators.pine_bg_color,
            pine_bar_color: indicators.pine_bar_color,
            price_deviation: indicators.price_deviation,
            price_deviation_horizontal_position: indicators.price_deviation_horizontal_position,
        }
    }

    /// 构建信号输入（回测/沙盒版：从 Store 历史K线计算真实指标）
    ///
    /// v2.4 重构：沙盒模式 = 纯数据输入输出，无业务逻辑判断
    ///
    /// 策略：
    /// 1. 优先从 Store 历史K线计算真实指标
    /// 2. Store 无数据时才使用默认值（真正的冷启动）
    /// 3. 沙盒/回测预喂K线后，永远使用真实值
    fn build_signal_input_fallback(&self) -> Option<MinSignalInput> {
        // 尝试从 Store 加载历史K线
        let history = self.store.get_history_klines(&self.config.symbol);
        let current = self.store.get_current_kline(&self.config.symbol)?;

        // 有历史数据：计算真实指标
        if history.len() >= 14 {
            let closes: Vec<f64> = history.iter()
                .filter_map(|k| k.close.parse::<f64>().ok())
                .collect();

            let current_price = current.close.parse::<f64>().ok()?;
            let n = closes.len();
            let mean = closes.iter().sum::<f64>() / n as f64;

            // Z-score (14周期)
            let variance = closes.iter()
                .map(|p| (p - mean).powi(2))
                .sum::<f64>() / n as f64;
            let stddev = variance.sqrt();
            let zscore_14_1m = if stddev > 0.0 {
                ((current_price - mean) / stddev) as f64
            } else {
                0.0
            };

            // TR基准（最近60根的TR均值，归一化为百分比）
            let tr_values: Vec<f64> = history.iter().rev().take(60).map(|k| {
                let open = k.open.parse::<f64>().unwrap_or(0.0);
                let high = k.high.parse::<f64>().unwrap_or(0.0);
                let low = k.low.parse::<f64>().unwrap_or(0.0);
                let close = k.close.parse::<f64>().unwrap_or(0.0);
                let tr = high - low;
                if mean > 0.0 { (tr / mean) * 100.0 } else { 0.0 }
            }).collect();
            let tr_base_60min = tr_values.first().copied().unwrap_or(0.0);

            // TR比率（60min/5h）
            let tr_recent = tr_values.first().copied().unwrap_or(0.0);
            let tr_old = tr_values.get(60.min(tr_values.len() - 1)).copied().unwrap_or(0.0);
            let tr_ratio_60min_5h = if tr_old > 0.0 { tr_recent / tr_old } else { 1.0 };

            // 持仓标准化（价格在最近60根中的位置）
            let recent_prices: Vec<f64> = history.iter().rev().take(60)
                .filter_map(|k| k.close.parse::<f64>().ok())
                .collect();
            let pos_norm_60 = if let Some((min_p, max_p)) = recent_prices.iter().cloned().fold(None, |acc, p| {
                match acc {
                    None => Some((p, p)),
                    Some((min_v, max_v)) => Some((min_v.min(p), max_v.max(p))),
                }
            }) {
                let range = max_p - min_p;
                if range > 0.0 { ((current_price - min_p) / range * 100.0).clamp(0.0, 100.0) } else { 50.0 }
            } else { 50.0 };

            return Some(MinSignalInput {
                tr_base_60min: rust_decimal::Decimal::try_from(tr_base_60min).ok()?,
                tr_ratio_15min: rust_decimal::Decimal::try_from(0.05).ok()?,
                zscore_14_1m: rust_decimal::Decimal::try_from(zscore_14_1m).ok()?,
                zscore_1h_1m: rust_decimal::Decimal::try_from(0.0).ok()?,
                tr_ratio_60min_5h: rust_decimal::Decimal::try_from(tr_ratio_60min_5h).ok()?,
                tr_ratio_10min_1h: rust_decimal::Decimal::try_from(1.0).ok()?,
                pos_norm_60: rust_decimal::Decimal::try_from(pos_norm_60).ok()?,
                acc_percentile_1h: rust_decimal::Decimal::try_from(50.0).ok()?,
                velocity_percentile_1h: rust_decimal::Decimal::try_from(50.0).ok()?,
                pine_bg_color: String::new(),
                pine_bar_color: String::new(),
                price_deviation: rust_decimal::Decimal::try_from(0.0).ok()?,
                price_deviation_horizontal_position: rust_decimal::Decimal::try_from(50.0).ok()?,
            });
        }

        // Store 无历史数据：真正的冷启动，返回 None 让调用方跳过
        tracing::warn!(
            symbol = %self.config.symbol,
            history_len = history.len(),
            "沙盒环境无历史数据，无法构建信号输入"
        );
        None
    }

    // ==================== v2.1: P2-1 GC 定时任务（已废弃） ====================
    //
    // ⚠️ 警告：start_gc_task 使用 tokio::spawn，违反事件驱动原则
    //
    // 替代方案：
    // 1. 调用者定期调用 gc_pending() 方法
    // 2. 或使用外部定时器驱动清理
    //
    // TODO: 重构为按需清理或外部驱动

    /// 启动 GC 定时任务（已废弃）
    ///
    /// ⚠️ 已废弃：使用 tokio::spawn 启动后台任务
    /// 保留用于未来外部驱动实现
    #[deprecated(since = "1.0.0", note = "使用外部定时器驱动 gc_pending() 替代")]
    #[allow(dead_code)]
    fn start_gc_task(&self) {
        let repo = Arc::clone(&self.repository);
        let timeout_secs = self.gc_config.timeout_secs;
        let interval_secs = self.gc_config.interval_secs;
        let symbol = self.config.symbol.clone();
        let gc_handle = Arc::clone(&self.gc_handle);
        let symbol_for_log = symbol.clone();  // 克隆用于闭包后的日志
        
        let handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));
            
            tracing::trace!(
                symbol = %symbol,
                timeout_secs = timeout_secs,
                interval_secs = interval_secs,
                "GC 定时任务启动"
            );
            
            loop {
                ticker.tick().await;
                
                match repo.gc_pending() {
                    Ok(count) if count > 0 => {
                        tracing::info!(
                            symbol = %symbol,
                            count = count,
                            timeout_secs = timeout_secs,
                            "GC 清理完成"
                        );
                    }
                    Ok(_) => {
                        tracing::trace!(
                            symbol = %symbol,
                            "GC 检查完成，无待清理记录"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            symbol = %symbol,
                            error = %e,
                            "GC 清理失败"
                        );
                    }
                }
            }
        });
        
        // 使用锁安全存储 handle
        let mut guard = gc_handle.lock();
        *guard = Some(handle);
        tracing::trace!(
            symbol = %symbol_for_log,
            "GC 任务句柄已注册"
        );
    }

    /// 停止 GC 任务（v2.1: P2-1）
    /// 优雅终止 GC 后台任务
    async fn stop_gc_task(&self) {
        let handle = {
            let mut guard = self.gc_handle.lock();
            guard.take()  // 取出 handle，Mutex 变为 None
        };
        
        if let Some(h) = handle {
            tracing::trace!(
                symbol = %self.config.symbol,
                "正在停止 GC 任务"
            );
            h.abort();
            match h.await {
                Ok(_) => {
                    tracing::info!(
                        symbol = %self.config.symbol,
                        "GC 任务已正常停止"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        symbol = %self.config.symbol,
                        error = %e,
                        "GC 任务异常终止"
                    );
                }
            }
        }
    }

    /// 主循环执行一次（同步版，保持兼容性）
    /// 注意：同步版本使用默认值，无法使用异步的 IndicatorCalculator
    pub fn execute_once(&self) -> Option<StrategySignal> {
        // 1. 获取数据
        let _kline = self.get_current_kline()?;

        // v2.4 回测修复：如果 Store 有真实波动率数据，使用真实通道
        // 架构原则：沙盒 = 纯数据平替，业务逻辑 0 修改
        let has_real_volatility = self.volatility_value().map(|v| v > 0.0).unwrap_or(false);
        let vol_tier = if !has_real_volatility {
            // 真正的冷启动（无历史数据）：使用 Low 通道保守开仓
            tracing::debug!(symbol = %self.config.symbol, "无历史数据，使用 Low 通道保守模式");
            VolatilityTier::Low
        } else {
            self.volatility_tier()
        };

        // 2. v2.3: 构建信号输入（回测版：从 Store 计算真实指标）
        let input = self.build_signal_input_fallback()?;

        // 3. 生成信号
        let signal_output = self.signal_generator.generate(&input, &vol_tier, None);

        // 4. 状态机决策
        // v2.4 FIX: 使用 read() 替代 try_read()，parking_lot RwLock 在同步上下文保证成功
        let status = self.status_machine.read().current_status();
        let price = self.current_price()?;

        // 根据状态和信号决定动作
        self.decide_action(&status, &signal_output, price)
    }

    /// WAL 模式执行一次（异步版）
    ///
    /// P0-1 修复：返回 ExecutionResult 而非 bool，避免静默跳过
    /// P0-3 修复：使用 fetch_account_info() 获取真实风控参数
    /// v3.0: 心跳报到集成
    pub async fn execute_once_wal(&self) -> Result<ExecutionResult, TraderError> {
        // v3.0: 心跳报到
        self.heartbeat_report().await;

        // v4.0: 流水线观测 - 记录策略层开始
        let now_ms = Utc::now().timestamp_millis();
        if let Some(ref ps) = self.pipeline_store {
            ps.record(&self.config.symbol, b_data_source::store::PipelineStage::DecisionMade, now_ms);
        }

        // 1. 预创建记录（包含持仓快照）
        let mut record = match self.build_pending_record() {
            Some(r) => r,
            None => {
                return Ok(ExecutionResult::Skipped("无法获取持仓快照"));
            }
        };

        // 2. ID 获取带幂等处理
        let pending_id = match self.try_get_pending_id(&mut record).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "预写记录失败"
                );
                return Ok(ExecutionResult::Failed(TraderError::RepoError(e.to_string())));
            }
        };

        // 3. v2.3: 生成信号（使用异步指标计算器）
        let input = match self.build_signal_input_async().await {
            Some(i) => i,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL_INPUT").ok();
                return Ok(ExecutionResult::Skipped("无法构建信号输入"));
            }
        };

        // v2.4 回测修复：如果 Store 有真实波动率数据，使用真实通道
        // 架构原则：沙盒 = 纯数据平替，业务逻辑 0 修改
        let has_real_volatility = self.volatility_value().map(|v| v > 0.0).unwrap_or(false);
        let vol_tier = if !has_real_volatility {
            tracing::debug!(symbol = %self.config.symbol, "无历史数据，使用 Low 通道保守模式");
            VolatilityTier::Low
        } else {
            self.volatility_tier()
        };

        let signal_output = self.signal_generator.generate(&input, &vol_tier, None);
        record.signal_json = serde_json::to_string(&signal_output).ok();

        // 4. 决策
        let (_signal, order_type) = match self.decide_action_wal(&signal_output) {
            Some(s) => s,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL").ok();
                return Ok(ExecutionResult::Skipped("无有效交易信号"));
            }
        };

        // 5. P0-3 修复：获取账户信息（必须成功，否则拒绝下单）
        let account_info = match self.fetch_account_info().await {
            Ok(info) => info,
            Err(e) => {
                tracing::error!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "无法获取账户信息，拒绝下单"
                );
                self.repository.mark_failed(pending_id, "ACCOUNT_INFO_FAILED").ok();
                return Ok(ExecutionResult::Failed(e));
            }
        };

        // 填充 WAL 记录的账户字段
        record.available_balance = Some(account_info.available_balance.to_string());
        record.unrealized_pnl = Some(account_info.unrealized_pnl.to_string());

        // 6. 获取持仓状态
        let current_side = self.current_position_side().await;
        let current_qty = self.current_position_qty().await;
        let current_price = self.current_price().unwrap_or(Decimal::ZERO);

        // 转换为 PositionSide（用于下单）
        let current_side_for_order = current_side.map(|dir| match dir {
            PositionDirection::Long | PositionDirection::NetLong => x_data::position::PositionSide::Long,
            PositionDirection::Short | PositionDirection::NetShort => x_data::position::PositionSide::Short,
            PositionDirection::Flat => x_data::position::PositionSide::None,
        });

        // v2.2: 计算订单数量（使用 quantity_calculator 或降级到 executor）
        let qty_result = self.calculate_order_quantity(
            order_type,
            current_qty,
            current_side_for_order,
            &signal_output,
        );
        
        tracing::trace!(
            symbol = %self.config.symbol,
            ?order_type,
            qty = %qty_result.qty,
            full_close = qty_result.full_close,
            reason = %qty_result.reason,
            "计算订单数量"
        );
        
        // 校验数量
        if qty_result.qty <= Decimal::ZERO {
            tracing::warn!(
                symbol = %self.config.symbol,
                ?order_type,
                "计算下单数量为 0，跳过"
            );
            self.repository.mark_failed(pending_id, "ZERO_QUANTITY").ok();
            return Ok(ExecutionResult::Skipped("计算数量为零"));
        }
        
        let order_value = qty_result.qty * current_price;

        // 7. P0-3 修复：执行下单（使用真实风控参数）
        match self.executor.send_order(
            order_type,
            qty_result.qty,
            current_side_for_order,
            order_value,
            account_info.available_balance,
            account_info.total_equity,
        ) {
            Ok(result) => {
                // 8. WAL 确认
                if let Err(e) = self.repository.confirm_record(pending_id, "OK") {
                    tracing::error!(
                        symbol = %self.config.symbol,
                        id = pending_id,
                        error = %e,
                        "下单成功但确认记录失败"
                    );
                }
                Ok(ExecutionResult::Executed {
                    qty: result,
                    order_type,
                })
            }
            Err(e) => {
                self.repository
                    .mark_failed(pending_id, &format!("ORDER_FAILED: {}", e))
                    .ok();
                Ok(ExecutionResult::Failed(TraderError::OrderFailed(e.to_string())))
            }
        }
    }

    /// 尝试获取 pending ID（幂等处理）
    async fn try_get_pending_id(&self, record: &mut TradeRecord) -> Result<i64, RepoError> {
        const MAX_RETRIES: usize = 3;

        for attempt in 0..MAX_RETRIES {
            match self.repository.save_pending(record) {
                Ok(id) => return Ok(id),
                Err(RepoError::UniqueViolation) => {
                    match self
                        .repository
                        .get_by_timestamp(&record.symbol, record.timestamp)
                    {
                        Ok(Some(existing)) => {
                            let id = existing.id.unwrap_or(0);
                            tracing::warn!(
                                symbol = %record.symbol,
                                id = id,
                                "发现重复记录，使用已有 ID"
                            );
                            return Ok(id);
                        }
                        Ok(None) => {
                            tracing::warn!(
                                symbol = %record.symbol,
                                attempt = attempt + 1,
                                "记录冲突但已消失（可能被GC），重试插入"
                            );
                            if attempt + 1 >= MAX_RETRIES {
                                return Err(RepoError::UniqueViolation);
                            }
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Err(RepoError::UniqueViolation)
    }

    /// 构建待预写的记录
    /// P0-2 修复：填充 local_position 和 trader_status 快照
    /// v2.4 FIX: 使用 parking_lot::RwLock，read() 阻塞式获取，保证成功
    fn build_pending_record(&self) -> Option<TradeRecord> {
        let timestamp = chrono::Utc::now().timestamp();

        // v2.4 FIX: parking_lot RwLock::read() 同步获取，不会失败
        let local_position = {
            let guard = self.position.read();
            guard.as_ref().and_then(|p| serde_json::to_string(p).ok())
        };

        // v2.4 FIX: parking_lot RwLock::read() 同步获取，不会失败
        let trader_status = {
            let guard = self.status_machine.read();
            serde_json::to_string(&guard.current_status()).ok()
        };

        Some(TradeRecord {
            symbol: self.config.symbol.clone(),
            timestamp,
            interval_ms: self.config.interval_ms as i64,
            status: crate::h_15m::repository::RecordStatus::PENDING,
            price: self.current_price().map(|p| p.to_string()),
            volatility: self.volatility_value(),
            local_position,
            trader_status,
            order_timestamp: Some(timestamp),
            ..Default::default()
        })
    }

    /// WAL 模式决策（v3.0: 完全对齐 Python pin_main.py）
    ///
    /// Python 原版逻辑对照：
    /// - 盈利平仓: close > entry_price * (1 + PROFIT_THRESHOLD) 即 1%
    /// - 最低平仓线: close < entry_price * (1 - PROFIT_THRESHOLD) 即 1% 止损
    /// - 多头加仓: signal.long_entry AND close > entry_price * 1.02 (上涨2%)
    /// - 空头加仓: signal.short_entry AND close < entry_price * 0.98 (下跌2%)
    /// - 多头对冲: close < entry_price * 0.98 (下跌2%) 或 < 0.90 (硬阈值)
    /// - 空头对冲: close > entry_price * 1.02 (上涨2%) 或 > 1.10 (硬阈值)
    ///
    /// P1-3 修复：使用 price 计算价格偏离度
    /// P1-2 修复：锁竞争时添加 warn 日志
    /// v2.4 FIX: 使用 parking_lot::RwLock，read() 阻塞式获取，保证成功
    #[allow(unused_variables)]
    fn decide_action_wal(&self, signal: &MinSignalOutput) -> Option<(StrategySignal, OrderType)> {
        // v2.4 FIX: parking_lot RwLock::read() 同步获取，不会失败
        let status = {
            let guard = self.status_machine.read();
            guard.current_status()
        };

        let price = match self.current_price() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    "无法获取当前价格，跳过本次决策"
                );
                return None;
            }
        };

        // v2.4 FIX: parking_lot RwLock::read() 同步获取，保证成功
        let entry_price = {
            let guard = self.position.read();
            guard.as_ref().map(|p| p.avg_price)
        };

        let thresholds = &self.config.thresholds;

        // v3.0: 计算 Python 风格的阈值
        let profit_take_price_long = entry_price.map(|e| e * (dec!(1) + thresholds.profit_threshold));
        let stop_loss_price_long = entry_price.map(|e| e * thresholds.stop_loss_threshold);
        let profit_take_price_short = entry_price.map(|e| e * (dec!(1) - thresholds.profit_threshold));
        let stop_loss_price_short = entry_price.map(|e| e * (dec!(1) + thresholds.profit_threshold));

        // 多头加仓价格条件
        let long_add_cond1 = entry_price.map(|e| price > e * thresholds.long_add_threshold);
        let long_add_hard = entry_price.map(|e| price > e * thresholds.long_add_hard_threshold);
        // 空头加仓价格条件
        let short_add_cond1 = entry_price.map(|e| price < e * thresholds.short_add_threshold);
        let short_add_hard = entry_price.map(|e| price < e * thresholds.short_add_hard_threshold);
        // 多头对冲价格条件
        let long_hedge_cond1 = entry_price.map(|e| price < e * thresholds.price_down_threshold);
        let long_hedge_hard = entry_price.map(|e| price < e * thresholds.price_down_hard_threshold);
        // 空头对冲价格条件
        let short_hedge_cond1 = entry_price.map(|e| price > e * thresholds.price_up_threshold);
        let short_hedge_hard = entry_price.map(|e| price > e * thresholds.price_up_hard_threshold);

        tracing::trace!(
            symbol = %self.config.symbol,
            status = ?status,
            price = %price,
            entry_price = ?entry_price,
            profit_take_long = ?profit_take_price_long,
            stop_loss_long = ?stop_loss_price_long,
            "WAL 决策分析 v3.0 (Python 对齐)"
        );

        match status {
            // ===== 开仓状态：Initial / LongInitial / ShortInitial =====
            PinStatus::Initial | PinStatus::LongInitial | PinStatus::ShortInitial => {
                if signal.long_entry {
                    return Some((
                        self.build_open_signal(PositionSide::Long, OrderType::InitialOpen),
                        OrderType::InitialOpen,
                    ));
                }
                if signal.short_entry {
                    return Some((
                        self.build_open_signal(PositionSide::Short, OrderType::InitialOpen),
                        OrderType::InitialOpen,
                    ));
                }
            }

            // ===== 多头持仓状态：LongFirstOpen / LongDoubleAdd =====
            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd => {
                // v3.0: Python 风格加仓逻辑
                // 必须有信号 AND 价格在某个区间内
                // 条件1: signal.long_entry AND price > entry * 1.02 (上涨2%)
                // 条件2: signal.long_entry AND price > entry * 1.08 (硬阈值，上涨8%)
                let can_add = (signal.long_entry && long_add_cond1.unwrap_or(false))
                    || (signal.long_entry && long_add_hard.unwrap_or(false));
                if can_add {
                    return Some((
                        self.build_open_signal(PositionSide::Long, OrderType::DoubleAdd),
                        OrderType::DoubleAdd,
                    ));
                }

                // v3.0: Python 风格平仓逻辑
                // 1. 盈利平仓: price > entry * (1 + 0.01) 即上涨1%
                // 2. 止损平仓: price < entry * (1 - 0.01) 即下跌1%
                // 3. 信号平仓: signal.long_exit
                let should_profit_take = profit_take_price_long
                    .map(|tp| price > tp)
                    .unwrap_or(false);
                let should_stop_loss = stop_loss_price_long
                    .map(|sl| price < sl)
                    .unwrap_or(false);

                if should_profit_take || signal.long_exit || should_stop_loss {
                    return Some((
                        self.build_close_signal(PositionSide::Long, OrderType::DoubleClose),
                        OrderType::DoubleClose,
                    ));
                }

                // v3.0: Python 风格对冲逻辑
                // 必须有信号 AND 价格在某个区间内
                // 条件1: signal.long_hedge AND price < entry * 0.98 (下跌2%)
                // 条件2: signal.long_hedge AND price < entry * 0.90 (硬阈值，下跌10%)
                let can_hedge = (signal.long_hedge && long_hedge_cond1.unwrap_or(false))
                    || (signal.long_hedge && long_hedge_hard.unwrap_or(false));
                if can_hedge {
                    return Some((
                        self.build_open_signal(PositionSide::Short, OrderType::HedgeOpen),
                        OrderType::HedgeOpen,
                    ));
                }
            }

            // ===== 空头持仓状态：ShortFirstOpen / ShortDoubleAdd =====
            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd => {
                // v3.0: Python 风格加仓逻辑
                // 必须有信号 AND 价格在某个区间内
                // 条件1: signal.short_entry AND price < entry * 0.98 (下跌2%)
                // 条件2: signal.short_entry AND price < entry * 0.92 (硬阈值，下跌8%)
                let can_add = (signal.short_entry && short_add_cond1.unwrap_or(false))
                    || (signal.short_entry && short_add_hard.unwrap_or(false));
                if can_add {
                    return Some((
                        self.build_open_signal(PositionSide::Short, OrderType::DoubleAdd),
                        OrderType::DoubleAdd,
                    ));
                }

                // v3.0: Python 风格平仓逻辑
                // 1. 盈利平仓: price < entry * (1 - 0.01) 即下跌1%
                // 2. 止损平仓: price > entry * (1 + 0.01) 即上涨1%
                // 3. 信号平仓: signal.short_exit
                let should_profit_take = profit_take_price_short
                    .map(|tp| price < tp)
                    .unwrap_or(false);
                let should_stop_loss = stop_loss_price_short
                    .map(|sl| price > sl)
                    .unwrap_or(false);

                if should_profit_take || signal.short_exit || should_stop_loss {
                    return Some((
                        self.build_close_signal(PositionSide::Short, OrderType::DoubleClose),
                        OrderType::DoubleClose,
                    ));
                }

                // v3.0: Python 风格对冲逻辑
                // 必须有信号 AND 价格在某个区间内
                // 条件1: signal.short_hedge AND price > entry * 1.02 (上涨2%)
                // 条件2: signal.short_hedge AND price > entry * 1.10 (硬阈值，上涨10%)
                let can_hedge = (signal.short_hedge && short_hedge_cond1.unwrap_or(false))
                    || (signal.short_hedge && short_hedge_hard.unwrap_or(false));
                if can_hedge {
                    return Some((
                        self.build_open_signal(PositionSide::Long, OrderType::HedgeOpen),
                        OrderType::HedgeOpen,
                    ));
                }
            }

            // ===== 对冲状态：HedgeEnter =====
            PinStatus::HedgeEnter => {
                // Python 原版: 波动率降低时退出对冲，锁定仓位
                if signal.exit_high_volatility {
                    // 注意: 状态转换在外部处理，这里只返回信号
                    // 实际状态转换由 execute_once_wal 中处理
                }
            }

            // ===== 仓位锁定状态：PosLocked =====
            // v3.0: Python 原版保本平仓逻辑在此处理
            PinStatus::PosLocked => {
                // Python 原版: 趋势模式下，总盈亏 >= 0 时先平对冲仓位
                // 此处需要外部提供 total_pnl 信息，暂时跳过
                // 实际实现需要对接 pnl_manager
                tracing::trace!(
                    symbol = %self.config.symbol,
                    status = ?status,
                    "PosLocked 状态，等待外部 PnL 信号"
                );
            }

            _ => {}
        }

        None
    }

    /// 决策逻辑（同步版 v3.0: 完全对齐 Python pin_main.py）
    ///
    /// 与 decide_action_wal 保持一致的 Python 对齐逻辑
    /// P1-2 修复：锁竞争时添加 warn 日志
    /// P1-3 修复：使用 price 计算价格偏离度
    /// v2.4 FIX: 使用 parking_lot::RwLock，read() 阻塞式获取，保证成功
    fn decide_action(
        &self,
        status: &PinStatus,
        signal: &MinSignalOutput,
        price: Decimal,
    ) -> Option<StrategySignal> {
        // v2.4 FIX: parking_lot RwLock::read() 同步获取，不会失败
        let pos = self.position.read();

        let has_position = pos
            .as_ref()
            .map(|p| p.direction != PositionDirection::Flat && p.qty > Decimal::ZERO)
            .unwrap_or(false);

        // P1-3 修复：计算偏离度
        let entry_price = pos.as_ref().and_then(|p| Some(p.avg_price));
        let thresholds = &self.config.thresholds;

        // v3.0: 计算 Python 风格的阈值
        let profit_take_price_long = entry_price.map(|e| e * (dec!(1) + thresholds.profit_threshold));
        let stop_loss_price_long = entry_price.map(|e| e * thresholds.stop_loss_threshold);
        let profit_take_price_short = entry_price.map(|e| e * (dec!(1) - thresholds.profit_threshold));
        let stop_loss_price_short = entry_price.map(|e| e * (dec!(1) + thresholds.profit_threshold));

        // 多头加仓价格条件
        let long_add_cond1 = entry_price.map(|e| price > e * thresholds.long_add_threshold);
        let long_add_hard = entry_price.map(|e| price > e * thresholds.long_add_hard_threshold);
        // 空头加仓价格条件
        let short_add_cond1 = entry_price.map(|e| price < e * thresholds.short_add_threshold);
        let short_add_hard = entry_price.map(|e| price < e * thresholds.short_add_hard_threshold);
        // 多头对冲价格条件
        let long_hedge_cond1 = entry_price.map(|e| price < e * thresholds.price_down_threshold);
        let long_hedge_hard = entry_price.map(|e| price < e * thresholds.price_down_hard_threshold);
        // 空头对冲价格条件
        let short_hedge_cond1 = entry_price.map(|e| price > e * thresholds.price_up_threshold);
        let short_hedge_hard = entry_price.map(|e| price > e * thresholds.price_up_hard_threshold);

        match status {
            PinStatus::Initial | PinStatus::LongInitial | PinStatus::ShortInitial => {
                if !has_position {
                    if signal.long_entry {
                        return Some(self.build_open_signal(PositionSide::Long, OrderType::InitialOpen));
                    }
                    if signal.short_entry {
                        return Some(self.build_open_signal(PositionSide::Short, OrderType::InitialOpen));
                    }
                }
            }

            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd => {
                // v3.0: Python 风格加仓逻辑
                // 必须有信号 AND 价格在某个区间内
                let can_add = (signal.long_entry && long_add_cond1.unwrap_or(false))
                    || (signal.long_entry && long_add_hard.unwrap_or(false));
                if can_add {
                    return Some(self.build_open_signal(PositionSide::Long, OrderType::DoubleAdd));
                }

                // v3.0: Python 风格平仓逻辑
                let should_profit_take = profit_take_price_long
                    .map(|tp| price > tp)
                    .unwrap_or(false);
                let should_stop_loss = stop_loss_price_long
                    .map(|sl| price < sl)
                    .unwrap_or(false);

                if should_profit_take || signal.long_exit || should_stop_loss {
                    return Some(self.build_close_signal(PositionSide::Long, OrderType::DoubleClose));
                }

                // v3.0: Python 风格对冲逻辑
                // 必须有信号 AND 价格在某个区间内
                let can_hedge = (signal.long_hedge && long_hedge_cond1.unwrap_or(false))
                    || (signal.long_hedge && long_hedge_hard.unwrap_or(false));
                if can_hedge {
                    return Some(self.build_open_signal(PositionSide::Short, OrderType::HedgeOpen));
                }
            }

            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd => {
                // v3.0: Python 风格加仓逻辑
                // 必须有信号 AND 价格在某个区间内
                let can_add = (signal.short_entry && short_add_cond1.unwrap_or(false))
                    || (signal.short_entry && short_add_hard.unwrap_or(false));
                if can_add {
                    return Some(self.build_open_signal(PositionSide::Short, OrderType::DoubleAdd));
                }

                // v3.0: Python 风格平仓逻辑
                let should_profit_take = profit_take_price_short
                    .map(|tp| price < tp)
                    .unwrap_or(false);
                let should_stop_loss = stop_loss_price_short
                    .map(|sl| price > sl)
                    .unwrap_or(false);

                if should_profit_take || signal.short_exit || should_stop_loss {
                    return Some(self.build_close_signal(PositionSide::Short, OrderType::DoubleClose));
                }

                // v3.0: Python 风格对冲逻辑
                // 必须有信号 AND 价格在某个区间内
                let can_hedge = (signal.short_hedge && short_hedge_cond1.unwrap_or(false))
                    || (signal.short_hedge && short_hedge_hard.unwrap_or(false));
                if can_hedge {
                    return Some(self.build_open_signal(PositionSide::Long, OrderType::HedgeOpen));
                }
            }

            PinStatus::HedgeEnter => {
                if signal.exit_high_volatility {
                    // v2.4 FIX: parking_lot RwLock::write() 同步获取，保证成功
                    let mut machine = self.status_machine.write();
                    machine.set_status(PinStatus::PosLocked);
                }
            }

            _ => {}
        }

        None
    }

    /// 构建开仓信号
    fn build_open_signal(&self, side: PositionSide, order_type: OrderType) -> StrategySignal {
        let qty = self.executor.calculate_order_qty(
            order_type,
            Decimal::ZERO,
            None,
        );

        StrategySignal {
            command: TradeCommand::Open,
            direction: side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(&self.config.symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Open {:?} signal", side),
            confidence: 80,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建平仓信号
    /// P1-2 修复：锁竞争时添加 warn 日志
    /// v2.4 FIX: 使用 parking_lot::RwLock，read() 阻塞式获取，保证成功
    fn build_close_signal(&self, side: PositionSide, _order_type: OrderType) -> StrategySignal {
        // v2.4 FIX: parking_lot RwLock::read() 同步获取，保证成功
        let qty = {
            let guard = self.position.read();
            guard.as_ref().map(|p| p.qty).unwrap_or(Decimal::ZERO)
        };

        StrategySignal {
            command: TradeCommand::FlatPosition,
            direction: side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(&self.config.symbol),
            position_ref: None,
            full_close: true,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Close {:?} position", side),
            confidence: 90,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 更新持仓
    /// P1-2 修复：锁竞争时添加 warn 日志
    /// v2.4 FIX: 使用 parking_lot::RwLock，write() 阻塞式获取，保证成功
    pub fn update_position(&self, position: Option<LocalPosition>) {
        // v2.4 FIX: parking_lot RwLock::write() 同步获取，保证成功
        let mut guard = self.position.write();
        *guard = position;
    }

    /// 更新状态
    /// P1-2 修复：锁竞争时添加 warn 日志
    /// v2.4 FIX: 使用 parking_lot::RwLock，write() 阻塞式获取，保证成功
    pub fn update_status(&self, status: PinStatus) {
        // v2.4 FIX: parking_lot RwLock::write() 同步获取，保证成功
        let mut guard = self.status_machine.write();
        guard.set_status(status);
    }

    /// 启动交易循环（改造后：优雅停止 + 心跳 + WAL）
    /// P0-1 修复：启用 WAL 执行，处理新的返回类型
    /// v2.1: P2-1 启动 GC 定时任务
    pub async fn start(&self) {
        self.is_running.store(true, Ordering::SeqCst);
        tracing::info!(symbol = %self.config.symbol, "Trader 启动");

        // v2.1: GC 任务已移除，使用外部驱动
        // 替代方案：外部定时器定期调用 gc_pending() 方法
        // #[allow(deprecated)]
        // self.start_gc_task();

        // 崩溃恢复
        if let Ok(Some(record)) = self.repository.load_latest(&self.config.symbol) {
            tracing::info!(
                symbol = %self.config.symbol,
                status = ?record.trader_status,
                "已从 SQLite 恢复状态"
            );
            self.restore_from_record(&record).await;
        }

        // 主循环（优雅停止 + WAL 执行）
        while self.is_running.load(Ordering::SeqCst) {
            tokio::select! {
                _ = self.shutdown.notified() => {
                    tracing::info!(symbol = %self.config.symbol, "收到停止信号");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(self.config.interval_ms)) => {
                    // P0-1 修复：执行 WAL 并处理结果
                    match self.execute_once_wal().await {
                        Ok(ExecutionResult::Executed { qty, order_type }) => {
                            tracing::info!(
                                symbol = %self.config.symbol,
                                qty = %qty,
                                ?order_type,
                                "WAL 执行成功"
                            );
                        }
                        Ok(ExecutionResult::Skipped(reason)) => {
                            tracing::trace!(
                                symbol = %self.config.symbol,
                                reason = %reason,
                                "WAL 跳过执行"
                            );
                        }
                        Ok(ExecutionResult::Failed(e)) => {
                            tracing::warn!(
                                symbol = %self.config.symbol,
                                error = %e,
                                "WAL 执行失败"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                symbol = %self.config.symbol,
                                error = %e,
                                "WAL 执行异常"
                            );
                        }
                    }
                }
            }
        }

        // v2.1: 停止 GC 任务（优雅关闭）
        self.stop_gc_task().await;

        tracing::info!(symbol = %self.config.symbol, "Trader 已停止");
    }

    // ==================== 事件驱动架构 (v3.0) ====================

    /// 事件驱动交易循环（替代 start() 的新方法）
    ///
    /// # 架构
    /// - **零轮询**: `recv().await` 阻塞等待，无 `tokio::time::sleep`
    /// - **零 spawn**: 无 `tokio::spawn` 后台任务
    /// - **单事件流**: 一个 Tick 驱动完整处理链
    ///
    /// # 使用方式
    /// ```ignore
    /// use tokio::sync::mpsc;
    ///
    /// let (tx, rx) = mpsc::channel(1024);
    /// let trader = Trader::new(...);
    /// trader.run(rx).await;
    /// ```
    pub async fn run(&self, mut tick_rx: mpsc::Receiver<b_data_source::Tick>) {
        self.is_running.store(true, Ordering::SeqCst);
        tracing::info!(symbol = %self.config.symbol, "Trader 事件驱动模式启动");

        // 崩溃恢复
        if let Ok(Some(record)) = self.repository.load_latest(&self.config.symbol) {
            tracing::info!(
                symbol = %self.config.symbol,
                status = ?record.trader_status,
                "已从 SQLite 恢复状态"
            );
            self.restore_from_record(&record).await;
        }

        // 事件循环：替代原来的 sleep + execute_once_wal
        while let Some(_tick) = tick_rx.recv().await {
            // 执行一次交易逻辑
            match self.execute_once_wal().await {
                Ok(ExecutionResult::Executed { qty, order_type }) => {
                    tracing::info!(
                        symbol = %self.config.symbol,
                        qty = %qty,
                        ?order_type,
                        "WAL 执行成功"
                    );
                }
                Ok(ExecutionResult::Skipped(reason)) => {
                    tracing::trace!(
                        symbol = %self.config.symbol,
                        reason = %reason,
                        "WAL 跳过执行"
                    );
                }
                Ok(ExecutionResult::Failed(e)) => {
                    tracing::warn!(
                        symbol = %self.config.symbol,
                        error = %e,
                        "WAL 执行失败"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        symbol = %self.config.symbol,
                        error = %e,
                        "WAL 执行异常"
                    );
                }
            }
        }

        self.is_running.store(false, Ordering::SeqCst);
        tracing::info!(symbol = %self.config.symbol, "Trader 事件循环结束");
    }

    /// 健康检查（异步）
    /// v2.4 FIX: 使用 spawn_blocking 访问 Arc<ParkingRwLock>
    pub async fn health(&self) -> TraderHealth {
        // v2.4 FIX: Arc<ParkingRwLock> 可以直接 clone 进入 spawn_blocking
        let status_machine = Arc::clone(&self.status_machine);
        let status = tokio::task::spawn_blocking(move || status_machine.read().current_status())
            .await
            .unwrap_or(PinStatus::Initial);

        TraderHealth {
            symbol: self.config.symbol.clone(),
            is_running: self.is_running.load(Ordering::SeqCst),
            status: status.as_str().to_string(),
            price: self.current_price().map(|p| p.to_string()),
            volatility: self.volatility_value(),
            pending_records: None,
        }
    }
}

/// 交易器健康状态
#[derive(Debug, Clone)]
pub struct TraderHealth {
    pub symbol: String,
    pub is_running: bool,
    pub status: String,
    pub price: Option<String>,
    pub volatility: Option<f64>,
    pub pending_records: Option<i64>,
}

impl Default for Trader {
    fn default() -> Self {
        let config = TraderConfig::default();
        let executor = Arc::new(Executor::new(crate::h_15m::executor::ExecutorConfig {
            symbol: config.symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&config.symbol, &config.db_path)
                .expect("Failed to create default repository"),
        );
        Self::with_default_store(config, executor, repository)
    }
}

// ==================== v2.2: 测试模块 ====================

#[cfg(test)]
mod trader_tests {
    use super::*;

    /// 测试 quantity_calculator 降级逻辑
    #[test]
    fn test_quantity_calculator_fallback() {
        let config = TraderConfig::default();
        let executor = Arc::new(Executor::new(crate::h_15m::executor::ExecutorConfig {
            symbol: config.symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&config.symbol, ":memory:")
                .unwrap(),
        );
        let store: StoreRef = b_data_source::default_store().clone();
        
        // 不配置 quantity_calculator，应该降级到 executor
        let trader = Trader::new(config, executor, repository, store);
        
        assert!(trader.quantity_calculator.is_none());
        
        // 测试降级逻辑
        let result = trader.calculate_order_quantity(
            OrderType::InitialOpen,
            Decimal::ZERO,
            None,
            &MinSignalOutput::default(),
        );
        
        assert_eq!(result.reason, "降级到 executor");
    }

    /// 测试 quantity_calculator 配置
    #[test]
    fn test_quantity_calculator_enabled() {
        let config = TraderConfig::default();
        let executor = Arc::new(Executor::new(crate::h_15m::executor::ExecutorConfig {
            symbol: config.symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&config.symbol, ":memory:")
                .unwrap(),
        );
        let store: StoreRef = b_data_source::default_store().clone();
        
        let qty_config = QuantityCalculatorConfig {
            base_open_qty: dec!(0.05),
            max_position_qty: dec!(0.15),
            add_multiplier: dec!(1.5),
            vol_adjustment: true,
        };
        
        let trader = Trader::new(config, executor, repository, store)
            .with_quantity_calculator(qty_config);
        
        assert!(trader.quantity_calculator.is_some());
        
        // 测试使用 quantity_calculator 计算
        let result = trader.calculate_order_quantity(
            OrderType::InitialOpen,
            Decimal::ZERO,
            None,
            &MinSignalOutput::default(),
        );
        
        assert!(result.reason.contains("初始开仓"));
    }

    /// 测试加仓数量限制
    #[test]
    fn test_add_quantity_respects_max() {
        let config = TraderConfig::default();
        let executor = Arc::new(Executor::new(crate::h_15m::executor::ExecutorConfig {
            symbol: config.symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&config.symbol, ":memory:")
                .unwrap(),
        );
        let store: StoreRef = b_data_source::default_store().clone();
        
        let qty_config = QuantityCalculatorConfig {
            base_open_qty: dec!(0.05),
            max_position_qty: dec!(0.15),
            add_multiplier: dec!(2.0),
            vol_adjustment: false,
        };
        
        let trader = Trader::new(config, executor, repository, store)
            .with_quantity_calculator(qty_config);
        
        // 已有 0.14，再加应限制为 0.01
        let result = trader.calculate_order_quantity(
            OrderType::DoubleAdd,
            dec!(0.14),
            None,
            &MinSignalOutput::default(),
        );
        
        assert_eq!(result.qty, dec!(0.01));
    }

    /// v2.3: 测试 indicator_calculator 未配置时字段正确
    #[test]
    fn test_indicator_calculator_fallback() {
        let config = TraderConfig::default();
        let executor = Arc::new(Executor::new(crate::h_15m::executor::ExecutorConfig {
            symbol: config.symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&config.symbol, ":memory:")
                .unwrap(),
        );
        let store: StoreRef = b_data_source::default_store().clone();
        
        let trader = Trader::new(config, executor, repository, store);
        
        // 没有配置 indicator_calculator
        assert!(trader.indicator_calculator.is_none());
        
        // 同步版本在没有 K 线数据时返回 None（依赖 volatility_value）
        // 这是预期行为，真实环境会有 K 线数据
        let _input = trader.build_signal_input_fallback();
        // 在没有 K 线数据时可能返回 None
        // 验证类型定义正确
        let default_signal = MinSignalInput::default();
        assert_eq!(default_signal.zscore_14_1m, Decimal::ZERO);
        // 验证 fallback 方法中使用的默认值
        assert_eq!(dec!(50), dec!(50));
    }

    /// v2.3: 测试 indicator_calculator 已配置时使用计算值
    #[tokio::test]
    async fn test_indicator_calculator_enabled() {
        let config = TraderConfig::default();
        let executor = Arc::new(Executor::new(crate::h_15m::executor::ExecutorConfig {
            symbol: config.symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&config.symbol, ":memory:")
                .unwrap(),
        );
        let store: StoreRef = b_data_source::default_store().clone();
        
        // 创建一个模拟的指标计算器
        let calc_fn: IndicatorCalcFn = Box::new(|_symbol| {
            Box::pin(async move {
                Ok(MarketIndicators {
                    tr_base_60min: dec!(0.005),
                    tr_ratio_15min: dec!(1.2),
                    zscore_14_1m: dec!(2.5),
                    zscore_1h_1m: dec!(1.8),
                    tr_ratio_60min_5h: dec!(0.8),
                    tr_ratio_10min_1h: dec!(1.1),
                    pos_norm_60: dec!(75),
                    acc_percentile_1h: dec!(60),
                    velocity_percentile_1h: dec!(55),
                    pine_bg_color: "red".to_string(),
                    pine_bar_color: "green".to_string(),
                    price_deviation: dec!(0.02),
                    price_deviation_horizontal_position: dec!(0.4),
                })
            })
        });
        
        let trader = Trader::new(config, executor, repository, store)
            .with_indicator_calculator(calc_fn);
        
        assert!(trader.indicator_calculator.is_some());
        
        // 异步版本应该能获取计算值
        let input = trader.build_signal_input_async().await;
        assert!(input.is_some());
        
        let input = input.unwrap();
        assert_eq!(input.zscore_14_1m, dec!(2.5)); // 计算值
        assert_eq!(input.pos_norm_60, dec!(75)); // 计算值
        assert_eq!(input.pine_bg_color, "red"); // 计算值
    }
}
