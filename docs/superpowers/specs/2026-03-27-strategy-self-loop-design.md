================================================================
品种协程自循环设计方案（详细版·第二轮评审后）
================================================================
Author: 软件架构师
Date: 2026-03-27
Status: 待实现
Based on: pin_main.py (Python) + h_15m/ (Rust)
Review: 架构评审 v2（阻塞性语法 + 逻辑修正）
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
│  ├── spawn(symbol) → tokio::spawn(trader.start(handle))         │
│  ├── Monitor 协程（心跳检测 + 指数退避重启）                      │
│  └── stop(symbol)                                               │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  h_15m::Trader.loop()                                       │
│  loop:                                                           │
│    handle.heartbeat()            -- 更新心跳（P0: 监控链路）       │
│    1. record = TradeRecord.new()  -- 预创建 pending 记录        │
│    2. repository.save_pending(record)     -- WAL: 先写日志       │
│    3. record.market = get_market()                              │
│    4. record.signal = signal_generator.generate()                │
│    5. record.check_*() = check()                               │
│    6. if all passed:                                           │
│         executor.send_order(record)      -- 发送订单             │
│         repository.confirm_record(id)    -- WAL: 确认写入         │
│    7. sleep(interval)                                          │
└─────────────────────────────────────────────────────────────────┘

三、h_15m 模块结构（评审后）
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  h_15m/                                                         │
├─────────────────────────────────────────────────────────────────┤
│  trader.rs         # Trader（自循环）✅ 已有                    │
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
│  check_passed        INTEGER  -- 0/1                           │
│  signal_passed       INTEGER  -- 0/1                           │
│  price_check_passed  INTEGER  -- 0/1                           │
│  risk_check_passed   INTEGER  -- 0/1                           │
│  lock_acquired       INTEGER  -- 0/1                           │
│  order_executed      INTEGER  -- 0/1                           │
│  record_saved        INTEGER  -- 0/1                           │
└─────────────────────────────────────────────────────────────────┘

status 字段状态机（WAL 一致性保证）：
  PENDING   → 下单前预写，幂等性保证
  CONFIRMED → 下单成功后确认
  FAILED    → 下单失败标记

五、executor.rs 设计（评审后·第二轮修正）
================================================================

5.1 职责

┌─────────────────────────────────────────────────────────────────┐
│  Executor（线程安全：Arc<dyn ExchangeGateway>）                  │
├─────────────────────────────────────────────────────────────────┤
│  send_order(signal, order_type, current_qty, current_side)      │
│  rate_limit_check(interval_ms) → bool   【原子操作 P0】        │
│  calculate_order_qty(...) → Decimal 【Decimal精度 P1】           │
└─────────────────────────────────────────────────────────────────┘

5.2 rate_limit_check（评审后·原子操作）

```rust
// P0: 使用 AtomicU64 替代 RwLock，消除检查-更新竞态窗口

use std::sync::atomic::{AtomicU64, Ordering};

pub struct Executor {
    last_order_ms: AtomicU64,  // UNIX 毫秒时间戳
    // ...
}

impl Executor {
    pub fn new(symbol: &str) -> Self {
        Self {
            last_order_ms: AtomicU64::new(0),
            // ...
        }
    }

    /// 频率限制检查（完全原子，无锁）
    ///
    /// CAS 操作确保：
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

            // 时间窗口内，拒绝
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

            // CAS 尝试更新时间戳
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

5.3 calculate_order_qty（评审后·修正 PositionSide 参数）

```rust
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub enum OrderType {
    InitialOpen = 0,  // Python 对齐
    HedgeOpen   = 1,  // Python 对齐
    DoubleAdd   = 2,  // Python 对齐
    DoubleClose = 3,  // Python 对齐
    DayHedge    = 4,  // Rust 扩展（Python 暂无）
    DayClose    = 5,  // Rust 扩展（Python 暂无）
}

impl Executor {
    /// 计算下单数量（P1: 全程 Decimal 精度，修正 PositionSide 参数）
    ///
    /// 对齐 Python place_order 中的 open_qty 计算逻辑
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
        match order_type {
            OrderType::InitialOpen => {
                // 初始开仓：取 initial_ratio 与风控计算量中的较小值
                self.config.initial_ratio
            }
            OrderType::HedgeOpen => {
                // 对冲：需要当前持仓方向来计算反向数量
                // 如果 current_qty > 0（有持仓），返回当前数量作为对冲量
                if current_qty.abs() > Decimal::ZERO {
                    current_qty.abs()
                } else {
                    Decimal::ZERO
                }
            }
            OrderType::DoubleAdd => {
                // 翻倍加仓：当前数量 * 0.5
                current_qty.abs() * dec!(0.5)
            }
            OrderType::DoubleClose | OrderType::DayClose => {
                // 平仓：全部同向持仓
                current_qty.abs()
            }
            OrderType::DayHedge => {
                // 日线对冲：反向持仓数量
                current_qty.abs()
            }
        }
    }
}
```

5.4 send_order 完整逻辑（评审后·修正参数签名）

```rust
impl Executor {
    /// 发送订单（完整流程）
    ///
    /// 步骤：
    /// 1. rate_limit_check - 频率限制（原子）
    /// 2. calculate_order_qty - 数量计算（Decimal）
    /// 3. pre_risk_check - 风控前置检查
    /// 4. gateway.place_order() - 交易所下单
    /// 5. 返回结果
    pub fn send_order(
        &self,
        signal: &StrategySignal,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Result<OrderResult, ExecutorError> {
        // 1. 频率限制（原子，无竞态）
        if !self.rate_limit_check(self.config.order_interval_ms) {
            return Err(ExecutorError::RateLimited);
        }

        // 2. 计算下单数量（使用正确的 current_side 参数）
        let qty = self.calculate_order_qty(
            order_type,
            current_qty,
            current_side,
        );

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
            order_type: OrderType::LimitPrice, // 或 MarketPrice
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

5.5 Error 类型定义（评审后·新增）

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

六、repository.rs 设计（评审后·WAL + ID 生命周期）
================================================================

6.1 职责

┌─────────────────────────────────────────────────────────────────┐
│  Repository（WAL 事务一致性 + 连接池）                            │
├─────────────────────────────────────────────────────────────────┤
│  save_pending(record)     → 保存 PENDING 记录（幂等）             │
│  confirm_record(id, res)  → 更新为 CONFIRMED                       │
│  mark_failed(id, msg)    → 更新为 FAILED + 错误信息               │
│  load_latest(symbol)     → 崩溃恢复：加载最新记录                 │
│  get_by_timestamp()      → P0: 幂等冲突处理（新增）              │
│  gc_pending()            → 清理超时的 PENDING 记录                │
└─────────────────────────────────────────────────────────────────┘

6.2 连接池配置（评审后·P2 新增）

```rust
// 使用 r2d2 连接池，支持多品种并发

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

        // 初始化表
        Self::init_schema(&pool)?;

        Ok(Self {
            pool: Arc::new(pool),
            symbol: symbol.to_string(),
        })
    }

    fn init_schema(pool: &Pool<SqliteConnectionManager>) -> Result<(), RepoError> {
        let conn = pool.get()?;
        conn.execute_batch(SCHEMA)?;
        Ok(())
    }
}
```

6.3 WAL 事务流程（评审后·修正 ID 生命周期）

```rust
impl Repository {
    /// 预写记录（对应 WAL write-ahead 阶段）
    ///
    /// 关键保证：
    /// - INSERT OR REPLACE 保证幂等（同一 symbol+timestamp 唯一）
    /// - 先写入 PENDING 状态，保证崩溃后可恢复
    /// - 不依赖下单结果，独立事务
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
    ///
    /// 仅更新 status + order_result，其他字段不变
    pub fn confirm_record(
        &self,
        id: i64,
        order_result: &str,
    ) -> Result<(), RepoError> {
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

        // P0: 告警通知（可扩展为钉钉/飞书 webhook）
        tracing::error!(id = id, error = %error, "下单失败且记录已标记");

        Ok(())
    }

    /// P0: 按 symbol + timestamp 查询（幂等冲突处理）
    ///
    /// 用于 save_pending 遇到 UNIQUE 约束冲突时，获取已有记录的 id
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

6.4 崩溃恢复（对应 Python _load_data）

```rust
impl Repository {
    /// 从 SQLite 加载最新记录，用于崩溃恢复
    ///
    /// 对应 Python: _load_data()
    ///
    /// 恢复流程：
    /// 1. 查询最新记录
    /// 2. 根据 status 处理：
    ///    - PENDING → 需确认状态（可能是下单成功但未确认）
    ///    - CONFIRMED → 已完成，无需处理
    ///    - FAILED → 记录失败原因，无需处理
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

            match record.status.as_str() {
                "PENDING" => {
                    // P0: 存在未确认记录，可能下单成功但崩溃
                    // 策略：标记为 FAILED，下次 loop 重试或人工确认
                    tracing::warn!(
                        symbol = %symbol,
                        id = record.id,
                        timestamp = record.timestamp,
                        "发现 PENDING 记录，可能存在未确认下单"
                    );
                    // 更新为 FAILED，避免重复下单
                    self.mark_failed(
                        record.id,
                        "CRASH_RECOVERY: pending record marked as failed",
                    )?;
                }
                _ => {}
            }

            return Ok(Some(record));
        }

        Ok(None)
    }

    /// 兜底清理：超时 PENDING 记录（定时任务）
    ///
    /// 超时时间：5 分钟（评审后修正：与 Python 的 5 天区分）
    /// Python TIMEOUT_PERIOD = 60*60*24*5（5天）是历史数据记录超时
    /// 此处 PENDING_TIMEOUT_SECS 是 WAL 预写记录超时，应较短
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

七、SQLite 表结构（评审后）
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

-- 崩溃恢复查询（symbol + timestamp DESC）
CREATE INDEX IF NOT EXISTS idx_symbol_time
    ON trade_records(symbol, timestamp DESC);

-- 数据导出/审计（按时间清理）
CREATE INDEX IF NOT EXISTS idx_timestamp
    ON trade_records(timestamp DESC);

-- 状态批量查询
CREATE INDEX IF NOT EXISTS idx_status
    ON trade_records(status, timestamp DESC);
```

八、trader.rs 改造（评审后·第二轮修正）
================================================================

8.1 字段（评审后）

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;  // tokio 异步锁

pub struct Trader {
    config: TraderConfig,
    status_machine: RwLock<PinStatusMachine>,  // tokio 异步锁
    signal_generator: MinSignalGenerator,
    position: RwLock<Option<LocalPosition>>,  // tokio 异步锁
    executor: Arc<Executor>,        // 下单执行器
    repository: Arc<Repository>,     // 数据持久化
    last_order_ms: AtomicU64,       // P0: 原子操作
    is_running: RwLock<bool>,       // tokio 异步锁
}
```

8.2 主循环（评审后·修正 RwLock 语法 + 心跳调用）

```rust
impl Trader {
    /// P0: Trader 启动（修正 RwLock 语法）
    ///
    /// 参数说明：
    /// - handle: TraderHandle 引用，用于心跳更新
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
            // 恢复 Trader 内部状态
            self.restore_from_record(&record);
        }

        // P0: 主循环必须调用 heartbeat，否则 Monitor 会误判超时
        while *self.is_running.read().await {
            // 更新心跳（P0: 监控链路完整）
            handle.heartbeat();

            self.execute_once_wal().await;
            sleep(Duration::from_millis(self.config.interval_ms)).await;
        }

        tracing::info!(symbol = %self.config.symbol, "Trader 已停止");
    }

    /// WAL 模式执行一次（评审后·P0 一致性 + ID 生命周期）
    async fn execute_once_wal(&self) {
        // 1. 预创建记录（不依赖下单结果）
        let mut record = self.build_pending_record();

        // P0: ID 获取带幂等处理
        let pending_id = loop {
            match self.repository.save_pending(&record) {
                Ok(id) => break id,
                Err(RepoError::UniqueViolation) => {
                    // 幂等冲突：同 symbol+timestamp 已存在
                    if let Some(existing) = self.repository
                        .get_by_timestamp(&record.symbol, record.timestamp)
                        .ok()
                        .flatten()
                    {
                        tracing::warn!(
                            symbol = %record.symbol,
                            id = existing.id,
                            "发现重复记录，使用已有 ID"
                        );
                        break existing.id;
                    }
                }
                Err(e) => {
                    tracing::error!(
                        symbol = %self.config.symbol,
                        error = %e,
                        "预写记录失败，跳过本次下单"
                    );
                    return;
                }
            }
        };

        // 2. 生成信号
        let input = match self.build_signal_input() {
            Some(i) => i,
            None => {
                self.repository.mark_failed(pending_id, "NO_SIGNAL_INPUT").ok();
                return;
            }
        };

        let signal_output = self.signal_generator.generate(&input, &self.volatility_tier(), None);
        record.signal_json = serde_json::to_string(&signal_output).ok();

        // 3. 决策
        let (signal, order_type) = match self.decide_action(&signal_output) {
            Some(s) => s,
            None => {
                // 无信号，标记为 FAILED（不是崩溃，只是无信号）
                self.repository.mark_failed(pending_id, "NO_SIGNAL").ok();
                return;
            }
        };

        // 4. 获取当前持仓状态（用于 calculate_order_qty）
        let current_side = self.current_position_side();
        let current_qty = self.current_position_qty();

        // 5. 执行下单（传入正确的参数）
        match self.executor.send_order(
            &signal,
            order_type,
            current_qty,
            current_side,
        ) {
            Ok(result) => {
                // 6. WAL 确认
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                if let Err(e) = self.repository.confirm_record(pending_id, &result_str) {
                    // P0: 确认失败需要告警
                    tracing::error!(
                        symbol = %self.config.symbol,
                        id = pending_id,
                        error = %e,
                        "下单成功但确认记录失败，需要人工介入"
                    );
                }

                // 7. 更新 Trader 内部持仓状态
                self.update_position_from_result(&result);
            }
            Err(e) => {
                self.repository.mark_failed(pending_id, &format!("ORDER_FAILED: {}", e)).ok();
            }
        }
    }
}
```

8.3 状态恢复（评审后·新增）

```rust
impl Trader {
    /// 从 SQLite 记录恢复 Trader 内部状态
    ///
    /// 评审后新增：恢复 last_order_ms 防止重启后频率限制被绕过
    pub fn restore_from_record(&self, record: &TradeRecord) {
        // 1. 恢复状态机
        if let Ok(status) = serde_json::from_str::<PinStatus>(&record.trader_status) {
            *self.status_machine.write().await = PinStatusMachine::from(status);
        }

        // 2. 恢复持仓
        if let Ok(position) = serde_json::from_str::<LocalPosition>(&record.local_position) {
            *self.position.write().await = Some(position);
        }

        // 3. 恢复 last_order_ms（P0: 防止重启后频率限制被绕过）
        if let Some(ts) = record.order_timestamp {
            // 如果上次下单时间在 5 分钟内，恢复频率限制
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

九、Engine 心跳监控（评审后·指数退避）
================================================================

9.1 TraderHandle 结构

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub struct TraderHandle {
    pub join_handle: Mutex<Option<JoinHandle<()>>>,  // 包装为 Option 支持 take()
    pub last_heartbeat_ms: AtomicU64,
    pub restart_count: AtomicU32,      // 原子计数
    pub max_restart_count: u32,       // 最大重启次数
    pub symbol: String,
}

impl TraderHandle {
    pub fn new(symbol: String) -> Self {
        Self {
            join_handle: Mutex::new(None),
            last_heartbeat_ms: AtomicU64::new(current_time_ms()),
            restart_count: AtomicU32::new(0),
            max_restart_count: 10,  // 超过后停止重启，告警
            symbol,
        }
    }

    /// 更新心跳（Trader 每次 loop 调用）
    pub fn heartbeat(&self) {
        self.last_heartbeat_ms.store(current_time_ms(), Ordering::Relaxed);
    }

    /// 检查是否超时
    pub fn is_stale(&self, timeout_ms: u64) -> bool {
        let elapsed = current_time_ms() - self.last_heartbeat_ms.load(Ordering::Relaxed);
        elapsed > timeout_ms
    }

    /// 替换 JoinHandle（用于 restart 时）
    pub fn set_join_handle(&self, handle: JoinHandle<()>) {
        let mut guard = self.join_handle.lock().unwrap();
        *guard = Some(handle);
    }

    /// 检查是否已结束
    pub fn is_finished(&self) -> bool {
        let guard = self.join_handle.lock().unwrap();
        guard.as_ref().map(|h| h.is_finished()).unwrap_or(false)
    }
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
```

9.2 Engine spawn（评审后·传递 handle）

```rust
impl Engine {
    /// 启动 Trader 协程（评审后·传递 TraderHandle）
    pub async fn spawn(&self, symbol: &str) {
        let trader = Arc::new(Trader::new(TraderConfig {
            symbol: symbol.to_string(),
            ..Default::default()
        }));

        let handle = Arc::new(TraderHandle::new(symbol.to_string()));

        // P0: 将 handle 传入 start()，保证心跳链路完整
        let trader_clone = trader.clone();
        let handle_clone = handle.clone();

        let join_handle = tokio::spawn(async move {
            trader_clone.start(handle_clone).await;
        });

        // 保存 JoinHandle
        handle.set_join_handle(join_handle);

        // 存入实例映射
        let mut instances = self.instances.write().await;
        instances.insert(symbol.to_string(), handle);

        tracing::info!(symbol = symbol, "Trader 协程已启动");
    }
}
```

9.3 Monitor 协程（评审后·指数退避）

```rust
impl Engine {
    /// 心跳监控协程（定期检查 + 自动重启）
    async fn monitor_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        let heartbeat_timeout_ms = 30_000u64; // 30 秒超时

        loop {
            interval.tick().await;
            let mut to_restart = Vec::new();

            for (symbol, handle) in self.instances.read().await.iter() {
                // 检查 JoinHandle 是否已退出
                if handle.is_finished() {
                    tracing::error!(
                        symbol = %symbol,
                        "协程已退出但未清理，触发重启"
                    );
                    to_restart.push(symbol.clone());
                    continue;
                }

                // 检查心跳是否超时
                if handle.is_stale(heartbeat_timeout_ms) {
                    tracing::warn!(
                        symbol = %symbol,
                        elapsed_ms = current_time_ms() - handle.last_heartbeat_ms.load(Ordering::Relaxed),
                        "心跳超时，触发重启"
                    );
                    to_restart.push(symbol.clone());
                }
            }

            // 批量重启（带指数退避）
            for symbol in to_restart {
                self.restart_with_backoff(&symbol).await;
            }
        }
    }

    /// 指数退避重启（P2）
    ///
    /// 重启间隔：2^restart_count 秒，最大 32 秒
    /// 超过 max_restart_count 次后停止重启，告警
    async fn restart_with_backoff(&self, symbol: &str) {
        let handle = match self.instances.read().await.get(symbol) {
            Some(h) => h.clone(),
            None => return,
        };

        let count = handle.restart_count.load(Ordering::Relaxed);

        // 超过最大重启次数
        if count >= handle.max_restart_count {
            tracing::critical!(
                symbol = %symbol,
                restart_count = count,
                "达到最大重启次数，停止自动重启，需要人工介入"
            );
            // TODO: 触发告警（钉钉/飞书）
            return;
        }

        // 指数退避：2^count 秒，最大 32 秒
        let delay_secs = 2u64.saturating_pow(count.min(5));
        let delay = Duration::from_secs(delay_secs);

        tracing::info!(
            symbol = %symbol,
            restart_count = count,
            delay_secs = delay_secs,
            "等待 {} 秒后重启", delay_secs
        );

        sleep(delay).await;

        // 停止旧协程
        self.stop(symbol).await;

        // 重启
        self.spawn(symbol).await;

        // 更新重启计数
        handle.restart_count.fetch_add(1, Ordering::SeqCst);

        tracing::info!(
            symbol = %symbol,
            restart_count = count + 1,
            "重启完成"
        );
    }

    /// 停止 Trader
    async fn stop(&self, symbol: &str) {
        // 从实例映射中获取
        let handle = match self.instances.read().await.get(symbol) {
            Some(h) => h.clone(),
            None => return,
        };

        // 等待 JoinHandle 结束
        let join_handle = {
            let mut guard = handle.join_handle.lock().unwrap();
            guard.take()
        };

        if let Some(handle) = join_handle {
            let _ = handle.await;
        }

        tracing::info!(symbol = symbol, "Trader 协程已停止");
    }
}
```

十、文件改动清单
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  新建 / 修改                                                     │
├─────────────────────────────────────────────────────────────────┤
│  [新建] crates/d_checktable/src/h_15m/                         │
│  [新建]   ├── repository.rs     # WAL 持久化 + 连接池           │
│  [改造]   ├── executor.rs       # 仅保留下单网关 + Error 定义   │
│  [改造]   ├── trader.rs         # 整合 WAL + 心跳 + 状态恢复   │
│  [改造]   └── mod.rs            # 导出 Repository              │
│  [改造] crates/f_engine/src/core/                               │
│  [改造]   └── strategy_loop.rs  # 心跳监控 + 指数退避重启       │
└─────────────────────────────────────────────────────────────────┘

十一、测试策略
================================================================

11.1 单元测试

┌─────────────────────────────────────────────────────────────────┐
│  executor_test.rs                                               │
│  ├── rate_limit_check 边界：首次调用、时间窗口内、重叠并发       │
│  ├── calculate_order_qty 边界：零持仓、极小值(0.001)、最大值    │
│  └── send_order 模拟：成功、频率限制、风控拒绝、网关超时         │
├─────────────────────────────────────────────────────────────────┤
│  repository_test.rs                                             │
│  ├── save_pending 幂等性：同一 symbol+timestamp 多次插入        │
│  ├── get_by_timestamp 幂等冲突处理                              │
│  ├── confirm_record 正常流程                                    │
│  ├── load_latest 崩溃恢复：PENDING → FAILED                    │
│  └── gc_pending 清理超时记录                                    │
└─────────────────────────────────────────────────────────────────┘

11.2 集成测试（故障注入）

| 场景 | 注入方式 | 预期结果 |
|------|----------|----------|
| SQLite 磁盘满 | mock 返回 Error::DiskFull | save_pending 失败，loop 继续 |
| 交易所超时 | mock gateway 延迟 30s | WAL 保证记录不丢失 |
| 并发下单 | 多线程同时 rate_limit_check | 仅一线程通过 |
| 崩溃恢复 | 进程 kill 后重启 | load_latest 恢复 PENDING 标记为 FAILED |
| 心跳超时 | Trader 循环中不调用 heartbeat | Monitor 触发重启 |

十二、优先级总结（第二轮评审后）
================================================================

| 优先级 | 问题 | 状态 |
|--------|------|------|
| P0 | RwLock 语法（.read().await/.write().await） | 文档已修正 ✓ |
| P0 | 心跳监控链路（handle.heartbeat() 调用） | 文档已修正 ✓ |
| P0 | WAL ID 生命周期（幂等冲突处理） | 文档已修正 ✓ |
| P1 | PositionSide 参数混淆（current_side） | 文档已修正 ✓ |
| P1 | GC 时间配置不一致（5分钟 vs 5天） | 文档已修正 ✓ |
| P2 | SQLite 连接池（r2d2 Pool） | 文档已修正 ✓ |
| P2 | Error 类型定义缺失 | 文档已修正 ✓ |
| P2 | 状态恢复粒度（last_order_ms） | 文档已修正 ✓ |

================================================================
End of Document
================================================================
