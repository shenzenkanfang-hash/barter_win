//! h_15m/trader.rs - 品种交易主循环
//!
//! 从 MarketDataStore 读取数据，生成交易信号
//! 自循环架构：Trader 自己 loop，Engine 管理 spawn/stop/monitor
//!
//! # 修复记录
//! - v2.0: P0-1 主循环启用、P0-2 local_position 填充、P0-3 风控接入、P1-2 锁日志、P1-3 价格偏离度
//! - v2.1: P2-1 gc_pending 定时调用基础设施

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use parking_lot::Mutex;

use async_trait::async_trait;
use b_data_source::MarketDataStore;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use thiserror::Error;
use tokio::sync::{RwLock as TokioRwLock, Notify};
use tokio::time::interval;
use x_data::position::{LocalPosition, PositionDirection, PositionSide};
use x_data::trading::signal::{StrategyId, StrategySignal, TradeCommand};

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

/// 账户信息提供者 Trait（异步接口）
/// 用于解耦 Trader 与具体账户服务的依赖
#[async_trait]
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

/// 品种交易器
pub struct Trader {
    config: TraderConfig,
    status_machine: TokioRwLock<PinStatusMachine>,
    signal_generator: MinSignalGenerator,
    position: TokioRwLock<Option<LocalPosition>>,
    executor: Arc<Executor>,
    repository: Arc<Repository>,
    store: StoreRef,
    /// P0-3: 账户提供者（必须配置，否则拒绝下单）
    account_provider: Option<Arc<dyn AccountProvider>>,
    last_order_ms: AtomicU64,
    is_running: AtomicBool,
    shutdown: Notify,
    /// v2.1: GC 配置
    gc_config: GcConfig,
    /// v2.1: GC 任务句柄（用于优雅停止）
    /// 使用 Arc<Mutex<Option<...>>> 解决 &self 不可变借用问题
    gc_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// v2.2: P1-2 数量计算器（可选，不配置时使用 executor 默认逻辑）
    quantity_calculator: Option<MinQuantityCalculator>,
}

impl Trader {
    /// 创建 Trader（需要注入 executor、repository 和 store）
    /// P0-3 修复：使用此构造函数时，风控将被禁用（不安全，生产环境禁止）
    /// v2.1: 使用默认 GC 配置
    /// v2.2: quantity_calculator = None，使用 executor 默认逻辑
    pub fn new(
        config: TraderConfig,
        executor: Arc<Executor>,
        repository: Arc<Repository>,
        store: StoreRef,
    ) -> Self {
        Self {
            config: config.clone(),
            status_machine: TokioRwLock::new(PinStatusMachine::new()),
            signal_generator: MinSignalGenerator::new(),
            position: TokioRwLock::new(None),
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
        }
    }

    /// 创建带账户服务的 Trader（推荐）
    /// P0-3 修复：必须配置 AccountProvider 才能下单
    /// v2.1: 使用默认 GC 配置
    /// v2.2: quantity_calculator = None，使用 executor 默认逻辑
    pub fn with_account_provider(
        config: TraderConfig,
        executor: Arc<Executor>,
        repository: Arc<Repository>,
        store: StoreRef,
        account_provider: Arc<dyn AccountProvider>,
    ) -> Self {
        Self {
            config: config.clone(),
            status_machine: TokioRwLock::new(PinStatusMachine::new()),
            signal_generator: MinSignalGenerator::new(),
            position: TokioRwLock::new(None),
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

    /// 获取波动率值
    pub fn volatility_value(&self) -> Option<f64> {
        self.get_volatility().map(|v| v.volatility)
    }

    /// 构建信号输入
    /// P1-1 修复：需要从 store 获取真实市场数据，而非硬编码
    /// TODO: 接入真实的指标计算模块
    fn build_signal_input(&self) -> Option<MinSignalInput> {
        let vol = self.volatility_value()?;

        // TODO: P1-1 修复 - 从 store 和指标缓存获取真实数据
        // 当前硬编码会导致信号失真，以下为临时实现
        Some(MinSignalInput {
            tr_base_60min: dec!(0.1),
            tr_ratio_15min: Decimal::from_f64_retain(vol)?,
            zscore_14_1m: dec!(0),        // TODO: 从指标缓存获取
            zscore_1h_1m: dec!(0),       // TODO: 从指标缓存获取
            tr_ratio_60min_5h: dec!(0),   // TODO: 从指标缓存获取
            tr_ratio_10min_1h: dec!(0),   // TODO: 从指标缓存获取
            pos_norm_60: dec!(50),        // TODO: 从指标缓存获取
            acc_percentile_1h: dec!(0),  // TODO: 从指标缓存获取
            velocity_percentile_1h: dec!(0), // TODO: 从指标缓存获取
            pine_bg_color: String::new(), // TODO: 从指标缓存获取
            pine_bar_color: String::new(), // TODO: 从指标缓存获取
            price_deviation: dec!(0),     // TODO: 基于持仓均价计算
            price_deviation_horizontal_position: dec!(0), // TODO: 从指标缓存获取
        })
    }

    /// 判断波动率通道
    fn volatility_tier(&self) -> VolatilityTier {
        match self.volatility_value() {
            Some(v) if v > 0.15 => VolatilityTier::High,
            Some(v) if v > 0.05 => VolatilityTier::Medium,
            _ => VolatilityTier::Low,
        }
    }

    /// 获取账户信息（必须成功，否则拒绝下单）
    /// P0-3 修复：默认拒绝策略，而非使用危险默认值
    async fn fetch_account_info(&self) -> Result<AccountInfo, TraderError> {
        if let Some(ref provider) = self.account_provider {
            match provider.get_account(&self.config.symbol).await {
                Ok(info) => {
                    tracing::debug!(
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

        tracing::error!(
            symbol = %self.config.symbol,
            "未配置 AccountProvider，无法获取风控参数，拒绝下单"
        );
        Err(TraderError::AccountProviderNotConfigured)
    }

    /// 获取当前持仓方向（异步）
    pub async fn current_position_side(&self) -> Option<PositionDirection> {
        self.position
            .read()
            .await
            .as_ref()
            .map(|p| p.direction)
    }

    /// 获取当前持仓数量（异步）
    pub async fn current_position_qty(&self) -> Decimal {
        self.position
            .read()
            .await
            .as_ref()
            .map(|p| p.qty)
            .unwrap_or_default()
    }

    /// 从记录恢复 Trader 状态（异步）
    pub async fn restore_from_record(&self, record: &TradeRecord) {
        // 恢复状态机
        if let Some(ref status_str) = record.trader_status {
            if let Ok(status) = serde_json::from_str::<PinStatus>(status_str) {
                self.status_machine.write().await.set_status(status);
                tracing::info!(
                    symbol = %self.config.symbol,
                    ?status,
                    "状态机已恢复"
                );
            }
        }

        // 恢复持仓
        if let Some(ref pos_str) = record.local_position {
            if let Ok(position) = serde_json::from_str::<LocalPosition>(pos_str) {
                let qty = position.qty;
                *self.position.write().await = Some(position);
                tracing::info!(
                    symbol = %self.config.symbol,
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
        tracing::debug!(
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

    // ==================== v2.1: P2-1 GC 定时任务 ====================

    /// 启动 GC 定时任务（v2.1: P2-1）
    /// 定时清理超时的 PENDING WAL 记录
    fn start_gc_task(&self) {
        let repo = Arc::clone(&self.repository);
        let timeout_secs = self.gc_config.timeout_secs;
        let interval_secs = self.gc_config.interval_secs;
        let symbol = self.config.symbol.clone();
        let gc_handle = Arc::clone(&self.gc_handle);
        let symbol_for_log = symbol.clone();  // 克隆用于闭包后的日志
        
        let handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));
            
            tracing::debug!(
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
        tracing::debug!(
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
            tracing::debug!(
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
    pub fn execute_once(&self) -> Option<StrategySignal> {
        // 1. 获取数据
        let _kline = self.get_current_kline()?;
        let vol_tier = self.volatility_tier();

        // 2. 构建信号输入
        let input = self.build_signal_input()?;

        // 3. 生成信号
        let signal_output = self.signal_generator.generate(&input, &vol_tier, None);

        // 4. 状态机决策
        let status = self.status_machine.try_read().ok()?.current_status();
        let price = self.current_price()?;

        // 根据状态和信号决定动作
        self.decide_action(&status, &signal_output, price)
    }

    /// WAL 模式执行一次（异步版）
    ///
    /// P0-1 修复：返回 ExecutionResult 而非 bool，避免静默跳过
    /// P0-3 修复：使用 fetch_account_info() 获取真实风控参数
    pub async fn execute_once_wal(&self) -> Result<ExecutionResult, TraderError> {
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

        // 3. 生成信号
        let input = match self.build_signal_input() {
            Some(i) => i,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL_INPUT").ok();
                return Ok(ExecutionResult::Skipped("无法构建信号输入"));
            }
        };

        let signal_output = self.signal_generator.generate(&input, &self.volatility_tier(), None);
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
        
        tracing::debug!(
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
    fn build_pending_record(&self) -> Option<TradeRecord> {
        let timestamp = chrono::Utc::now().timestamp();

        // P1-2 修复：获取持仓快照（添加锁失败日志）
        let local_position = match self.position.try_read() {
            Ok(guard) => {
                guard.as_ref().and_then(|p| serde_json::to_string(p).ok())
            }
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "获取持仓锁失败，local_position 将为 None"
                );
                None
            }
        };

        // 获取状态快照
        let trader_status = match self.status_machine.try_read() {
            Ok(guard) => serde_json::to_string(&guard.current_status()).ok(),
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "获取状态锁失败，trader_status 将为 None"
                );
                None
            }
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

    /// WAL 模式决策
    /// P1-3 修复：使用 price 计算价格偏离度
    /// P1-2 修复：锁竞争时添加 warn 日志
    #[allow(unused_variables)]
    fn decide_action_wal(&self, signal: &MinSignalOutput) -> Option<(StrategySignal, OrderType)> {
        // P1-2 修复：获取状态，锁失败时添加日志
        let status = match self.status_machine.try_read() {
            Ok(guard) => guard.current_status(),
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "获取状态锁失败，跳过本次决策"
                );
                return None;
            }
        };

        // P1-3 修复：price 将用于偏离度计算
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

        // P1-3 修复：计算价格偏离度用于辅助决策
        // 获取持仓均价
        let entry_price = match self.position.try_read() {
            Ok(guard) => guard.as_ref().map(|p| p.avg_price),
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "获取持仓锁失败，无法计算偏离度"
                );
                None
            }
        };

        // 计算偏离度（用于决策参考）
        let price_deviation_pct = entry_price
            .map(|entry| {
                if entry != Decimal::ZERO {
                    ((price - entry) / entry * dec!(100)).round_dp(2)
                } else {
                    Decimal::ZERO
                }
            })
            .unwrap_or(Decimal::ZERO);

        // P1-3 修复：使用偏离度进行辅助决策（平仓时判断是否极端偏离）
        let use_extreme_exit = price_deviation_pct.abs() > dec!(5);

        tracing::debug!(
            symbol = %self.config.symbol,
            status = ?status,
            price = %price,
            entry_price = ?entry_price,
            deviation_pct = %price_deviation_pct,
            use_extreme_exit = use_extreme_exit,
            "WAL 决策分析"
        );

        match status {
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
            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd => {
                if signal.long_entry {
                    return Some((
                        self.build_open_signal(PositionSide::Long, OrderType::DoubleAdd),
                        OrderType::DoubleAdd,
                    ));
                }
                // P1-3 修复：使用价格偏离度辅助平仓决策
                if signal.long_exit || use_extreme_exit {
                    return Some((
                        self.build_close_signal(PositionSide::Long, OrderType::DoubleClose),
                        OrderType::DoubleClose,
                    ));
                }
                if signal.long_hedge {
                    return Some((
                        self.build_open_signal(PositionSide::Short, OrderType::HedgeOpen),
                        OrderType::HedgeOpen,
                    ));
                }
            }
            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd => {
                if signal.short_entry {
                    return Some((
                        self.build_open_signal(PositionSide::Short, OrderType::DoubleAdd),
                        OrderType::DoubleAdd,
                    ));
                }
                // P1-3 修复：使用价格偏离度辅助平仓决策
                if signal.short_exit || use_extreme_exit {
                    return Some((
                        self.build_close_signal(PositionSide::Short, OrderType::DoubleClose),
                        OrderType::DoubleClose,
                    ));
                }
                if signal.short_hedge {
                    return Some((
                        self.build_open_signal(PositionSide::Long, OrderType::HedgeOpen),
                        OrderType::HedgeOpen,
                    ));
                }
            }
            _ => {}
        }

        None
    }

    /// 决策逻辑（同步版）
    /// P1-2 修复：锁竞争时添加 warn 日志
    /// P1-3 修复：使用 price 计算价格偏离度
    fn decide_action(
        &self,
        status: &PinStatus,
        signal: &MinSignalOutput,
        price: Decimal,
    ) -> Option<StrategySignal> {
        // P1-2 修复：获取持仓，锁失败时添加日志
        let pos = match self.position.try_read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "同步决策：获取持仓锁失败，跳过本次决策"
                );
                return None;
            }
        };

        let has_position = pos
            .as_ref()
            .map(|p| p.direction != PositionDirection::Flat && p.qty > Decimal::ZERO)
            .unwrap_or(false);

        // P1-3 修复：计算偏离度
        let entry_price = pos.as_ref().and_then(|p| Some(p.avg_price));
        let price_deviation_pct = entry_price
            .map(|entry| {
                if entry != Decimal::ZERO {
                    ((price - entry) / entry * dec!(100)).round_dp(2)
                } else {
                    Decimal::ZERO
                }
            })
            .unwrap_or(Decimal::ZERO);

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
                if signal.long_entry {
                    return Some(self.build_open_signal(PositionSide::Long, OrderType::DoubleAdd));
                }
                // P1-3 修复：使用偏离度辅助平仓
                if signal.long_exit || price_deviation_pct.abs() > dec!(5) {
                    return Some(self.build_close_signal(PositionSide::Long, OrderType::DoubleClose));
                }
                if signal.long_hedge {
                    return Some(self.build_open_signal(PositionSide::Short, OrderType::HedgeOpen));
                }
            }

            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd => {
                if signal.short_entry {
                    return Some(self.build_open_signal(PositionSide::Short, OrderType::DoubleAdd));
                }
                // P1-3 修复：使用偏离度辅助平仓
                if signal.short_exit || price_deviation_pct.abs() > dec!(5) {
                    return Some(self.build_close_signal(PositionSide::Short, OrderType::DoubleClose));
                }
                if signal.short_hedge {
                    return Some(self.build_open_signal(PositionSide::Long, OrderType::HedgeOpen));
                }
            }

            PinStatus::HedgeEnter => {
                if signal.exit_high_volatility {
                    if let Ok(mut machine) = self.status_machine.try_write() {
                        machine.set_status(PinStatus::PosLocked);
                    }
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
    fn build_close_signal(&self, side: PositionSide, _order_type: OrderType) -> StrategySignal {
        let qty = match self.position.try_read() {
            Ok(guard) => guard.as_ref().map(|p| p.qty).unwrap_or(Decimal::ZERO),
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "构建平仓信号：获取持仓锁失败，使用零数量"
                );
                Decimal::ZERO
            }
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
    pub fn update_position(&self, position: Option<LocalPosition>) {
        match self.position.try_write() {
            Ok(mut guard) => {
                *guard = position;
            }
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "更新持仓：获取写锁失败，跳过更新"
                );
            }
        }
    }

    /// 更新状态
    /// P1-2 修复：锁竞争时添加 warn 日志
    pub fn update_status(&self, status: PinStatus) {
        match self.status_machine.try_write() {
            Ok(mut guard) => {
                guard.set_status(status);
            }
            Err(e) => {
                tracing::warn!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "更新状态：获取写锁失败，跳过更新"
                );
            }
        }
    }

    /// 启动交易循环（改造后：优雅停止 + 心跳 + WAL）
    /// P0-1 修复：启用 WAL 执行，处理新的返回类型
    /// v2.1: P2-1 启动 GC 定时任务
    pub async fn start(&self) {
        self.is_running.store(true, Ordering::SeqCst);
        tracing::info!(symbol = %self.config.symbol, "Trader 启动");

        // v2.1: 启动 GC 定时任务
        self.start_gc_task();

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
                            tracing::debug!(
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

    /// 健康检查（异步）
    pub async fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.symbol.clone(),
            is_running: self.is_running.load(Ordering::SeqCst),
            status: self.status_machine.read().await.current_status().as_str().to_string(),
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
}
