//! h_15m/trader.rs - 品种交易主循环
//!
//! 自循环运行，从 MarketDataStore 读取数据，生成交易信号
//! 配置自动读写 SQLite

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use b_data_source::{default_store, MarketDataStore};
use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use rusqlite::{Connection, params};
use tokio::time::sleep;

use crate::types::{MinSignalInput, VolatilityTier};
use crate::h_15m::{MinSignalGenerator, PinStatusMachine, PinStatus};
use x_data::position::{LocalPosition, PositionDirection, PositionSide};
use x_data::trading::signal::{StrategySignal, TradeCommand, StrategyId};

/// 数据库路径
fn get_db_path() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("trading_data");
    std::fs::create_dir_all(&path).ok();
    path.push("trader_config.db");
    path
}

/// 品种交易器配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraderConfig {
    pub symbol: String,
    pub interval_ms: u64,
    pub max_position: Decimal,
    pub initial_ratio: Decimal,
    pub status: String,
}

impl Default for TraderConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 100,
            max_position: dec!(0.15),
            initial_ratio: dec!(0.05),
            status: "Initial".to_string(),
        }
    }
}

/// 品种交易器（自循环）
pub struct Trader {
    config: RwLock<TraderConfig>,
    status_machine: RwLock<PinStatusMachine>,
    signal_generator: MinSignalGenerator,
    position: RwLock<Option<LocalPosition>>,
    is_running: RwLock<bool>,
    db_path: PathBuf,
}

impl Trader {
    /// 创建 Trader（从 SQLite 加载配置，没有用默认）
    pub fn new(symbol: &str) -> Self {
        let db_path = get_db_path();
        let config = Self::load_from_sqlite(&db_path, symbol)
            .unwrap_or_else(|| {
                tracing::info!("[Trader {}] No config in SQLite, using default", symbol);
                TraderConfig {
                    symbol: symbol.to_string(),
                    ..Default::default()
                }
            });

        let mut trader = Self {
            config: RwLock::new(config),
            status_machine: RwLock::new(PinStatusMachine::new()),
            signal_generator: MinSignalGenerator::new(),
            position: RwLock::new(None),
            is_running: RwLock::new(false),
            db_path,
        };

        // 从配置恢复状态机状态
        let status = PinStatus::from_str(&trader.config.read().status);
        trader.status_machine.write().set_status(status);

        trader
    }

    /// 从 SQLite 加载配置
    fn load_from_sqlite(db_path: &PathBuf, symbol: &str) -> Option<TraderConfig> {
        let conn = Connection::open(db_path).ok()?;
        
        // 确保表存在
        conn.execute(
            "CREATE TABLE IF NOT EXISTS trader_config (
                symbol TEXT PRIMARY KEY,
                interval_ms INTEGER,
                max_position TEXT,
                initial_ratio TEXT,
                status TEXT
            )",
            [],
        ).ok()?;

        let mut stmt = conn.prepare(
            "SELECT symbol, interval_ms, max_position, initial_ratio, status FROM trader_config WHERE symbol = ?"
        ).ok()?;

        let config = stmt.query_row(params![symbol], |row| {
            Ok(TraderConfig {
                symbol: row.get(0)?,
                interval_ms: row.get(1)?,
                max_position: row.get::<_, String>(2)?.parse().unwrap_or(dec!(0.15)),
                initial_ratio: row.get::<_, String>(3)?.parse().unwrap_or(dec!(0.05)),
                status: row.get(4)?,
            })
        }).ok()?;

        Some(config)
    }

    /// 保存配置到 SQLite
    pub fn save_config(&self) {
        let config = self.config.read();
        if let Ok(conn) = Connection::open(&self.db_path) {
            conn.execute(
                "INSERT OR REPLACE INTO trader_config 
                 (symbol, interval_ms, max_position, initial_ratio, status) 
                 VALUES (?, ?, ?, ?, ?)",
                params![
                    config.symbol,
                    config.interval_ms,
                    config.max_position.to_string(),
                    config.initial_ratio.to_string(),
                    config.status,
                ],
            ).ok();
        }
    }

    /// 获取配置
    pub fn get_config(&self) -> TraderConfig {
        self.config.read().clone()
    }

    /// 更新配置
    pub fn update_config(&self, config: TraderConfig) {
        *self.config.write() = config.clone();
        self.save_config();
    }

    /// 检查是否运行中
    pub fn is_running(&self) -> bool {
        *self.is_running.read()
    }

    /// 启动交易循环（自循环）
    pub async fn start(&self) {
        self.execute().await;
    }

    /// 执行自循环（对外方法）
    pub async fn execute(&self) {
        // 先读取配置，避免在 async 函数中持有 RwLockReadGuard
        let symbol = self.config.read().symbol.clone();
        let interval_ms = self.config.read().interval_ms;
        
        *self.is_running.write() = true;
        tracing::info!("[Trader {}] Started", symbol);
        
        while *self.is_running.read() {
            if let Some(signal) = self.run_once_internal() {
                tracing::info!("[Trader {}] Signal: {:?}", symbol, signal);
            }
            sleep(Duration::from_millis(interval_ms)).await;
        }
        
        tracing::info!("[Trader {}] Stopped", symbol);
    }

    /// 停止交易
    pub fn stop(&self) {
        *self.is_running.write() = false;
    }

    /// 从 Store 获取当前K线
    pub fn get_current_kline(&self) -> Option<b_data_source::ws::kline_1m::ws::KlineData> {
        default_store().get_current_kline(&self.config.read().symbol)
    }

    /// 从 Store 获取波动率
    pub fn get_volatility(&self) -> Option<b_data_source::store::VolatilityData> {
        default_store().get_volatility(&self.config.read().symbol)
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
            tr_base_60min: dec!(0.1),      // TODO: 实际计算
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

    /// 执行一次交易逻辑（内部调用）
    fn run_once_internal(&self) -> Option<StrategySignal> {
        // 1. 获取数据
        let _kline = self.get_current_kline()?;
        let vol_tier = self.volatility_tier();
        
        // 2. 构建信号输入
        let input = self.build_signal_input()?;
        
        // 3. 生成信号
        let signal_output = self.signal_generator.generate(&input, &vol_tier, None);
        
        // 4. 状态机决策
        let status = self.status_machine.read().current_status();
        let price = self.current_price()?;
        
        // 根据状态和信号决定动作
        self.decide_action(&status, &signal_output, price)
    }

    /// 决策逻辑
    fn decide_action(
        &self,
        status: &PinStatus,
        signal: &crate::types::MinSignalOutput,
        _price: Decimal,
    ) -> Option<StrategySignal> {
        let pos = self.position.read();
        let has_position = pos.as_ref()
            .map(|p| p.direction != PositionDirection::Flat && p.qty > Decimal::ZERO)
            .unwrap_or(false);
        
        match status {
            // ========== 初始状态 ==========
            PinStatus::Initial | PinStatus::LongInitial | PinStatus::ShortInitial => {
                if !has_position {
                    // 多头开仓信号
                    if signal.long_entry {
                        return Some(self.build_open_signal(PositionSide::Long));
                    }
                    // 空头开仓信号
                    if signal.short_entry {
                        return Some(self.build_open_signal(PositionSide::Short));
                    }
                }
            }
            
            // ========== 多头状态 ==========
            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd => {
                // 翻倍加仓
                if signal.long_entry {
                    return Some(self.build_add_signal(PositionSide::Long));
                }
                // 平仓
                if signal.long_exit {
                    return Some(self.build_close_signal(PositionSide::Long));
                }
                // 对冲
                if signal.long_hedge {
                    return Some(self.build_hedge_signal(PositionSide::Long));
                }
            }
            
            // ========== 空头状态 ==========
            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd => {
                // 翻倍加仓
                if signal.short_entry {
                    return Some(self.build_add_signal(PositionSide::Short));
                }
                // 平仓
                if signal.short_exit {
                    return Some(self.build_close_signal(PositionSide::Short));
                }
                // 对冲
                if signal.short_hedge {
                    return Some(self.build_hedge_signal(PositionSide::Short));
                }
            }
            
            // ========== 对冲状态 ==========
            PinStatus::HedgeEnter => {
                // 退出高波动
                if signal.exit_high_volatility {
                    self.status_machine.write().set_status(PinStatus::PosLocked);
                }
            }
            
            // ========== 仓位锁定 ==========
            PinStatus::PosLocked => {
                // TODO: 日线方向决策
            }
            
            // ========== 日线开放 ==========
            PinStatus::LongDayAllow | PinStatus::ShortDayAllow => {
                // TODO: 日线方向平仓
            }
        }
        
        None
    }

    /// 构建开仓信号
    fn build_open_signal(&self, side: PositionSide) -> StrategySignal {
        let qty = self.calculate_initial_qty();
        
        StrategySignal {
            command: TradeCommand::Open,
            direction: side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(&self.config.read().symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Open {:?} signal", side),
            confidence: 80,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建加仓信号
    fn build_add_signal(&self, side: PositionSide) -> StrategySignal {
        let qty = self.calculate_add_qty();
        
        StrategySignal {
            command: TradeCommand::Add,
            direction: side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(&self.config.read().symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Add {:?} position", side),
            confidence: 70,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建平仓信号
    fn build_close_signal(&self, side: PositionSide) -> StrategySignal {
        let qty = self.position.read()
            .as_ref()
            .map(|p| p.qty)
            .unwrap_or(Decimal::ZERO);
        
        StrategySignal {
            command: TradeCommand::FlatPosition,
            direction: side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(&self.config.read().symbol),
            position_ref: None,
            full_close: true,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Close {:?} position", side),
            confidence: 90,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建对冲信号
    fn build_hedge_signal(&self, existing_side: PositionSide) -> StrategySignal {
        let hedge_side = match existing_side {
            PositionSide::Long => PositionSide::Short,
            PositionSide::Short => PositionSide::Long,
            _ => PositionSide::Long,
        };
        let qty = self.calculate_hedge_qty();
        
        StrategySignal {
            command: TradeCommand::HedgeOpen,
            direction: hedge_side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(&self.config.read().symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Hedge {:?} position", hedge_side),
            confidence: 60,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 计算初始开仓数量
    fn calculate_initial_qty(&self) -> Decimal {
        self.config.read().initial_ratio
    }

    /// 计算加仓数量
    fn calculate_add_qty(&self) -> Decimal {
        self.position.read()
            .as_ref()
            .map(|p| p.qty * dec!(0.5))
            .unwrap_or(self.config.read().initial_ratio)
    }

    /// 计算对冲数量
    fn calculate_hedge_qty(&self) -> Decimal {
        self.position.read()
            .as_ref()
            .map(|p| p.qty * dec!(0.3))
            .unwrap_or(self.config.read().initial_ratio * dec!(0.3))
    }

    /// 更新持仓
    pub fn update_position(&self, position: Option<LocalPosition>) {
        *self.position.write() = position;
    }

    /// 更新状态
    pub fn update_status(&self, status: PinStatus) {
        self.status_machine.write().set_status(status);
    }

    /// 健康检查
    pub fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.read().symbol.clone(),
            is_running: *self.is_running.read(),
            status: self.status_machine.read().current_status().as_str().to_string(),
            price: self.current_price().map(|p| p.to_string()),
            volatility: self.volatility_value(),
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
}

impl Default for Trader {
    fn default() -> Self {
        Self::new("BTCUSDT")
    }
}
