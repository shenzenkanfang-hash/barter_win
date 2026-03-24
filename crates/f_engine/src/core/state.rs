//! f_engine 核心状态管理
//!
//! # 设计原则
//! - 所有字段**私有化**
//! - 所有访问通过**方法**
//! - 模块间调用必须走接口
//!
//! # 模块说明
//! - `SymbolState`: 品种交易状态（含指标）
//! - `SymbolMetrics`: 品种级运行指标
//! - `TradeLock`: 交易锁，防止重复执行
//! - `StartupState`: 启动状态（正常/灾备恢复）
//! - `CheckConfig`: 检查配置（时间窗口）

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::TradingDecision;

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

/// 交易锁 - 品种级防止并发重复执行（V1.4）
///
/// # 机制
/// - `try_lock(timeout_secs)`: 尝试获取锁，超时则失败
/// - `is_locked()`: 检查锁是否有效
/// - `unlock()`: 释放锁
/// - 锁范围：只包住「状态比对 + 落地」，不包住下单
///
/// # V1.4 要求
/// - 锁粒度：品种级独立锁
/// - 锁超时：1s，拿不到直接拒单
/// - 锁范围：只包住「状态比对 + 落地」，不包住下单
///
/// # 模块间调用规则
/// ⚠️ 禁止直接访问字段，必须使用方法
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeLock {
    /// 锁持有时间戳（最后更新时间）
    timestamp: i64,
    /// 锁过期时间戳（0 表示未锁定）
    lock_until: i64,
    /// 当前持仓数量
    position_qty: Decimal,
    /// 持仓平均价格
    position_price: Decimal,
    /// 持仓更新时间戳
    position_ts: i64,
}

impl Default for TradeLock {
    fn default() -> Self {
        Self {
            timestamp: 0,
            lock_until: 0,
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

    /// 尝试获取锁（V1.4 核心）
    ///
    /// # 参数
    /// - `timeout_secs`: 超时时间（默认 1 秒）
    ///
    /// # 返回
    /// - `true`: 获取锁成功
    /// - `false`: 获取锁失败（已被锁定或超时）
    pub fn try_lock(&mut self, timeout_secs: i64) -> bool {
        let now = chrono::Utc::now().timestamp();

        // 如果锁已过期，自动解锁
        if self.lock_until > 0 && now >= self.lock_until {
            self.lock_until = 0;
        }

        // 如果未锁定，直接获取锁
        if self.lock_until == 0 {
            self.lock_until = now + timeout_secs;
            return true;
        }

        // 如果已锁定，检查是否超时
        if now >= self.lock_until {
            self.lock_until = now + timeout_secs;
            return true;
        }

        // 锁仍然有效，获取失败
        false
    }

    /// 检查锁是否有效
    pub fn is_locked(&self) -> bool {
        if self.lock_until == 0 {
            return false;
        }
        let now = chrono::Utc::now().timestamp();
        now < self.lock_until
    }

    /// 释放锁
    pub fn unlock(&mut self) {
        self.lock_until = 0;
    }

    /// 检查 tick 是否过期（已被处理过）
    pub fn is_stale(&self, tick_ts: i64) -> bool {
        tick_ts <= self.timestamp
    }

    /// 获取锁持有时间戳
    pub fn timestamp(&self) -> i64 {
        self.timestamp
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

    /// 获取持仓数量
    pub fn position_qty(&self) -> Decimal {
        self.position_qty
    }

    /// 获取持仓价格
    pub fn position_price(&self) -> Decimal {
        self.position_price
    }

    /// 获取持仓更新时间戳
    pub fn position_ts(&self) -> i64 {
        self.position_ts
    }
}

// ============================================================================
// 品种指标
// ============================================================================

/// 品种级运行指标
///
/// 所有字段私有化，通过方法访问
/// 计数器使用 AtomicU64 保证线程安全
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolMetrics {
    /// 累计处理 tick 数
    tick_processed: AtomicU64,
    /// 累计生成信号数
    signal_generated: AtomicU64,
    /// 累计下单数
    order_sent: AtomicU64,
    /// 累计成交数
    order_filled: AtomicU64,
    /// 累计失败数
    order_failed: AtomicU64,
    /// 最后信号时间
    last_signal_time: Option<DateTime<Utc>>,
    /// 最后下单时间
    last_order_time: Option<DateTime<Utc>>,
}

impl Clone for SymbolMetrics {
    fn clone(&self) -> Self {
        Self {
            tick_processed: AtomicU64::new(self.tick_processed.load(Ordering::SeqCst)),
            signal_generated: AtomicU64::new(self.signal_generated.load(Ordering::SeqCst)),
            order_sent: AtomicU64::new(self.order_sent.load(Ordering::SeqCst)),
            order_filled: AtomicU64::new(self.order_filled.load(Ordering::SeqCst)),
            order_failed: AtomicU64::new(self.order_failed.load(Ordering::SeqCst)),
            last_signal_time: self.last_signal_time,
            last_order_time: self.last_order_time,
        }
    }
}

impl Default for SymbolMetrics {
    fn default() -> Self {
        Self::new("")
    }
}

impl SymbolMetrics {
    /// 创建新的品种指标
    pub fn new(_symbol: &str) -> Self {
        Self {
            tick_processed: AtomicU64::new(0),
            signal_generated: AtomicU64::new(0),
            order_sent: AtomicU64::new(0),
            order_filled: AtomicU64::new(0),
            order_failed: AtomicU64::new(0),
            last_signal_time: None,
            last_order_time: None,
        }
    }

    // ─────────────────────────────────────────────────────────
    // 查询方法
    // ─────────────────────────────────────────────────────────

    /// tick 处理数
    pub fn tick_processed(&self) -> u64 {
        self.tick_processed.load(Ordering::SeqCst)
    }

    /// 信号生成数
    pub fn signal_generated(&self) -> u64 {
        self.signal_generated.load(Ordering::SeqCst)
    }

    /// 订单发送数
    pub fn order_sent(&self) -> u64 {
        self.order_sent.load(Ordering::SeqCst)
    }

    /// 订单成交数
    pub fn order_filled(&self) -> u64 {
        self.order_filled.load(Ordering::SeqCst)
    }

    /// 订单失败数
    pub fn order_failed(&self) -> u64 {
        self.order_failed.load(Ordering::SeqCst)
    }

    /// 最后信号时间
    pub fn last_signal_time(&self) -> Option<DateTime<Utc>> {
        self.last_signal_time
    }

    /// 最后下单时间
    pub fn last_order_time(&self) -> Option<DateTime<Utc>> {
        self.last_order_time
    }

    /// 获取成交率
    pub fn fill_rate(&self) -> f64 {
        let sent = self.order_sent();
        if sent == 0 {
            return 0.0;
        }
        self.order_filled() as f64 / sent as f64
    }

    // ─────────────────────────────────────────────────────────
    // 更新方法（线程安全）
    // ─────────────────────────────────────────────────────────

    /// 记录 tick 处理
    pub fn record_tick(&self) {
        self.tick_processed.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录信号生成
    pub fn record_signal(&self) {
        self.signal_generated.fetch_add(1, Ordering::SeqCst);
        // 注意：last_signal_time 更新仍需要外部同步
    }

    /// 记录订单发送
    pub fn record_order_sent(&self) {
        self.order_sent.fetch_add(1, Ordering::SeqCst);
        // 注意：last_order_time 更新仍需要外部同步
    }

    /// 记录订单成交
    pub fn record_order_filled(&self) {
        self.order_filled.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录订单失败
    pub fn record_order_failed(&self) {
        self.order_failed.fetch_add(1, Ordering::SeqCst);
    }
}

// ============================================================================
// 品种状态
// ============================================================================

/// 品种交易状态
///
/// # 维护信息
/// - 交易锁
/// - 指标请求/成功时间戳
/// - 缓存的信号
/// - 启动状态
/// - 运行指标
/// - 策略绑定
///
/// # 模块间调用规则
/// ⚠️ 禁止直接访问字段，必须使用方法
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolState {
    /// 品种符号
    symbol: String,
    /// 交易锁（品种级独立锁 = 原 TradeLock）
    trade_lock: TradeLock,
    /// 启动状态
    startup_state: StartupState,

    // --- 策略绑定 ---
    /// 绑定的策略ID（用于品种互斥检查）
    bound_strategy_id: Option<String>,
    /// 绑定时间戳
    bound_at: Option<i64>,

    // --- 分钟级状态 ---
    /// 上次分钟级指标请求时间戳
    last_1m_request_ts: i64,
    /// 上次分钟级指标成功获取时间戳
    last_1m_ok_ts: i64,
    /// 上次获取的分钟级信号时间戳
    last_1m_signal_ts: i64,
    /// 缓存的分钟级信号
    last_1m_signal: Option<TradingDecision>,

    // --- 日线级状态 ---
    /// 上次日线级指标请求时间戳
    last_daily_request_ts: i64,
    /// 上次日线级指标成功获取时间戳
    last_daily_ok_ts: i64,
    /// 缓存的日线级信号
    last_daily_signal: Option<TradingDecision>,

    /// 超时阈值（秒）
    timeout_secs: i64,

    // --- 运行指标 ---
    /// 品种级指标
    metrics: SymbolMetrics,
}

impl SymbolState {
    /// 创建新的品种状态
    pub fn new(symbol: String) -> Self {
        Self {
            symbol: symbol.clone(),
            trade_lock: TradeLock::new(),
            startup_state: StartupState::Fresh,
            bound_strategy_id: None,
            bound_at: None,
            last_1m_request_ts: 0,
            last_1m_ok_ts: 0,
            last_1m_signal_ts: 0,
            last_1m_signal: None,
            last_daily_request_ts: 0,
            last_daily_ok_ts: 0,
            last_daily_signal: None,
            timeout_secs: 60,
            metrics: SymbolMetrics::new(&symbol),
        }
    }

    /// 创建灾备恢复状态的品种
    pub fn new_recovery(symbol: String) -> Self {
        Self {
            symbol: symbol.clone(),
            startup_state: StartupState::Recovery,
            ..Self::new(symbol)
        }
    }

    // ─────────────────────────────────────────────────────────
    // 策略绑定方法
    // ─────────────────────────────────────────────────────────

    /// 绑定策略
    ///
    /// # 返回
    /// - `true` 绑定成功
    /// - `false` 已被其他策略绑定
    pub fn bind_strategy(&mut self, strategy_id: &str) -> bool {
        if let Some(ref bound) = self.bound_strategy_id {
            if bound != strategy_id {
                return false; // 已被其他策略绑定
            }
            return true; // 已绑定到同一策略
        }
        self.bound_strategy_id = Some(strategy_id.to_string());
        self.bound_at = Some(chrono::Utc::now().timestamp());
        true
    }

    /// 解绑策略
    ///
    /// 解绑后可重新被其他策略触发
    pub fn unbind_strategy(&mut self) {
        self.bound_strategy_id = None;
        self.bound_at = None;
    }

    /// 获取绑定的策略ID
    pub fn bound_strategy(&self) -> Option<&str> {
        self.bound_strategy_id.as_deref()
    }

    /// 检查是否已绑定策略
    pub fn is_bound(&self) -> bool {
        self.bound_strategy_id.is_some()
    }

    /// 检查是否被指定策略绑定
    pub fn is_bound_by(&self, strategy_id: &str) -> bool {
        self.bound_strategy_id.as_deref() == Some(strategy_id)
    }

    // ─────────────────────────────────────────────────────────
    // 查询方法
    // ─────────────────────────────────────────────────────────

    /// 获取品种符号
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// 获取启动状态
    pub fn startup_state(&self) -> StartupState {
        self.startup_state
    }

    /// 获取交易锁（只读）
    pub fn trade_lock(&self) -> &TradeLock {
        &self.trade_lock
    }

    /// 获取交易锁（可变）
    pub fn trade_lock_mut(&mut self) -> &mut TradeLock {
        &mut self.trade_lock
    }

    /// 获取品种指标
    pub fn metrics(&self) -> &SymbolMetrics {
        &self.metrics
    }

    /// 获取品种指标（可变）
    pub fn metrics_mut(&mut self) -> &mut SymbolMetrics {
        &mut self.metrics
    }

    /// 获取超时阈值
    pub fn timeout_secs(&self) -> i64 {
        self.timeout_secs
    }

    /// 检查分钟级是否超时
    pub fn is_1m_timeout(&self, now_ts: i64) -> bool {
        if self.last_1m_request_ts == 0 {
            return false;
        }
        now_ts - self.last_1m_request_ts > self.timeout_secs
    }

    /// 检查日线级是否超时
    pub fn is_daily_timeout(&self, now_ts: i64) -> bool {
        if self.last_daily_request_ts == 0 {
            return false;
        }
        now_ts - self.last_daily_request_ts > self.timeout_secs
    }

    /// 检查信号是否过期
    pub fn is_signal_stale(&self, signal_ts: i64, now_ts: i64) -> bool {
        now_ts - signal_ts > self.timeout_secs
    }

    /// 获取分钟级信号时间戳
    pub fn last_1m_signal_ts(&self) -> i64 {
        self.last_1m_signal_ts
    }

    /// 获取分钟级信号
    pub fn last_1m_signal(&self) -> Option<&TradingDecision> {
        self.last_1m_signal.as_ref()
    }

    /// 获取日线级信号
    pub fn last_daily_signal(&self) -> Option<&TradingDecision> {
        self.last_daily_signal.as_ref()
    }

    /// 上次分钟级请求时间戳
    pub fn last_1m_request_ts(&self) -> i64 {
        self.last_1m_request_ts
    }

    /// 上次分钟级成功时间戳
    pub fn last_1m_ok_ts(&self) -> i64 {
        self.last_1m_ok_ts
    }

    /// 上次日线级请求时间戳
    pub fn last_daily_request_ts(&self) -> i64 {
        self.last_daily_request_ts
    }

    /// 上次日线级成功时间戳
    pub fn last_daily_ok_ts(&self) -> i64 {
        self.last_daily_ok_ts
    }

    /// 获取绑定时间戳
    pub fn bound_at(&self) -> Option<i64> {
        self.bound_at
    }

    // ─────────────────────────────────────────────────────────
    // 更新方法
    // ─────────────────────────────────────────────────────────

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
        self.metrics.record_tick();
    }

    /// 记录分钟级成功
    pub fn record_1m_ok(&mut self, ts: i64, signal_ts: i64, signal: TradingDecision) {
        self.last_1m_ok_ts = ts;
        self.last_1m_signal_ts = signal_ts;
        self.last_1m_signal = Some(signal);
        self.metrics.record_signal();
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

    /// 记录订单发送
    pub fn record_order_sent(&mut self) {
        self.metrics.record_order_sent();
    }

    /// 记录订单成交
    pub fn record_order_filled(&mut self) {
        self.metrics.record_order_filled();
    }

    /// 记录订单失败
    pub fn record_order_failed(&mut self) {
        self.metrics.record_order_failed();
    }
}

// ============================================================================
// 检查配置
// ============================================================================

/// 检查配置 - 时间窗口控制
///
/// 所有字段私有化
#[derive(Debug, Clone)]
pub struct CheckConfig {
    /// 分钟级检查间隔（毫秒）
    minute_check_interval_ms: u64,
    /// 日线级检查间隔（毫秒）
    daily_check_interval_ms: u64,
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            minute_check_interval_ms: 1000,
            daily_check_interval_ms: 1000,
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

    /// 获取分钟级检查间隔
    pub fn minute_check_interval_ms(&self) -> u64 {
        self.minute_check_interval_ms
    }

    /// 获取日线级检查间隔
    pub fn daily_check_interval_ms(&self) -> u64 {
        self.daily_check_interval_ms
    }
}
