//! f_engine 核心状态管理
//!
//! # 模块说明
//! - `SymbolState`: 品种交易状态
//! - `TradeLock`: 交易锁，防止重复执行
//! - `StartupState`: 启动状态（正常/灾备恢复）
//! - `CheckConfig`: 检查配置（时间窗口）

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// 启动状态
// ============================================================================

/// 启动状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupState {
    /// 正常启动
    Fresh,
    /// 灾备恢复中
    Recovery,
}

impl Default for StartupState {
    fn default() -> Self {
        StartupState::Fresh
    }
}

// ============================================================================
// 交易锁
// ============================================================================

/// 交易锁 - 防止并发重复执行
///
/// 机制：
/// - 获取锁成功时核对时间戳
/// - tick_ts <= lock_ts 说明已被处理过，丢弃
/// - 执行成功后更新锁的时间戳和仓位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeLock {
    /// 锁持有时间戳
    pub timestamp: i64,
    /// 当前持仓数量
    pub position_qty: Decimal,
    /// 持仓平均价格
    pub position_price: Decimal,
    /// 持仓更新时间戳
    pub position_ts: i64,
}

impl Default for TradeLock {
    fn default() -> Self {
        Self {
            timestamp: 0,
            position_qty: Decimal::ZERO,
            position_price: Decimal::ZERO,
            position_ts: 0,
        }
    }
}

impl TradeLock {
    /// 创建新的交易锁
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查 tick 是否过期（已被处理过）
    pub fn is_stale(&self, tick_ts: i64) -> bool {
        tick_ts <= self.timestamp
    }

    /// 更新锁状态
    pub fn update(&mut self, tick_ts: i64, qty: Decimal, price: Decimal) {
        self.timestamp = tick_ts;
        self.position_qty = qty;
        self.position_price = price;
        self.position_ts = chrono::Utc::now().timestamp();
    }

    /// 获取持仓值
    pub fn position_value(&self) -> Decimal {
        self.position_qty * self.position_price
    }
}

// ============================================================================
// 品种状态
// ============================================================================

/// 品种交易状态
///
/// 维护每个交易品种的状态信息：
/// - 交易锁
/// - 指标请求/成功时间戳
/// - 缓存的信号
/// - 启动状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolState {
    /// 品种符号
    pub symbol: String,

    /// 交易锁
    pub trade_lock: TradeLock,

    /// 启动状态
    pub startup_state: StartupState,

    // --- 分钟级状态 ---
    /// 上次分钟级指标请求时间戳
    pub last_1m_request_ts: i64,
    /// 上次分钟级指标成功获取时间戳
    pub last_1m_ok_ts: i64,
    /// 上次获取的分钟级信号时间戳
    pub last_1m_signal_ts: i64,
    /// 缓存的分钟级信号
    pub last_1m_signal: Option<TradingDecision>,

    // --- 日线级状态 ---
    /// 上次日线级指标请求时间戳
    pub last_daily_request_ts: i64,
    /// 上次日线级指标成功获取时间戳
    pub last_daily_ok_ts: i64,
    /// 缓存的日线级信号
    pub last_daily_signal: Option<TradingDecision>,

    /// 超时阈值（秒）
    pub timeout_secs: i64,
}

impl SymbolState {
    /// 创建新的品种状态
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            trade_lock: TradeLock::new(),
            startup_state: StartupState::Fresh,
            last_1m_request_ts: 0,
            last_1m_ok_ts: 0,
            last_1m_signal_ts: 0,
            last_1m_signal: None,
            last_daily_request_ts: 0,
            last_daily_ok_ts: 0,
            last_daily_signal: None,
            timeout_secs: 60, // 默认1分钟超时
        }
    }

    /// 创建灾备恢复状态的品种
    pub fn new_recovery(symbol: String) -> Self {
        Self {
            symbol,
            startup_state: StartupState::Recovery,
            ..Self::new(symbol)
        }
    }

    /// 检查分钟级是否超时
    pub fn is_1m_timeout(&self, now_ts: i64) -> bool {
        if self.last_1m_request_ts == 0 {
            return false; // 从未请求过，不算超时
        }
        now_ts - self.last_1m_request_ts > self.timeout_secs
    }

    /// 检查日线级是否超时
    pub fn is_daily_timeout(&self, now_ts: i64) -> bool {
        if self.last_daily_request_ts == 0 {
            return false;
        }
        now_ts - self.daily_request_ts > self.timeout_secs
    }

    /// 检查信号是否过期（age > timeout）
    pub fn is_signal_stale(&self, signal_ts: i64, now_ts: i64) -> bool {
        now_ts - signal_ts > self.timeout_secs
    }

    /// 设置超时阈值
    pub fn set_timeout(&mut self, secs: i64) {
        self.timeout_secs = secs;
    }

    /// 确认恢复正常
    pub fn confirm_fresh(&mut self) {
        self.startup_state = StartupState::Fresh;
    }

    /// 记录分钟级请求
    pub fn record_1m_request(&mut self, ts: i64) {
        self.last_1m_request_ts = ts;
    }

    /// 记录分钟级成功
    pub fn record_1m_ok(&mut self, ts: i64, signal_ts: i64, signal: TradingDecision) {
        self.last_1m_ok_ts = ts;
        self.last_1m_signal_ts = signal_ts;
        self.last_1m_signal = Some(signal);
    }

    /// 记录日线级请求
    pub fn record_daily_request(&mut self, ts: i64) {
        self.last_daily_request_ts = ts;
    }

    /// 记录日线级成功
    pub fn record_daily_ok(&mut self, ts: i64, signal: TradingDecision) {
        self.last_daily_ok_ts = ts;
        self.last_daily_signal = Some(signal);
    }
}

// ============================================================================
// 检查配置
// ============================================================================

/// 检查配置 - 时间窗口控制
#[derive(Debug, Clone)]
pub struct CheckConfig {
    /// 分钟级检查间隔（毫秒）
    pub minute_check_interval_ms: u64,
    /// 日线级检查间隔（毫秒）
    pub daily_check_interval_ms: u64,
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            minute_check_interval_ms: 1000,  // 默认1秒
            daily_check_interval_ms: 1000,   // 默认1秒
        }
    }
}

impl CheckConfig {
    /// 创建生产配置
    pub fn production() -> Self {
        Self {
            minute_check_interval_ms: 1000,
            daily_check_interval_ms: 1000,
        }
    }

    /// 创建回测配置（更频繁）
    pub fn backtest() -> Self {
        Self {
            minute_check_interval_ms: 100,
            daily_check_interval_ms: 100,
        }
    }
}

// ============================================================================
// 交易决策 (从 c_data_process 导入)
// ============================================================================

pub use c_data_process::types::TradingDecision;
