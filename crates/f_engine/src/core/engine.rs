//! TradingEngine - TradeManager 模式实现
//!
//! # 架构
//! 基于 Python TradeManager 模式：
//! - 引擎层：任务注册表，心跳检查、持久化
//! - 品种层：每个品种独立任务，自己循环
//!
//! # 核心概念
//! - 异步任务（Async Task）：tokio::spawn 的协程，不是子线程
//! - 双重状态核对：引擎层 + 品种层
//! - 心跳机制：品种定时更新 last_beat，引擎定期检查

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use fnv::FnvHashMap;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use tokio::sync::{RwLock as TokioRwLock, Mutex as TokioMutex};
use tokio::time::sleep;

use crate::core::triggers::TriggerManager;
use crate::core::execution::TradingPipeline;
use crate::core::risk_manager::RiskManager;
use crate::order::OrderExecutor;

// ============================================================================
// 运行状态
// ============================================================================

/// 品种运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningStatus {
    /// 运行中
    Running,
    /// 已停止
    Stopped,
    /// 已结束
    Ended,
}

// ============================================================================
// 品种状态
// ============================================================================

/// TaskState - 每个品种的任务状态
///
/// # 职责
/// - 维护自己的运行状态
/// - 维护自己的心跳
/// - 维护自己的持仓信息
/// - 自己决定是否结束
///
/// # 特点
/// - 每个品种独立，使用 tokio::RwLock 支持 async
#[derive(Debug, Clone)]
pub struct TaskState {
    /// 品种符号
    pub symbol: String,
    /// 运行状态
    pub status: RunningStatus,
    /// 最后更新时间（心跳）
    pub last_beat: i64,
    /// 持仓数量
    pub position_qty: Decimal,
    /// 持仓均价
    pub position_price: Decimal,
    /// 持仓方向 (true=多, false=空)
    pub position_side: bool,
    /// 禁止交易截止时间
    pub forbid_until: Option<i64>,
    /// 禁止原因
    pub forbid_reason: Option<String>,
    /// 交易次数
    pub trade_count: u32,
    /// 结束原因
    pub done_reason: Option<String>,
    /// 循环间隔（毫秒）
    pub interval_ms: u64,
}

impl TaskState {
    /// 创建新的任务状态
    pub fn new(symbol: String, interval_ms: u64) -> Self {
        Self {
            symbol,
            status: RunningStatus::Running,
            last_beat: Utc::now().timestamp(),
            position_qty: Decimal::ZERO,
            position_price: Decimal::ZERO,
            position_side: true,
            forbid_until: None,
            forbid_reason: None,
            trade_count: 0,
            done_reason: None,
            interval_ms,
        }
    }

    /// 更新心跳
    pub fn heartbeat(&mut self) {
        self.last_beat = Utc::now().timestamp();
    }

    /// 检查是否被禁止
    pub fn is_forbidden(&self) -> bool {
        if let Some(until) = self.forbid_until {
            return Utc::now().timestamp() < until;
        }
        false
    }

    /// 标记为已结束
    pub fn end(&mut self, reason: String) {
        self.status = RunningStatus::Ended;
        self.done_reason = Some(reason);
        // 禁止到下个日线周期（次日 00:00 UTC）
        self.forbid_until = Some(next_day_start());
        self.heartbeat();
    }

    /// 更新持仓
    pub fn update_position(&mut self, qty: Decimal, price: Decimal, side: bool) {
        self.position_qty = qty;
        self.position_price = price;
        self.position_side = side;
        self.trade_count += 1;
    }

    /// 平仓完成
    pub fn close_position(&mut self) {
        self.position_qty = Decimal::ZERO;
        self.position_price = Decimal::ZERO;
    }

    /// 是否在交易中
    pub fn is_trading(&self) -> bool {
        self.position_qty > Decimal::ZERO
    }

    /// 获取持仓信息
    pub fn position_info(&self) -> (Decimal, Decimal, bool) {
        (self.position_qty, self.position_price, self.position_side)
    }
}

// ============================================================================
// 引擎
// ============================================================================

/// Engine - TradeManager 模式引擎
///
/// # 职责
/// - 维护任务注册表
/// - 触发器检查，启动任务
/// - 心跳检查
/// - 任务变化时持久化
///
/// # 特点
/// - 任务自主循环，引擎不控制
/// - 双重状态：引擎层 + 品种层
/// - 心跳机制同步状态
pub struct Engine {
    /// 任务注册表 (使用 tokio::RwLock 支持 async)
    tasks: Arc<TokioRwLock<FnvHashMap<String, Arc<TokioRwLock<TaskState>>>>>,
    /// 数据库
    db: Option<EngineDb>,
    /// 心跳超时（秒）
    heartbeat_timeout: i64,
    /// 全局锁（下单时使用）
    global_lock: Arc<TokioMutex<()>>,
    /// 触发器管理器
    trigger_manager: TriggerManager,
    /// 交易管道
    pipeline: TradingPipeline,
    /// 风控管理器
    risk_manager: RiskManager,
    /// 订单执行器
    order_executor: OrderExecutor,
}

impl Engine {
    /// 创建新的引擎
    pub fn new(
        pipeline: TradingPipeline,
        risk_manager: RiskManager,
        order_executor: OrderExecutor,
    ) -> Self {
        Self {
            tasks: Arc::new(TokioRwLock::new(FnvHashMap::default())),
            db: None,
            heartbeat_timeout: 90,
            global_lock: Arc::new(TokioMutex::new(())),
            trigger_manager: TriggerManager::default(),
            pipeline,
            risk_manager,
            order_executor,
        }
    }

    /// 设置数据库
    pub fn with_db(mut self, db: EngineDb) -> Self {
        self.db = Some(db);
        self
    }

    /// 启动任务
    pub async fn spawn_task(&self, symbol: String, interval_ms: u64) {
        // 如果任务已存在，跳过
        let tasks = self.tasks.read().await;
        if tasks.contains_key(&symbol) {
            return;
        }
        drop(tasks);

        let state = Arc::new(TokioRwLock::new(TaskState::new(symbol.clone(), interval_ms)));
        
        // 注册到任务表
        let mut tasks = self.tasks.write().await;
        tasks.insert(symbol.clone(), Arc::clone(&state));
        drop(tasks);

        // 持久化创建
        if let Some(ref db) = self.db {
            db.persist_task_created(&symbol, interval_ms);
        }

        // spawn 异步任务
        let global_lock = Arc::clone(&self.global_lock);
        let db = self.db.clone();
        let tasks_ref = Arc::clone(&self.tasks);
        let interval = interval_ms;
        let symbol_for_log = symbol.clone();

        tokio::spawn(async move {
            Self::task_loop(
                symbol,
                state,
                global_lock,
                db,
                tasks_ref,
                interval,
            ).await;
        });

        tracing::info!("[Engine] Spawned task: {} (interval: {}ms)", symbol_for_log, interval_ms);
    }

    /// 任务主循环
    async fn task_loop(
        symbol: String,
        state: Arc<TokioRwLock<TaskState>>,
        global_lock: Arc<TokioMutex<()>>,
        db: Option<EngineDb>,
        tasks_ref: Arc<TokioRwLock<FnvHashMap<String, Arc<TokioRwLock<TaskState>>>>>,
        interval_ms: u64,
    ) {
        loop {
            // 1. 检查禁止
            {
                let s = state.read().await;
                if s.is_forbidden() {
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
                if s.status == RunningStatus::Ended {
                    break;
                }
            }

            // 2. 获取全局锁（使用 tokio 的锁，支持跨 await）
            let _lock = global_lock.lock().await;

            // 3. 执行交易
            let should_end = Self::execute_once(&symbol, &state).await;

            // 4. 更新心跳
            state.write().await.heartbeat();

            // 5. 释放锁
            drop(_lock);

            // 6. 检查是否结束
            if should_end {
                let mut s = state.write().await;
                s.end("TradeComplete".to_string());
                
                // 持久化结束
                if let Some(ref db) = db {
                    db.persist_task_ended(&symbol, "TradeComplete");
                }
                
                tracing::info!("[Engine] Task ended: {} (reason: TradeComplete)", symbol);
                break;
            }

            // 7. 等待下一个周期
            sleep(Duration::from_millis(interval_ms)).await;
        }

        // 8. 从注册表移除
        let mut tasks = tasks_ref.write().await;
        tasks.remove(&symbol);
    }

    /// 执行一次交易
    async fn execute_once(_symbol: &str, state: &Arc<TokioRwLock<TaskState>>) -> bool {
        // 简化版本，实际需要调用策略、风控、下单
        let s = state.read().await;
        
        // 如果已持仓，检查是否该平仓
        if s.is_trading() {
            // TODO: 调用策略判断是否该平仓
            // let should_close = strategy.should_close();
            // if should_close {
            //     return true;
            // }
        }
        
        // TODO: 调用策略判断是否该开仓
        // let signal = strategy.check();
        // if signal.has_signal() {
        //     // 风控
        //     // 下单
        // }
        
        false
    }

    /// 引擎主循环
    pub async fn run(&self) {
        tracing::info!("[Engine] Engine started");

        loop {
            // 1. 检查任务
            self.check_tasks().await;

            // 2. 检查心跳
            self.check_heartbeat().await;

            // 3. 触发器检查
            self.check_triggers().await;

            // 4. 等待
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// 检查所有任务
    async fn check_tasks(&self) {
        let now = Utc::now().timestamp();
        let mut to_remove: Vec<String> = Vec::new();

        let tasks = self.tasks.read().await;
        for (symbol, state_arc) in tasks.iter() {
            let s = state_arc.read().await;

            match s.status {
                RunningStatus::Ended => {
                    // 发现已结束，准备移除
                    to_remove.push(symbol.clone());
                    
                    // 持久化
                    if let Some(ref db) = self.db {
                        if let Some(ref reason) = s.done_reason {
                            db.persist_task_ended(symbol, reason);
                        }
                    }
                }
                RunningStatus::Running if s.last_beat < now - self.heartbeat_timeout => {
                    // 超时
                    tracing::warn!("[Engine] Task heartbeat timeout: {}", symbol);
                }
                _ => {}
            }
        }
        drop(tasks);

        // 移除已结束的任务
        if !to_remove.is_empty() {
            let mut tasks = self.tasks.write().await;
            for symbol in &to_remove {
                tasks.remove(symbol);
                tracing::info!("[Engine] Removed task: {}", symbol);
            }
        }
    }

    /// 检查心跳
    async fn check_heartbeat(&self) {
        let now = Utc::now().timestamp();

        let tasks = self.tasks.read().await;
        for (symbol, state_arc) in tasks.iter() {
            let s = state_arc.read().await;

            if s.status == RunningStatus::Running && s.last_beat < now - self.heartbeat_timeout {
                tracing::warn!("[Engine] Task heartbeat timeout: {} (last_beat: {}, now: {})", 
                    symbol, s.last_beat, now);
            }
        }
    }

    /// 触发器检查
    async fn check_triggers(&self) {
        // TODO: 实现触发器逻辑
        // 1. 日线触发器扫描
        // 2. 分钟触发器扫描
        // 3. 调用 self.spawn_task(symbol, interval)
    }

    /// 获取任务数量
    pub async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    /// 获取运行中的任务
    pub async fn running_tasks(&self) -> Vec<String> {
        self.tasks.read().await.keys().cloned().collect()
    }
}

// ============================================================================
// 数据库接口
// ============================================================================

/// EngineDb - 引擎数据库接口
#[derive(Clone)]
pub struct EngineDb {
    // TODO: 实现数据库连接
}

impl EngineDb {
    /// 创建数据库
    pub fn new(_path: &str) -> Self {
        Self {
            // TODO: 初始化 SQLite 连接
        }
    }

    /// 持久化任务创建
    pub fn persist_task_created(&self, symbol: &str, interval_ms: u64) {
        tracing::debug!("[DB] Task created: {} (interval: {}ms)", symbol, interval_ms);
        // TODO: INSERT INTO symbol_tasks ...
    }

    /// 持久化任务结束
    pub fn persist_task_ended(&self, symbol: &str, reason: &str) {
        tracing::debug!("[DB] Task ended: {} (reason: {})", symbol, reason);
        // TODO: UPDATE symbol_tasks SET status='Ended', done_reason=...
    }

    /// 加载所有任务
    pub fn load_all_tasks(&self) -> Vec<(String, u64)> {
        // TODO: SELECT symbol, interval_ms FROM symbol_tasks WHERE status='Running'
        Vec::new()
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 获取下个日线周期开始时间（次日 00:00 UTC）
fn next_day_start() -> i64 {
    let now = Utc::now();
    let tomorrow = now.date_naive() + chrono::Duration::days(1);
    tomorrow.and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_new() {
        let state = TaskState::new("BTCUSDT".to_string(), 50);
        assert_eq!(state.symbol, "BTCUSDT");
        assert_eq!(state.status, RunningStatus::Running);
        assert_eq!(state.interval_ms, 50);
        assert!(!state.is_trading());
    }

    #[test]
    fn test_task_state_forbid() {
        let mut state = TaskState::new("BTCUSDT".to_string(), 50);
        
        // 设置禁止
        state.forbid_until = Some(Utc::now().timestamp() + 3600);
        assert!(state.is_forbidden());
        
        // 清除禁止
        state.forbid_until = None;
        assert!(!state.is_forbidden());
    }

    #[test]
    fn test_task_state_end() {
        let mut state = TaskState::new("BTCUSDT".to_string(), 50);
        state.position_qty = dec!(10);
        
        state.end("TradeComplete".to_string());
        
        assert_eq!(state.status, RunningStatus::Ended);
        assert_eq!(state.done_reason, Some("TradeComplete".to_string()));
        assert!(state.forbid_until.is_some());
        assert_eq!(state.position_qty, dec!(10)); // 持仓不自动清空
    }

    #[test]
    fn test_next_day_start() {
        let next = next_day_start();
        let now = Utc::now().timestamp();
        assert!(next > now);
        assert!(next - now < 86400); // 小于一天
    }
}
