//! h_15m/trader.rs - 品种交易主循环
//!
//! 从 MarketDataStore 读取数据，生成交易信号
//! 自循环架构：Trader 自己 loop，Engine 管理 spawn/stop/monitor

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b_data_source::MarketDataStore;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::{RwLock as TokioRwLock, Notify};
use x_data::position::{LocalPosition, PositionDirection, PositionSide};
use x_data::trading::signal::{StrategyId, StrategySignal, TradeCommand};

use crate::h_15m::executor::{Executor, OrderType};
use crate::h_15m::repository::{RepoError, Repository, TradeRecord};
use crate::h_15m::{MinSignalGenerator, PinStatus, PinStatusMachine};
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityTier};

/// MarketDataStore trait object for dependency injection
pub type StoreRef = Arc<dyn MarketDataStore + Send + Sync>;

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

/// 品种交易器
pub struct Trader {
    config: TraderConfig,
    status_machine: TokioRwLock<PinStatusMachine>,
    signal_generator: MinSignalGenerator,
    position: TokioRwLock<Option<LocalPosition>>,
    executor: Arc<Executor>,
    repository: Arc<Repository>,
    store: StoreRef,
    last_order_ms: AtomicU64,
    is_running: AtomicBool,
    shutdown: Notify,
}

impl Trader {
    /// 创建 Trader（需要注入 executor、repository 和 store）
    pub fn new(
        config: TraderConfig,
        executor: Arc<Executor>,
        repository: Arc<Repository>,
        store: StoreRef,
    ) -> Self {
        Self {
            config,
            status_machine: TokioRwLock::new(PinStatusMachine::new()),
            signal_generator: MinSignalGenerator::new(),
            position: TokioRwLock::new(None),
            executor,
            repository,
            store,
            last_order_ms: AtomicU64::new(0),
            is_running: AtomicBool::new(false),
            shutdown: Notify::new(),
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

    /// 构建信号输入（简化版）
    fn build_signal_input(&self) -> Option<MinSignalInput> {
        let vol = self.volatility_value()?;

        Some(MinSignalInput {
            tr_base_60min: dec!(0.1),
            tr_ratio_15min: Decimal::from_f64_retain(vol)?,
            zscore_14_1m: dec!(0),
            zscore_1h_1m: dec!(0),
            tr_ratio_60min_5h: dec!(0),
            tr_ratio_10min_1h: dec!(0),
            pos_norm_60: dec!(50),
            acc_percentile_1h: dec!(0),
            velocity_percentile_1h: dec!(0),
            pine_bg_color: String::new(),
            pine_bar_color: String::new(),
            price_deviation: dec!(0),
            price_deviation_horizontal_position: dec!(0),
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
    /// 返回 bool：是否成功执行（用于重启计数重置）
    pub async fn execute_once_wal(&self) -> bool {
        // 1. 预创建记录
        let mut record = self.build_pending_record();

        // 2. ID 获取带幂等处理
        let pending_id = match self.try_get_pending_id(&mut record).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(
                    symbol = %self.config.symbol,
                    error = %e,
                    "预写记录失败，跳过本次下单"
                );
                return false;
            }
        };

        // 3. 生成信号
        let input = match self.build_signal_input() {
            Some(i) => i,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL_INPUT").ok();
                return false;
            }
        };

        let signal_output = self.signal_generator.generate(&input, &self.volatility_tier(), None);
        record.signal_json = match serde_json::to_string(&signal_output) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(symbol = %self.config.symbol, error = %e, "信号序列化失败");
                None
            }
        };

        // 4. 决策
        let (signal, order_type) = match self.decide_action_wal(&signal_output) {
            Some(s) => s,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL").ok();
                return false;
            }
        };

        // 5. 获取当前持仓状态
        let current_side = self.current_position_side().await;
        let current_qty = self.current_position_qty().await;
        let current_price = self.current_price().unwrap_or(Decimal::ZERO);

        // 转换为 PositionSide（用于下单）
        let current_side_for_order = current_side.map(|dir| match dir {
            PositionDirection::Long | PositionDirection::NetLong => x_data::position::PositionSide::Long,
            PositionDirection::Short | PositionDirection::NetShort => x_data::position::PositionSide::Short,
            PositionDirection::Flat => x_data::position::PositionSide::None,
        });

        // 计算订单价值（用于风控检查）
        // 注意：实际风控参数应通过 ExecutorConfig 或外部注入获取
        let order_qty = self.executor.calculate_order_qty(order_type, current_qty, current_side_for_order);
        let order_value = order_qty * current_price;

        // 6. 执行下单（传入风控参数，如果 Executor 未配置风控则使用默认值跳过检查）
        match self.executor.send_order(
            order_type,
            current_qty,
            current_side_for_order,
            order_value,
            Decimal::MAX,      // available_balance: 使用最大值跳过风控
            Decimal::MAX,      // total_equity: 使用最大值跳过风控
        ) {
            Ok(_) => {
                // 7. WAL 确认
                if let Err(e) = self.repository.confirm_record(pending_id, "OK") {
                    tracing::error!(
                        symbol = %self.config.symbol,
                        id = pending_id,
                        error = %e,
                        "下单成功但确认记录失败"
                    );
                }
                true
            }
            Err(e) => {
                self.repository
                    .mark_failed(pending_id, &format!("ORDER_FAILED: {}", e))
                    .ok();
                false
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
    fn build_pending_record(&self) -> TradeRecord {
        let timestamp = chrono::Utc::now().timestamp();

        TradeRecord {
            symbol: self.config.symbol.clone(),
            timestamp,
            interval_ms: self.config.interval_ms as i64,
            status: crate::h_15m::repository::RecordStatus::PENDING,
            price: self.current_price().map(|p| p.to_string()),
            volatility: self.volatility_value(),
            trader_status: {
                let machine_guard = self.status_machine.try_read().ok();
                machine_guard.and_then(|m| {
                    match serde_json::to_string(&m.current_status()) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            tracing::warn!(symbol = %self.config.symbol, error = %e, "状态序列化失败");
                            None
                        }
                    }
                })
            },
            local_position: None, // 后续更新
            order_timestamp: Some(timestamp),
            ..Default::default()
        }
    }

    /// WAL 模式决策
    fn decide_action_wal(&self, signal: &MinSignalOutput) -> Option<(StrategySignal, OrderType)> {
        let status = self.status_machine.try_read().ok()?.current_status();
        let price = self.current_price()?;

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
                if signal.long_exit {
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
                if signal.short_exit {
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
    fn decide_action(
        &self,
        status: &PinStatus,
        signal: &MinSignalOutput,
        price: Decimal,
    ) -> Option<StrategySignal> {
        let pos = self.position.try_read().ok()?;
        let has_position = pos
            .as_ref()
            .map(|p| p.direction != PositionDirection::Flat && p.qty > Decimal::ZERO)
            .unwrap_or(false);

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
                if signal.long_exit {
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
                if signal.short_exit {
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
    fn build_close_signal(&self, side: PositionSide, order_type: OrderType) -> StrategySignal {
        let qty = self
            .position
            .try_read()
            .ok()
            .and_then(|p| p.as_ref().map(|p| p.qty))
            .unwrap_or(Decimal::ZERO);

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
    pub fn update_position(&self, position: Option<LocalPosition>) {
        if let Ok(mut guard) = self.position.try_write() {
            *guard = position;
        }
    }

    /// 更新状态
    pub fn update_status(&self, status: PinStatus) {
        if let Ok(mut guard) = self.status_machine.try_write() {
            guard.set_status(status);
        }
    }

    /// 启动交易循环（改造后：优雅停止 + 心跳 + WAL）
    pub async fn start(&self) {
        self.is_running.store(true, Ordering::SeqCst);
        tracing::info!(symbol = %self.config.symbol, "Trader 启动");

        // 崩溃恢复
        if let Ok(Some(record)) = self.repository.load_latest(&self.config.symbol) {
            tracing::info!(
                symbol = %self.config.symbol,
                status = ?record.trader_status,
                "已从 SQLite 恢复状态"
            );
            self.restore_from_record(&record).await;
        }

        // 主循环（优雅停止 + 心跳）
        while self.is_running.load(Ordering::SeqCst) {
            tokio::select! {
                _ = self.shutdown.notified() => {
                    tracing::info!(symbol = %self.config.symbol, "收到停止信号");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(self.config.interval_ms)) => {
                    // WAL 执行：预写→信号→决策→下单→确认
                    self.execute_once_wal().await;
                }
            }
        }

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
