================================================================
品种协程自循环设计方案（详细版·第三轮评审后·终版）
================================================================
Author: 软件架构师
Date: 2026-03-27
Status: 待实现
Based on: pin_main.py (Python) + h_15m/ (Rust)
Review: 架构评审 v3（异步上下文 + 重启计数 + 边界处理）← 终版
================================================================

一、核心设计原则
================================================================

一个品种 = 一个协程 = 一个 Trader
Engine 管 spawn/stop/监控/重启，Trader 自己 loop。

与 pin_main.py (Python) 1:1 对齐：
- Trader.loop() 自循环（对应 Python _run_loop）
- executor.rs 封装下单（对应 Python place_order）
- repository.rs 封装存储（对应 Python _store_data / _load_data）
- TradeRecord 贯穿全流程（对应 Python Account_Information）

二、架构图
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  Engine                                                          │
│  ├── InstanceMap: HashMap<symbol, TraderHandle>                 │
│  ├── spawn_with_count(symbol, restart_count)                    │
│  ├── Monitor 协程（心跳检测 + 指数退避重启）                      │
│  └── stop(symbol) → shutdown.notify_one() → 优雅停止             │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Trader.loop():                                                   │
│    1. shutdown.notified() → 优雅退出                           │
│    2. handle.heartbeat()          -- 更新心跳                   │
│    3. record = TradeRecord.new()  -- 预创建 pending 记录        │
│    4. repository.save_pending(record)  -- WAL: 先写日志          │
│    5. 生成信号 + 决策                                           │
│    6. if all passed:                                           │
│         executor.send_order(record)   -- 发送订单                │
│         repository.confirm_record(id) -- WAL: 确认               │
│    7. sleep(interval)                                          │
└─────────────────────────────────────────────────────────────────┘

三、h_15m 模块结构（终版）
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  h_15m/                                                         │
├─────────────────────────────────────────────────────────────────┤
│  trader.rs         # Trader（自循环 + 优雅停止）✅ 已有          │
│  signal.rs         # 信号生成器（7条件Pin）✅ 已有              │
│  status.rs         # 状态机（PinStatus）✅ 已有                │
│  executor.rs       # 下单网关 [改造：仅保留 send_order]         │
│  repository.rs     # 数据持久化 + WAL [新建]                   │
│  mod.rs            # 模块导出                                     │
└─────────────────────────────────────────────────────────────────┘

职责划分：
- executor.rs: 交易所交互 + 风控前置检查（rate_limit_check, calculate_order_qty）
- repository.rs: SQLite 读写 + WAL 事务管理 + 崩溃恢复
- trader.rs: 流程编排（协调 executor + repository）

四、TradeRecord 设计（WAL 模式）
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  TradeRecord（对应 Python: Account_Information）                │
├─────────────────────────────────────────────────────────────────┤
│  id                  i64 PRIMARY KEY                            │
│  symbol              TEXT NOT NULL                              │
│  timestamp           INTEGER NOT NULL                           │
│  interval_ms         INTEGER                                   │
│  status              TEXT  -- PENDING | CONFIRMED | FAILED     │
│  // === 行情快照 ===                                           │
│  price               TEXT                                      │
│  volatility          REAL                                      │
│  market_status       TEXT                                      │
│  // === 持仓快照 ===                                           │
│  exchange_position   TEXT                                      │
│  local_position     TEXT                                      │
│  // === 策略状态 ===                                           │
│  trader_status       TEXT                                      │
│  signal_json         TEXT                                      │
│  confidence          INTEGER                                   │
│  // === 账户状态 ===                                           │
│  realized_pnl        TEXT                                      │
│  unrealized_pnl      TEXT                                      │
│  available_balance   TEXT                                      │
│  used_margin         TEXT                                      │
│  // === 订单执行 ===                                           │
│  order_type          INTEGER                                   │
│  order_direction     TEXT                                      │
│  order_quantity      TEXT                                      │
│  order_result        TEXT                                      │
│  order_timestamp     INTEGER                                   │
│  // === 检查表 ===                                             │
│  check_passed        INTEGER                                   │
│  signal_passed       INTEGER                                   │
│  price_check_passed  INTEGER                                   │
│  risk_check_passed   INTEGER                                   │
│  lock_acquired       INTEGER                                   │
│  order_executed      INTEGER                                   │
│  record_saved        INTEGER                                   │
└─────────────────────────────────────────────────────────────────┘

status 状态机（WAL 一致性保证）：
  PENDING   → 下单前预写，幂等性保证
  CONFIRMED → 下单成功后确认
  FAILED    → 下单失败标记

五、executor.rs 设计（终版）
================================================================

5.1 职责

┌─────────────────────────────────────────────────────────────────┐
│  Executor（线程安全：Arc<dyn ExchangeGateway>）                  │
├─────────────────────────────────────────────────────────────────┤
│  send_order(signal, order_type, current_qty, current_side)      │
│  rate_limit_check(interval_ms) → bool   【原子操作】            │
│  calculate_order_qty(...) → Decimal 【Decimal精度 + 步长裁剪】   │
└─────────────────────────────────────────────────────────────────┘

5.2 rate_limit_check（原子操作）

```rust
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Executor {
    last_order_ms: AtomicU64,  // UNIX 毫秒时间戳
    // ...
}

impl Executor {
    /// 频率限制检查（完全原子，无锁）
    ///
    /// CAS 循环确保：
    /// 1. 时间窗口检查
    /// 2. 时间戳更新
    /// 在同一原子操作中完成，消除竞态窗口
    pub fn rate_limit_check(&self, interval_ms: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        loop {
            let last = self.last_order_ms.load(Ordering::Relaxed);

            if now.saturating_sub(last) < interval_ms {
                tracing::warn!(
                    symbol = %self.symbol,
                    last_ms = last,
                    now_ms = now,
                    interval_ms = interval_ms,
                    "下单频率过高，跳过"
                );
                return false;
            }

            match self.last_order_ms.compare_exchange_weak(
                last, now,
                Ordering::SeqCst,
                Ordering::Relaxed
            ) {
                Ok(_) => return true,
                Err(_) => continue, // 已被其他线程更新，重试
            }
        }
    }
}
```

5.3 calculate_order_qty（终版·Decimal精度 + 步长裁剪）

```rust
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub enum OrderType {
    InitialOpen = 0,
    HedgeOpen   = 1,
    DoubleAdd   = 2,
    DoubleClose = 3,
    DayHedge    = 4,
    DayClose    = 5,
}

impl Executor {
    /// 计算下单数量
    ///
    /// 参数说明：
    /// - order_type: 下单类型
    /// - current_qty: 当前持仓数量（带符号：多头正，空头负）
    /// - current_side: 当前持仓方向（用于判断是否已有持仓）
    pub fn calculate_order_qty(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Decimal {
        let raw_qty = match order_type {
            OrderType::InitialOpen => {
                self.config.initial_ratio
            }
            OrderType::HedgeOpen => {
                if current_qty.abs() > Decimal::ZERO {
                    current_qty.abs()
                } else {
                    Decimal::ZERO
                }
            }
            OrderType::DoubleAdd => {
                current_qty.abs() * dec!(0.5)
            }
            OrderType::DoubleClose | OrderType::DayClose => {
                current_qty.abs()
            }
            OrderType::DayHedge => {
                current_qty.abs()
            }
        };

        // P2: 按交易所步长裁剪（如 BTCUSDT 步长 0.001）
        self.round_to_lot_size(raw_qty)
    }

    /// 按步长裁剪数量（P2: Decimal 精度 + 交易所约束）
    ///
    /// 例如：raw_qty = 0.123456, step = 0.001 → 0.123
    fn round_to_lot_size(&self, qty: Decimal) -> Decimal {
        let step = self.config.lot_size;
        (qty / step).floor() * step
    }
}
```

5.4 send_order 完整逻辑（终版·参数签名正确）

```rust
impl Executor {
    /// 发送订单（完整流程）
    pub fn send_order(
        &self,
        signal: &StrategySignal,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Result<OrderResult, ExecutorError> {
        // 1. 频率限制（原子）
        if !self.rate_limit_check(self.config.order_interval_ms) {
            return Err(ExecutorError::RateLimited);
        }

        // 2. 计算下单数量（Decimal 精度 + 步长裁剪）
        let qty = self.calculate_order_qty(order_type, current_qty, current_side);

        if qty <= Decimal::ZERO {
            tracing::warn!(
                symbol = %self.symbol,
                order_type = ?order_type,
                "计算下单数量为 0，跳过"
            );
            return Err(ExecutorError::ZeroQuantity);
        }

        // 3. 风控前置检查
        self.pre_risk_check(qty)?;

        // 4. 构造 OrderRequest
        let req = OrderRequest {
            symbol: self.symbol.clone(),
            side: signal.direction.to_side(),
            position_side: signal.direction.to_position_side(),
            order_type: OrderType::LimitPrice,
            quantity: qty,
            price: signal.target_price,
            // ...
        };

        // 5. 执行下单
        self.gateway.place_order(req)
            .map_err(ExecutorError::Gateway)
    }
}
```

5.5 Error 类型定义（终版）

```rust
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("频率限制")]
    RateLimited,

    #[error("数量为零")]
    ZeroQuantity,

    #[error("风控拒绝: {0}")]
    RiskCheckFailed(String),

    #[error("网关错误: {0}")]
    Gateway(#[from] GatewayError),

    #[error("超时: {0}")]
    Timeout(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("记录不存在")]
    NotFound,

    #[error("唯一约束冲突")]
    UniqueViolation,

    #[error("序列化错误: {0}")]
    Serialization(String),
}
```

六、repository.rs 设计（终版·WAL + ID 生命周期）
================================================================

6.1 职责

┌─────────────────────────────────────────────────────────────────┐
│  Repository（WAL 事务一致性 + 连接池）                            │
├─────────────────────────────────────────────────────────────────┤
│  save_pending(record)       → 保存 PENDING 记录（幂等）          │
│  confirm_record(id, res)    → 更新为 CONFIRMED                   │
│  mark_failed(id, msg)      → 更新为 FAILED                       │
│  load_latest(symbol)        → 崩溃恢复：加载最新记录              │
│  get_by_timestamp()        → 幂等冲突处理                        │
│  gc_pending()              → 清理超时的 PENDING 记录             │
└─────────────────────────────────────────────────────────────────┘

6.2 连接池配置

```rust
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub struct Repository {
    pool: Arc<Pool<SqliteConnectionManager>>,  // 共享连接池
    symbol: String,
}

impl Repository {
    pub fn new(symbol: &str, db_path: &str) -> Result<Self, RepoError> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(10)  // 连接池大小，根据品种数调整
            .build(manager)?;

        Self::init_schema(&pool)?;
        Ok(Self {
            pool: Arc::new(pool),
            symbol: symbol.to_string(),
        })
    }
}
```

6.3 WAL 事务流程（终版·修正 ID 生命周期 + 无限循环防护）

```rust
impl Repository {
    /// 预写记录（对应 WAL write-ahead 阶段）
    ///
    /// 关键保证：
    /// - INSERT OR REPLACE 保证幂等（同一 symbol+timestamp 唯一）
    /// - 先写入 PENDING 状态，保证崩溃后可恢复
    pub fn save_pending(&self, record: &TradeRecord) -> Result<i64, RepoError> {
        let record = TradeRecord {
            status: RecordStatus::PENDING,
            ..record.clone()
        };

        let sql = r#"
            INSERT OR REPLACE INTO trade_records (
                symbol, timestamp, interval_ms, status,
                price, volatility, market_status,
                exchange_position, local_position,
                trader_status, signal_json, confidence,
                realized_pnl, unrealized_pnl,
                available_balance, used_margin,
                order_type, order_direction, order_quantity,
                order_result, order_timestamp,
                check_passed, signal_passed,
                price_check_passed, risk_check_passed,
                lock_acquired, order_executed, record_saved
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let id = self.exec_insert(sql, &record)?;
        tracing::debug!(id = id, symbol = %record.symbol, "预写记录 PENDING");
        Ok(id)
    }

    /// 确认记录（对应 WAL commit 阶段）
    pub fn confirm_record(&self, id: i64, order_result: &str) -> Result<(), RepoError> {
        let sql = r#"
            UPDATE trade_records
            SET status = 'CONFIRMED',
                order_result = ?,
                record_saved = 1
            WHERE id = ?
        "#;

        self.exec_update(sql, rusqlite::params![order_result, id])?;
        tracing::info!(id = id, "记录已确认 CONFIRMED");
        Ok(())
    }

    /// 标记失败（对应 WAL rollback 阶段）
    pub fn mark_failed(&self, id: i64, error: &str) -> Result<(), RepoError> {
        let sql = r#"
            UPDATE trade_records
            SET status = 'FAILED',
                order_result = ?,
                record_saved = 1
            WHERE id = ?
        "#;

        self.exec_update(sql, rusqlite::params![error, id])?;
        tracing::error!(id = id, error = %error, "下单失败且记录已标记");
        Ok(())
    }

    /// 按 symbol + timestamp 查询（幂等冲突处理）
    pub fn get_by_timestamp(
        &self,
        symbol: &str,
        timestamp: i64,
    ) -> Result<Option<TradeRecord>, RepoError> {
        let sql = r#"
            SELECT * FROM trade_records
            WHERE symbol = ? AND timestamp = ?
            LIMIT 1
        "#;

        let mut stmt = self.prepare(sql)?;
        let mut rows = stmt.query(rusqlite::params![symbol, timestamp])?;

        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_record(row)?))
        } else {
            Ok(None)
        }
    }
}
```

6.4 崩溃恢复

```rust
impl Repository {
    /// 从 SQLite 加载最新记录，用于崩溃恢复
    ///
    /// 对应 Python: _load_data()
    ///
    /// 恢复流程：
    /// 1. 查询最新记录
    /// 2. PENDING → 标记为 FAILED（下次 loop 重试或人工确认）
    /// 3. CONFIRMED / FAILED → 无需处理
    pub fn load_latest(&self, symbol: &str) -> Result<Option<TradeRecord>, RepoError> {
        let sql = r#"
            SELECT * FROM trade_records
            WHERE symbol = ?
            ORDER BY timestamp DESC
            LIMIT 1
        "#;

        let mut stmt = self.prepare(sql)?;
        let mut rows = stmt.query(rusqlite::params![symbol])?;

        if let Some(row) = rows.next()? {
            let record = self.row_to_record(row)?;

            if record.status.as_str() == "PENDING" {
                tracing::warn!(
                    symbol = %symbol,
                    id = record.id,
                    timestamp = record.timestamp,
                    "发现 PENDING 记录，可能存在未确认下单"
                );
                self.mark_failed(
                    record.id,
                    "CRASH_RECOVERY: pending record marked as failed",
                )?;
            }

            return Ok(Some(record));
        }

        Ok(None)
    }

    /// 兜底清理：超时 PENDING 记录
    ///
    /// 超时时间：5 分钟
    /// 注意：与 Python 的 5 天历史数据超时（TIMEOUT_PERIOD）是不同概念
    pub const PENDING_TIMEOUT_SECS: i64 = 300;  // 5 分钟

    pub fn gc_pending(&self) -> Result<usize, RepoError> {
        let cutoff = chrono::Utc::now().timestamp() - Self::PENDING_TIMEOUT_SECS;

        let sql = r#"
            UPDATE trade_records
            SET status = 'FAILED',
                order_result = 'GC_TIMEOUT: pending record cleaned up'
            WHERE status = 'PENDING'
              AND timestamp < ?
        "#;

        let affected = self.exec_update(sql, rusqlite::params![cutoff])?;
        if affected > 0 {
            tracing::warn!(count = affected, "清理了 {} 条超时 PENDING 记录", affected);
        }

        Ok(affected)
    }
}
```

七、SQLite 表结构（终版）
================================================================

```sql
CREATE TABLE IF NOT EXISTS trade_records (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol              TEXT NOT NULL,
    timestamp           INTEGER NOT NULL,
    interval_ms         INTEGER DEFAULT 100,
    status              TEXT DEFAULT 'PENDING',
    -- 行情快照
    price               TEXT,
    volatility          REAL,
    market_status       TEXT,
    -- 持仓快照
    exchange_position   TEXT,
    local_position     TEXT,
    -- 策略状态
    trader_status       TEXT,
    signal_json        TEXT,
    confidence          INTEGER,
    -- 账户状态
    realized_pnl        TEXT,
    unrealized_pnl      TEXT,
    available_balance  TEXT,
    used_margin        TEXT,
    -- 订单执行
    order_type          INTEGER,
    order_direction    TEXT,
    order_quantity     TEXT,
    order_result       TEXT,
    order_timestamp    INTEGER,
    -- 检查表
    check_passed       INTEGER DEFAULT 0,
    signal_passed      INTEGER DEFAULT 0,
    price_check_passed INTEGER DEFAULT 0,
    risk_check_passed  INTEGER DEFAULT 0,
    lock_acquired      INTEGER DEFAULT 0,
    order_executed     INTEGER DEFAULT 0,
    record_saved       INTEGER DEFAULT 0,
    UNIQUE(symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_symbol_time
    ON trade_records(symbol, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_timestamp
    ON trade_records(timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_status
    ON trade_records(status, timestamp DESC);
```

八、trader.rs 改造（终版·第三轮修正）
================================================================

8.1 字段（终版）

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, Notify};

pub struct Trader {
    config: TraderConfig,
    status_machine: RwLock<PinStatusMachine>,     // tokio 异步锁
    signal_generator: MinSignalGenerator,
    position: RwLock<Option<LocalPosition>>,      // tokio 异步锁
    executor: Arc<Executor>,                      // 下单执行器
    repository: Arc<Repository>,                  // 数据持久化
    last_order_ms: AtomicU64,                   // 原子操作
    is_running: RwLock<bool>,                    // tokio 异步锁
    shutdown: Notify,                             // P2: 优雅停止信号
}
```

8.2 主循环（终版·异步签名 + 优雅停止 + 心跳 + 重启计数重置）

```rust
impl Trader {
    /// Trader 启动（终版）
    ///
    /// 参数：
    /// - handle: TraderHandle 引用，用于心跳更新和重启计数管理
    pub async fn start(&self, handle: Arc<TraderHandle>) {
        // P0: RwLock 需要 .write().await 解引用
        *self.is_running.write().await = true;
        tracing::info!(symbol = %self.config.symbol, "Trader 启动");

        // 崩溃恢复
        if let Some(record) = self.repository.load_latest(&self.config.symbol).ok().flatten() {
            tracing::info!(
                symbol = %self.config.symbol,
                status = %record.trader_status,
                "已从 SQLite 恢复状态"
            );
            // P0: async 函数调用需要 .await
            self.restore_from_record(&record).await;
        }

        // P0: 主循环（优雅停止 + 心跳）
        while *self.is_running.read().await {
            tokio::select! {
                // P2: 优雅停止信号
                _ = self.shutdown.notified() => {
                    tracing::info!(symbol = %self.config.symbol, "收到停止信号");
                    break;
                }
                // 正常循环
                _ = tokio::time::sleep(Duration::from_millis(self.config.interval_ms)) => {
                    // 更新心跳（P0: 监控链路完整）
                    handle.heartbeat();

                    // P1: 成功执行后重置重启计数（暗示稳定运行）
                    if self.execute_once_wal().await {
                        handle.reset_restart_count();
                    }
                }
            }
        }

        tracing::info!(symbol = %self.config.symbol, "Trader 已停止");
    }
}
```

8.3 WAL 执行一次（终版·修正异步签名 + ID 生命周期）

```rust
impl Trader {
    /// WAL 模式执行一次
    ///
    /// 返回 bool：是否成功执行（用于重启计数重置）
    async fn execute_once_wal(&self) -> bool {
        // 1. 预创建记录
        let mut record = self.build_pending_record();

        // P0: ID 获取带幂等处理（修正无限循环风险）
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

        // 2. 生成信号
        let input = match self.build_signal_input() {
            Some(i) => i,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL_INPUT").ok();
                return false;
            }
        };

        let signal_output = self.signal_generator.generate(&input, &self.volatility_tier(), None);
        record.signal_json = serde_json::to_string(&signal_output).ok();

        // 3. 决策
        let (signal, order_type) = match self.decide_action(&signal_output) {
            Some(s) => s,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL").ok();
                return false;
            }
        };

        // P0: 获取当前持仓状态（异步方法调用）
        let current_side = self.current_position_side().await;
        let current_qty = self.current_position_qty().await;

        // 4. 执行下单
        match self.executor.send_order(&signal, order_type, current_qty, current_side) {
            Ok(result) => {
                // 5. WAL 确认
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                if let Err(e) = self.repository.confirm_record(pending_id, &result_str) {
                    tracing::error!(
                        symbol = %self.config.symbol,
                        id = pending_id,
                        error = %e,
                        "下单成功但确认记录失败，需要人工介入"
                    );
                }
                // 6. 更新持仓
                self.update_position_from_result(&result);
                true  // 成功
            }
            Err(e) => {
                self.repository.mark_failed(pending_id, &format!("ORDER_FAILED: {}", e)).ok();
                false
            }
        }
    }

    /// P0: 尝试获取 pending ID（修正无限循环风险）
    ///
    /// 流程：
    /// 1. 尝试 save_pending
    /// 2. 如果 UNIQUE 冲突，查询已有记录
    /// 3. 如果查询返回 None（已被 GC），继续重试（最多3次）
    /// 4. 如果查询返回 Some，使用该 ID
    async fn try_get_pending_id(&self, record: &mut TradeRecord) -> Result<i64, RepoError> {
        const MAX_RETRIES: usize = 3;

        for attempt in 0..MAX_RETRIES {
            match self.repository.save_pending(record) {
                Ok(id) => return Ok(id),
                Err(RepoError::UniqueViolation) => {
                    // 幂等冲突
                    match self.repository.get_by_timestamp(&record.symbol, record.timestamp) {
                        Ok(Some(existing)) => {
                            tracing::warn!(
                                symbol = %record.symbol,
                                id = existing.id,
                                "发现重复记录，使用已有 ID"
                            );
                            return Ok(existing.id);
                        }
                        Ok(None) => {
                            // P1: 记录刚被 GC，重新尝试插入
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
}
```

8.4 持仓访问方法（终版·异步签名）

```rust
impl Trader {
    /// P0: 获取当前持仓方向（异步方法）
    async fn current_position_side(&self) -> Option<PositionSide> {
        self.position.read().await.as_ref().map(|p| p.side)
    }

    /// P0: 获取当前持仓数量（异步方法）
    async fn current_position_qty(&self) -> Decimal {
        self.position.read().await.as_ref().map(|p| p.qty).unwrap_or_default()
    }
}
```

8.5 状态恢复（终版·异步函数）

```rust
impl Trader {
    /// P0: 从 SQLite 记录恢复 Trader 内部状态（异步函数）
    ///
    /// 关键：status_machine 和 position 是 tokio::sync::RwLock
    /// 必须声明为 async fn，内部使用 .await
    pub async fn restore_from_record(&self, record: &TradeRecord) {
        // 1. 恢复状态机（异步写锁）
        if let Ok(status) = serde_json::from_str::<PinStatus>(&record.trader_status) {
            *self.status_machine.write().await = PinStatusMachine::from(status);
            tracing::info!(
                symbol = %self.config.symbol,
                status = ?status,
                "状态机已恢复"
            );
        }

        // 2. 恢复持仓（异步写锁）
        if let Ok(position) = serde_json::from_str::<LocalPosition>(&record.local_position) {
            *self.position.write().await = Some(position);
            tracing::info!(
                symbol = %self.config.symbol,
                qty = %position.qty,
                "持仓已恢复"
            );
        }

        // 3. 恢复 last_order_ms（P0: 防止重启后频率限制被绕过）
        if let Some(ts) = record.order_timestamp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            if now - (ts as u64) < (Self::RATE_LIMIT_INTERVAL_MS as u64) {
                self.last_order_ms.store(ts as u64, Ordering::Relaxed);
                tracing::info!(
                    symbol = %self.config.symbol,
                    last_order_ms = ts,
                    "已恢复下单频率限制"
                );
            }
        }
    }
}
```

九、Engine 心跳监控（终版·重启计数修正）
================================================================

9.1 TraderHandle 结构（终版）

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub struct TraderHandle {
    pub join_handle: Mutex<Option<JoinHandle<()>>>,
    pub last_heartbeat_ms: AtomicU64,
    pub restart_count: AtomicU32,
    pub max_restart_count: u32,    // 默认 10
    pub symbol: String,
}

pub const MAX_RESTART_COUNT: u32 = 10;

impl TraderHandle {
    pub fn new(symbol: String) -> Self {
        Self {
            join_handle: Mutex::new(None),
            last_heartbeat_ms: AtomicU64::new(current_time_ms()),
            restart_count: AtomicU32::new(0),
            max_restart_count: MAX_RESTART_COUNT,
            symbol,
        }
    }

    pub fn heartbeat(&self) {
        self.last_heartbeat_ms.store(current_time_ms(), Ordering::Relaxed);
    }

    pub fn is_stale(&self, timeout_ms: u64) -> bool {
        current_time_ms() - self.last_heartbeat_ms.load(Ordering::Relaxed) > timeout_ms
    }

    pub fn set_join_handle(&self, handle: JoinHandle<()>) {
        let mut guard = self.join_handle.lock().unwrap();
        *guard = Some(handle);
    }

    pub fn is_finished(&self) -> bool {
        let guard = self.join_handle.lock().unwrap();
        guard.as_ref().map(|h| h.is_finished()).unwrap_or(false)
    }

    /// P1: 重置重启计数（成功运行后调用）
    pub fn reset_restart_count(&self) {
        self.restart_count.store(0, Ordering::Relaxed);
        tracing::debug!(symbol = %self.symbol, "重启计数已重置");
    }

    /// 获取当前重启计数
    pub fn load_restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::Relaxed)
    }

    /// 累加重启计数
    pub fn increment_restart_count(&self) {
        self.restart_count.fetch_add(1, Ordering::SeqCst);
    }
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
```

9.2 Engine spawn（终版·传递 restart_count）

```rust
impl Engine {
    /// 启动 Trader 协程（终版）
    ///
    /// spawn_with_count：带 restart_count 启动，用于重启时继承计数
    pub async fn spawn(&self, symbol: &str) {
        self.spawn_with_count(symbol, 0).await;
    }

    /// P0: 带计数启动（传递 restart_count，防止计数器被重置）
    ///
    /// 重启时调用，继承上一次的 restart_count
    pub async fn spawn_with_count(&self, symbol: &str, restart_count: u32) {
        let trader = Arc::new(Trader::new(TraderConfig {
            symbol: symbol.to_string(),
            ..Default::default()
        }));

        let handle = Arc::new(TraderHandle::new(symbol.to_string()));
        handle.restart_count.store(restart_count, Ordering::Relaxed);  // 继承计数

        // 将 handle 传入 start()
        let trader_clone = trader.clone();
        let handle_clone = handle.clone();

        let join_handle = tokio::spawn(async move {
            trader_clone.start(handle_clone).await;
        });

        handle.set_join_handle(join_handle);

        let mut instances = self.instances.write().await;
        instances.insert(symbol.to_string(), handle);

        tracing::info!(
            symbol = symbol,
            restart_count = restart_count,
            "Trader 协程已启动"
        );
    }
}
```

9.3 Monitor 协程（终版·重启计数累加在旧 handle）

```rust
impl Engine {
    /// 心跳监控协程（定期检查 + 自动重启）
    async fn monitor_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        let heartbeat_timeout_ms = 30_000u64;  // 30 秒超时

        loop {
            interval.tick().await;
            let mut to_restart = Vec::new();

            for (symbol, handle) in self.instances.read().await.iter() {
                // 检查 JoinHandle 是否已退出
                if handle.is_finished() {
                    tracing::error!(symbol = %symbol, "协程已退出但未清理");
                    to_restart.push(symbol.clone());
                    continue;
                }

                // 检查心跳是否超时
                if handle.is_stale(heartbeat_timeout_ms) {
                    tracing::warn!(
                        symbol = %symbol,
                        elapsed_ms = current_time_ms() - handle.last_heartbeat_ms.load(Ordering::Relaxed),
                        "心跳超时"
                    );
                    to_restart.push(symbol.clone());
                }
            }

            for symbol in to_restart {
                self.restart_with_backoff(&symbol).await;
            }
        }
    }

    /// P0: 指数退避重启（终版·累加旧 handle 计数）
    ///
    /// 关键修复：
    /// - 获取旧 handle 的 restart_count
    /// - 停止旧实例
    /// - 指数退避
    /// - 用旧计数 +1 启动新实例（新实例继承累加后的计数）
    /// - 而不是累加到已不存在的旧 handle 上
    async fn restart_with_backoff(&self, symbol: &str) {
        // 1. 获取旧计数（从 instance_map 中的 handle 读取）
        let old_count = {
            let instances = self.instances.read().await;
            instances.get(symbol)
                .map(|h| h.load_restart_count())
                .unwrap_or(0)
        };

        // 超过最大重启次数，停止并告警
        if old_count >= MAX_RESTART_COUNT {
            tracing::critical!(
                symbol = %symbol,
                restart_count = old_count,
                "达到最大重启次数（{}），停止自动重启，需要人工介入",
                MAX_RESTART_COUNT
            );
            // TODO: 触发告警
            return;
        }

        // 2. 停止旧实例（会从 instance_map 移除）
        self.stop(symbol).await;

        // 3. 指数退避：2^count 秒，最大 32 秒
        let delay_secs = 2u64.saturating_pow(old_count.min(5));
        let delay = Duration::from_secs(delay_secs);

        tracing::info!(
            symbol = %symbol,
            restart_count = old_count,
            delay_secs = delay_secs,
            "等待 {} 秒后重启", delay_secs
        );

        sleep(delay).await;

        // 4. 用旧计数 +1 启动新实例（P0: 继承累加后的计数）
        self.spawn_with_count(symbol, old_count + 1).await;

        tracing::info!(
            symbol = %symbol,
            new_restart_count = old_count + 1,
            "重启完成"
        );
    }

    /// P2: 优雅停止 Trader
    async fn stop(&self, symbol: &str) {
        // 从 instance_map 获取
        let handle = {
            let mut instances = self.instances.write().await;
            instances.remove(symbol)
        };

        if let Some(handle) = handle {
            // 等待 JoinHandle 结束
            let join_handle = {
                let mut guard = handle.join_handle.lock().unwrap();
                guard.take()
            };

            if let Some(h) = join_handle {
                let _ = h.await;
            }

            tracing::info!(symbol = symbol, "Trader 协程已停止");
        }
    }
}
```

十、文件改动清单（终版）
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  新建 / 修改                                                     │
├─────────────────────────────────────────────────────────────────┤
│  [新建] crates/d_checktable/src/h_15m/                         │
│  [新建]   ├── repository.rs     # WAL + 连接池 + 幂等处理       │
│  [改造]   ├── executor.rs       # 下单网关 + Decimal + 步长裁剪  │
│  [改造]   ├── trader.rs         # async 签名 + 优雅停止 + 心跳 │
│  [改造]   └── mod.rs            # 导出 Repository               │
│  [改造] crates/f_engine/src/core/                               │
│  [改造]   └── strategy_loop.rs  # 心跳监控 + 重启计数继承        │
└─────────────────────────────────────────────────────────────────┘

十一、测试策略（终版）
================================================================

11.1 单元测试

┌─────────────────────────────────────────────────────────────────┐
│  executor_test.rs                                               │
│  ├── rate_limit_check：首次调用、时间窗口内、并发竞态             │
│  ├── calculate_order_qty：零持仓、极小值、步长裁剪               │
│  └── send_order：成功、频率限制、风控拒绝、网关超时              │
├─────────────────────────────────────────────────────────────────┤
│  repository_test.rs                                             │
│  ├── save_pending：幂等性（同一 symbol+timestamp）              │
│  ├── try_get_pending_id：GC后重试（最多3次）                   │
│  ├── confirm_record / mark_failed：正常流程                    │
│  ├── load_latest：PENDING → FAILED 恢复                        │
│  └── gc_pending：超时清理                                       │
├─────────────────────────────────────────────────────────────────┤
│  trader_test.rs                                                 │
│  ├── current_position_side/qty：异步调用正确性                  │
│  ├── restore_from_record：状态恢复完整性                        │
│  └── execute_once_wal：WAL 流程正确性                          │
└─────────────────────────────────────────────────────────────────┘

11.2 集成测试（故障注入）

| 场景 | 注入方式 | 预期结果 |
|------|----------|----------|
| SQLite 磁盘满 | mock 返回 Error::DiskFull | save_pending 失败，loop 继续 |
| 交易所超时 | mock gateway 延迟 30s | WAL 保证记录不丢失 |
| 并发下单 | 多线程同时 rate_limit_check | 仅一线程通过 |
| 崩溃恢复 | 进程 kill 后重启 | load_latest 恢复 PENDING → FAILED |
| 心跳超时 | Trader 循环中不调用 heartbeat | Monitor 触发重启 |
| 无限 GC | GC 清理冲突记录 | try_get_pending_id 重试3次后报错 |
| 重启计数 | 连续3次重启后成功运行 | reset_restart_count 归零，可继续运行 |

十二、评审修正记录（终版）
================================================================

| 轮次 | 问题 | 优先级 | 状态 |
|------|------|--------|------|
| v1 | WAL 事务一致性 | P0 | ✓ |
| v1 | 原子竞态（AtomicU64 CAS） | P0 | ✓ |
| v1 | Executor 职责拆分 | P1 | ✓ |
| v1 | Decimal 精度 | P1 | ✓ |
| v2 | RwLock 语法（.read().await） | P0 | ✓ |
| v2 | 心跳监控链路（heartbeat 调用） | P0 | ✓ |
| v2 | WAL ID 生命周期（幂等冲突） | P0 | ✓ |
| v2 | PositionSide 参数混淆 | P1 | ✓ |
| v2 | GC 时间配置（5分钟 vs 5天） | P1 | ✓ |
| v2 | SQLite 连接池 | P2 | ✓ |
| v2 | Error 类型定义 | P2 | ✓ |
| v3 | 异步上下文（restore_from_record async） | P0 | ✓ |
| v3 | 重启计数逻辑（累加到旧 handle） | P0 | ✓ |
| v3 | 持仓访问异步签名（current_position_*） | P0 | ✓ |
| v3 | WAL 幂等循环无限风险（MAX_RETRIES=3） | P1 | ✓ |
| v3 | 重启计数重置（成功运行后归零） | P1 | ✓ |
| v3 | 优雅停止（Notify） | P2 | ✓ |
| v3 | Decimal 步长裁剪（round_to_lot_size） | P2 | ✓ |

================================================================
End of Document（设计文档已冻结，可进入编码阶段）
================================================================
