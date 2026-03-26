# h_15m 策略自循环模块实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `crates/d_checktable/src/h_15m/` 下新建 `repository.rs` 和改造 `executor.rs`/`trader.rs`，实现品种协程自循环交易系统（基于设计文档 `docs/superpowers/specs/2026-03-27-strategy-self-loop-design.md`）

**Architecture:**
- `repository.rs` — SQLite WAL 持久化层（r2d2 连接池 + PENDING/CONFIRMED/FAILED 状态机）
- `executor.rs` — 下单网关（AtomicU64 CAS 频率限制 + Decimal 步长裁剪）
- `trader.rs` — 流程编排（tokio::select 优雅停止 + WAL 整合 + 心跳更新）
- `strategy_loop.rs` — Engine 心跳监控（指数退避重启 + restart_count 继承）

**Tech Stack:** Rust (Tokio async runtime, parking_lot, rust_decimal, rusqlite, r2d2, thiserror, serde, tracing, chrono)

---

## 前置检查：依赖和目录

首先确认目录存在且 Cargo.toml 包含必要依赖：

**Modify:** `crates/d_checktable/Cargo.toml`

```toml
[package]
name = "d_checktable"
version = "0.1.0"
edition = "2024"

[dependencies]
# ... 已有依赖 ...
rusqlite = { version = "0.32", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.25"
```

- [ ] **Step 1: 确认目录结构**

Run: `ls crates/d_checktable/src/h_15m/`
Expected: `mod.rs signal.rs signal.rs status.rs quantity_calculator.rs trader.rs`

---

## Phase 1: repository.rs（WAL 持久化层）

### Task 1.1: 新建 repository.rs 文件骨架

**Files:**
- Create: `crates/d_checktable/src/h_15m/repository.rs`

```rust
//! h_15m/repository.rs
//!
//! SQLite WAL 持久化层 - TradeRecord 存储与崩溃恢复

#![forbid(unsafe_code)]

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 记录状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordStatus {
    PENDING,
    CONFIRMED,
    FAILED,
}

impl Default for RecordStatus {
    fn default() -> Self {
        RecordStatus::PENDING
    }
}

impl std::fmt::Display for RecordStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordStatus::PENDING => write!(f, "PENDING"),
            RecordStatus::CONFIRMED => write!(f, "CONFIRMED"),
            RecordStatus::FAILED => write!(f, "FAILED"),
        }
    }
}

/// 交易记录
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TradeRecord {
    pub id: Option<i64>,
    pub symbol: String,
    pub timestamp: i64,
    pub interval_ms: i64,
    pub status: RecordStatus,
    // 行情快照
    pub price: Option<String>,
    pub volatility: Option<f64>,
    pub market_status: Option<String>,
    // 持仓快照
    pub exchange_position: Option<String>,
    pub local_position: Option<String>,
    // 策略状态
    pub trader_status: Option<String>,
    pub signal_json: Option<String>,
    pub confidence: Option<i32>,
    // 账户状态
    pub realized_pnl: Option<String>,
    pub unrealized_pnl: Option<String>,
    pub available_balance: Option<String>,
    pub used_margin: Option<String>,
    // 订单执行
    pub order_type: Option<i32>,
    pub order_direction: Option<String>,
    pub order_quantity: Option<String>,
    pub order_result: Option<String>,
    pub order_timestamp: Option<i64>,
    // 检查表
    pub check_passed: i32,
    pub signal_passed: i32,
    pub price_check_passed: i32,
    pub risk_check_passed: i32,
    pub lock_acquired: i32,
    pub order_executed: i32,
    pub record_saved: i32,
}

/// WAL 超时清理时间（5分钟）
pub const PENDING_TIMEOUT_SECS: i64 = 300;

/// Repository 错误类型
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

    #[error("连接池错误: {0}")]
    Pool(#[from] r2d2::Error),
}

/// Repository - SQLite WAL 持久化层
pub struct Repository {
    pool: Arc<Pool<SqliteConnectionManager>>,
    symbol: String,
}

impl Repository {
    pub fn new(symbol: &str, db_path: &str) -> Result<Self, RepoError> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(10)
            .build(manager)?;

        // 初始化表结构
        Self::init_schema(&pool)?;

        Ok(Self {
            pool: Arc::new(pool),
            symbol: symbol.to_string(),
        })
    }

    /// 初始化数据库表
    fn init_schema(pool: &Pool<SqliteConnectionManager>) -> Result<(), RepoError> {
        let conn = pool.get()?;
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS trade_records (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol              TEXT NOT NULL,
                timestamp           INTEGER NOT NULL,
                interval_ms         INTEGER DEFAULT 100,
                status              TEXT DEFAULT 'PENDING',
                price               TEXT,
                volatility          REAL,
                market_status       TEXT,
                exchange_position   TEXT,
                local_position     TEXT,
                trader_status      TEXT,
                signal_json        TEXT,
                confidence          INTEGER,
                realized_pnl        TEXT,
                unrealized_pnl      TEXT,
                available_balance   TEXT,
                used_margin        TEXT,
                order_type         INTEGER,
                order_direction    TEXT,
                order_quantity     TEXT,
                order_result       TEXT,
                order_timestamp    INTEGER,
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
        "#)?;
        Ok(())
    }

    fn prepare(&self, sql: &str) -> Result<rusqlite::Statement, RepoError> {
        let conn = self.pool.get()?;
        Ok(conn.prepare(sql)?)
    }

    fn exec_insert(&self, sql: &str, record: &TradeRecord) -> Result<i64, RepoError> {
        let conn = self.pool.get()?;
        conn.execute(
            sql,
            params![
                record.symbol,
                record.timestamp,
                record.interval_ms,
                record.status.to_string(),
                record.price,
                record.volatility,
                record.market_status,
                record.exchange_position,
                record.local_position,
                record.trader_status,
                record.signal_json,
                record.confidence,
                record.realized_pnl,
                record.unrealized_pnl,
                record.available_balance,
                record.used_margin,
                record.order_type,
                record.order_direction,
                record.order_quantity,
                record.order_result,
                record.order_timestamp,
                record.check_passed,
                record.signal_passed,
                record.price_check_passed,
                record.risk_check_passed,
                record.lock_acquired,
                record.order_executed,
                record.record_saved,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn exec_update(&self, sql: &str, params: impl rusqlite::Params) -> Result<usize, RepoError> {
        let conn = self.pool.get()?;
        Ok(conn.execute(sql, params)?)
    }

    fn row_to_record(row: &rusqlite::Row) -> Result<TradeRecord, RepoError> {
        Ok(TradeRecord {
            id: Some(row.get(0)?),
            symbol: row.get(1)?,
            timestamp: row.get(2)?,
            interval_ms: row.get(3)?,
            status: match row.get::<_, String>(4)? .as_str() {
                "PENDING" => RecordStatus::PENDING,
                "CONFIRMED" => RecordStatus::CONFIRMED,
                "FAILED" => RecordStatus::FAILED,
                _ => RecordStatus::PENDING,
            },
            price: row.get(5)?,
            volatility: row.get(6)?,
            market_status: row.get(7)?,
            exchange_position: row.get(8)?,
            local_position: row.get(9)?,
            trader_status: row.get(10)?,
            signal_json: row.get(11)?,
            confidence: row.get(12)?,
            realized_pnl: row.get(13)?,
            unrealized_pnl: row.get(14)?,
            available_balance: row.get(15)?,
            used_margin: row.get(16)?,
            order_type: row.get(17)?,
            order_direction: row.get(18)?,
            order_quantity: row.get(19)?,
            order_result: row.get(20)?,
            order_timestamp: row.get(21)?,
            check_passed: row.get::<_, i32>(22)?,
            signal_passed: row.get::<_, i32>(23)?,
            price_check_passed: row.get::<_, i32>(24)?,
            risk_check_passed: row.get::<_, i32>(25)?,
            lock_acquired: row.get::<_, i32>(26)?,
            order_executed: row.get::<_, i32>(27)?,
            record_saved: row.get::<_, i32>(28)?,
        })
    }
}
```

- [ ] **Step 2: 创建 repository.rs 骨架文件**

Run: 创建上述文件
Expected: `crates/d_checktable/src/h_15m/repository.rs` 已创建

### Task 1.2: 实现 WAL 方法

**Files:**
- Modify: `crates/d_checktable/src/h_15m/repository.rs`（添加 impl 块）

```rust
impl Repository {
    /// 预写记录（WAL write-ahead 阶段）
    ///
    /// INSERT OR REPLACE 保证幂等：同一 symbol+timestamp 唯一
    pub fn save_pending(&self, record: &TradeRecord) -> Result<i64, RepoError> {
        let mut record = record.clone();
        record.status = RecordStatus::PENDING;

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

        match self.exec_insert(sql, &record) {
            Ok(id) => {
                tracing::debug!(id = id, symbol = %record.symbol, "预写记录 PENDING");
                Ok(id)
            }
            Err(rusqlite::Error::QueryReturnedRow(_) ) => Err(RepoError::UniqueViolation),
            Err(e) => {
                // 检测 UNIQUE 约束冲突
                if let Some(code) = e.extended_code() {
                    if code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE {
                        return Err(RepoError::UniqueViolation);
                    }
                }
                Err(RepoError::Database(e))
            }
        }
    }

    /// 确认记录（WAL commit 阶段）
    pub fn confirm_record(&self, id: i64, order_result: &str) -> Result<(), RepoError> {
        let sql = r#"
            UPDATE trade_records
            SET status = 'CONFIRMED',
                order_result = ?,
                record_saved = 1
            WHERE id = ?
        "#;
        self.exec_update(sql, params![order_result, id])?;
        tracing::info!(id = id, "记录已确认 CONFIRMED");
        Ok(())
    }

    /// 标记失败（WAL rollback 阶段）
    pub fn mark_failed(&self, id: i64, error: &str) -> Result<(), RepoError> {
        let sql = r#"
            UPDATE trade_records
            SET status = 'FAILED',
                order_result = ?,
                record_saved = 1
            WHERE id = ?
        "#;
        self.exec_update(sql, params![error, id])?;
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
        let mut rows = stmt.query(params![symbol, timestamp])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_record(row)?))
        } else {
            Ok(None)
        }
    }

    /// 从 SQLite 加载最新记录（崩溃恢复）
    pub fn load_latest(&self, symbol: &str) -> Result<Option<TradeRecord>, RepoError> {
        let sql = r#"
            SELECT * FROM trade_records
            WHERE symbol = ?
            ORDER BY timestamp DESC
            LIMIT 1
        "#;
        let mut stmt = self.prepare(sql)?;
        let mut rows = stmt.query(params![symbol])?;
        if let Some(row) = rows.next()? {
            let record = self.row_to_record(row)?;
            if record.status == RecordStatus::PENDING {
                tracing::warn!(
                    symbol = %symbol,
                    id = record.id,
                    timestamp = record.timestamp,
                    "发现 PENDING 记录，可能存在未确认下单"
                );
                self.mark_failed(
                    record.id.unwrap_or(0),
                    "CRASH_RECOVERY: pending record marked as failed",
                )?;
            }
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    /// 兜底清理：超时 PENDING 记录
    pub fn gc_pending(&self) -> Result<usize, RepoError> {
        let cutoff = chrono::Utc::now().timestamp() - PENDING_TIMEOUT_SECS;
        let sql = r#"
            UPDATE trade_records
            SET status = 'FAILED',
                order_result = 'GC_TIMEOUT: pending record cleaned up'
            WHERE status = 'PENDING'
              AND timestamp < ?
        "#;
        let affected = self.exec_update(sql, params![cutoff])?;
        if affected > 0 {
            tracing::warn!(count = affected, "清理了 {} 条超时 PENDING 记录", affected);
        }
        Ok(affected)
    }
}
```

- [ ] **Step 3: 添加 WAL 方法实现**

Run: 修改 repository.rs 添加 impl Repository 块
Expected: save_pending / confirm_record / mark_failed / get_by_timestamp / load_latest / gc_pending 已实现

### Task 1.3: 编写 repository_test.rs

**Files:**
- Create: `crates/d_checktable/src/h_15m/repository_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn temp_repo() -> (Repository, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let repo = Repository::new("BTCUSDT", db_path.to_str().unwrap()).unwrap();
        (repo, dir)
    }

    #[test]
    fn test_save_pending_and_load() {
        let (repo, _dir) = temp_repo();
        let mut record = TradeRecord {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1234567890,
            interval_ms: 100,
            ..Default::default()
        };

        let id = repo.save_pending(&record).unwrap();
        assert!(id > 0);

        let loaded = repo.load_latest("BTCUSDT").unwrap().unwrap();
        assert_eq!(loaded.id, Some(id));
        assert_eq!(loaded.status, RecordStatus::PENDING);
    }

    #[test]
    fn test_confirm_record() {
        let (repo, _dir) = temp_repo();
        let record = TradeRecord {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1234567890,
            ..Default::default()
        };

        let id = repo.save_pending(&record).unwrap();
        repo.confirm_record(id, "Order123").unwrap();

        let loaded = repo.load_latest("BTCUSDT").unwrap().unwrap();
        assert_eq!(loaded.status, RecordStatus::CONFIRMED);
        assert_eq!(loaded.order_result, Some("Order123".to_string()));
    }

    #[test]
    fn test_mark_failed() {
        let (repo, _dir) = temp_repo();
        let record = TradeRecord {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1234567890,
            ..Default::default()
        };

        let id = repo.save_pending(&record).unwrap();
        repo.mark_failed(id, "ORDER_FAILED: Gateway timeout").unwrap();

        let loaded = repo.load_latest("BTCUSDT").unwrap().unwrap();
        assert_eq!(loaded.status, RecordStatus::FAILED);
    }

    #[test]
    fn test_idempotency_same_timestamp() {
        let (repo, _dir) = temp_repo();
        let record = TradeRecord {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1234567890,
            ..Default::default()
        };

        let id1 = repo.save_pending(&record).unwrap();
        let id2 = repo.save_pending(&record).unwrap();

        // INSERT OR REPLACE 替换同一记录
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_gc_pending() {
        let (repo, _dir) = temp_repo();

        // 插入一个超时的 PENDING 记录（手动修改 timestamp）
        let conn = repo.pool.get().unwrap();
        conn.execute(
            "INSERT INTO trade_records (symbol, timestamp, status) VALUES (?, ?, 'PENDING')",
            params!["BTCUSDT", chrono::Utc::now().timestamp() - PENDING_TIMEOUT_SECS - 10],
        ).unwrap();

        let count = repo.gc_pending().unwrap();
        assert_eq!(count, 1);
    }
}
```

**注意：** 需要添加测试依赖到 Cargo.toml：

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: 添加 tempfile 测试依赖**

Run: 修改 `crates/d_checktable/Cargo.toml` 添加 tempfile dev-dependency
Expected: dev-dependencies 已添加

- [ ] **Step 5: 创建 repository_test.rs 并运行测试**

Run: `cd crates/d_checktable && cargo test --lib h_15m::repository_test -- --nocapture`
Expected: 5 tests PASS

---

## Phase 2: executor.rs（下单网关）

### Task 2.1: 新建 executor.rs

**Files:**
- Create: `crates/d_checktable/src/h_15m/executor.rs`

```rust
//! h_15m/executor.rs
//!
//! 下单网关 - 交易所交互 + 风控前置检查

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use x_data::position::PositionSide;

/// Executor 错误类型
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("频率限制")]
    RateLimited,

    #[error("数量为零")]
    ZeroQuantity,

    #[error("风控拒绝: {0}")]
    RiskCheckFailed(String),

    #[error("网关错误")]
    Gateway,

    #[error("超时: {0}")]
    Timeout(String),
}

/// 下单类型枚举（对齐 Python place_order order_type）
#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    InitialOpen = 0,
    HedgeOpen   = 1,
    DoubleAdd   = 2,
    DoubleClose = 3,
    DayHedge    = 4,
    DayClose    = 5,
}

/// Executor 配置
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub symbol: String,
    pub order_interval_ms: u64,
    pub initial_ratio: Decimal,
    pub lot_size: Decimal,
    pub max_position: Decimal,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 100,
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            max_position: dec!(0.15),
        }
    }
}

/// Executor - 下单网关
pub struct Executor {
    config: ExecutorConfig,
    last_order_ms: AtomicU64,
}

impl Executor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            config,
            last_order_ms: AtomicU64::new(0),
        }
    }

    /// 频率限制检查（原子操作，CAS 循环）
    pub fn rate_limit_check(&self, interval_ms: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        loop {
            let last = self.last_order_ms.load(Ordering::Relaxed);

            if now.saturating_sub(last) < interval_ms {
                tracing::warn!(
                    symbol = %self.config.symbol,
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
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    /// 计算下单数量（Decimal 精度 + 步长裁剪）
    pub fn calculate_order_qty(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Decimal {
        let raw_qty = match order_type {
            OrderType::InitialOpen => self.config.initial_ratio,
            OrderType::HedgeOpen => {
                if current_qty.abs() > Decimal::ZERO {
                    current_qty.abs()
                } else {
                    Decimal::ZERO
                }
            }
            OrderType::DoubleAdd => current_qty.abs() * dec!(0.5),
            OrderType::DoubleClose | OrderType::DayClose => current_qty.abs(),
            OrderType::DayHedge => current_qty.abs(),
        };

        self.round_to_lot_size(raw_qty)
    }

    /// 按步长裁剪数量
    fn round_to_lot_size(&self, qty: Decimal) -> Decimal {
        let step = self.config.lot_size;
        if step <= Decimal::ZERO {
            return qty;
        }
        (qty / step).floor() * step
    }

    /// 发送订单（完整流程）
    pub fn send_order(
        &self,
        order_type: OrderType,
        current_qty: Decimal,
        current_side: Option<PositionSide>,
    ) -> Result<(), ExecutorError> {
        // 1. 频率限制
        if !self.rate_limit_check(self.config.order_interval_ms) {
            return Err(ExecutorError::RateLimited);
        }

        // 2. 计算数量
        let qty = self.calculate_order_qty(order_type, current_qty, current_side);
        if qty <= Decimal::ZERO {
            tracing::warn!(
                symbol = %self.config.symbol,
                order_type = ?order_type,
                "计算下单数量为 0，跳过"
            );
            return Err(ExecutorError::ZeroQuantity);
        }

        // 3. 风控前置检查（TODO: 实际实现）
        // self.pre_risk_check(qty)?;

        tracing::info!(
            symbol = %self.config.symbol,
            order_type = ?order_type,
            qty = %qty,
            "下单请求"
        );

        Ok(())
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(ExecutorConfig::default())
    }
}
```

- [ ] **Step 6: 创建 executor.rs**

Run: 创建上述文件
Expected: `crates/d_checktable/src/h_15m/executor.rs` 已创建

### Task 2.2: 编写 executor_test.rs

**Files:**
- Create: `crates/d_checktable/src/h_15m/executor_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn executor() -> Executor {
        Executor::new(ExecutorConfig {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 100,
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            max_position: dec!(0.15),
        })
    }

    #[test]
    fn test_rate_limit_first_call() {
        let ex = executor();
        // 首次调用应通过
        assert!(ex.rate_limit_check(100));
    }

    #[test]
    fn test_rate_limit_within_window() {
        let ex = executor();
        ex.rate_limit_check(100).unwrap();
        // 立即调用应被限制
        assert!(!ex.rate_limit_check(100));
    }

    #[test]
    fn test_calculate_initial_open() {
        let ex = executor();
        let qty = ex.calculate_order_qty(OrderType::InitialOpen, Decimal::ZERO, None);
        assert_eq!(qty, dec!(0.05));
    }

    #[test]
    fn test_calculate_double_add() {
        let ex = executor();
        let qty = ex.calculate_order_qty(
            OrderType::DoubleAdd,
            dec!(0.1),
            Some(PositionSide::Long),
        );
        assert_eq!(qty, dec!(0.05));
    }

    #[test]
    fn test_lot_size_rounding() {
        let mut ex = executor();
        ex.config.lot_size = dec!(0.001);

        let qty = ex.calculate_order_qty(
            OrderType::InitialOpen,
            dec!(0.123456),
            None,
        );
        // 向下取整到 0.001
        assert_eq!(qty, dec!(0.123));
    }

    #[test]
    fn test_send_order_success() {
        let ex = executor();
        let result = ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_order_rate_limited() {
        let ex = executor();
        ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None).unwrap();
        // 第二次应被频率限制
        let result = ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None);
        assert!(matches!(result, Err(ExecutorError::RateLimited)));
    }
}
```

- [ ] **Step 7: 创建 executor_test.rs 并运行测试**

Run: `cd crates/d_checktable && cargo test --lib h_15m::executor_test -- --nocapture`
Expected: 7 tests PASS

---

## Phase 3: trader.rs（流程编排）

### Task 3.1: 改造 trader.rs - 添加新字段和 import

**Files:**
- Modify: `crates/d_checktable/src/h_15m/trader.rs`

在文件顶部添加 import：

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{RwLock as TokioRwLock, Notify};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
```

替换 Trader 结构体字段：

```rust
pub struct Trader {
    config: TraderConfig,
    status_machine: TokioRwLock<PinStatusMachine>,     // 改为 tokio 异步锁
    signal_generator: MinSignalGenerator,
    position: TokioRwLock<Option<LocalPosition>>,     // 改为 tokio 异步锁
    executor: Arc<Executor>,                          // 新增：下单执行器
    repository: Arc<Repository>,                      // 新增：数据持久化
    last_order_ms: AtomicU64,                        // 新增：原子操作
    is_running: TokioRwLock<bool>,                  // 改为 tokio 异步锁
    shutdown: Notify,                               // 新增：优雅停止信号
}
```

替换 Trader::new：

```rust
impl Trader {
    pub fn new(config: TraderConfig, executor: Arc<Executor>, repository: Arc<Repository>) -> Self {
        Self {
            config,
            status_machine: TokioRwLock::new(PinStatusMachine::new()),
            signal_generator: MinSignalGenerator::new(),
            position: TokioRwLock::new(None),
            executor,
            repository,
            last_order_ms: AtomicU64::new(0),
            is_running: TokioRwLock::new(false),
            shutdown: Notify::new(),
        }
    }
}
```

- [ ] **Step 8: 改造 trader.rs 字段和 import**

Run: 修改 trader.rs 替换字段和 import
Expected: Trader 结构体包含 executor / repository / last_order_ms / shutdown

### Task 3.2: 添加异步持仓访问方法

**Files:**
- Modify: `crates/d_checktable/src/h_15m/trader.rs`（在 impl Trader 块末尾添加）

```rust
    /// 获取当前持仓方向（异步）
    pub async fn current_position_side(&self) -> Option<PositionSide> {
        self.position.read().await.as_ref().map(|p| p.direction)
    }

    /// 获取当前持仓数量（异步）
    pub async fn current_position_qty(&self) -> Decimal {
        self.position.read().await.as_ref().map(|p| p.qty).unwrap_or_default()
    }

    /// 从记录恢复 Trader 状态（异步）
    pub async fn restore_from_record(&self, record: &crate::h_15m::repository::TradeRecord) {
        // 恢复状态机
        if let Some(ref status_str) = record.trader_status {
            if let Ok(status) = serde_json::from_str::<PinStatus>(status_str) {
                *self.status_machine.write().await = PinStatusMachine::from(status);
                tracing::info!(symbol = %self.config.symbol, ?status, "状态机已恢复");
            }
        }

        // 恢复持仓
        if let Some(ref pos_str) = record.local_position {
            if let Ok(position) = serde_json::from_str::<LocalPosition>(pos_str) {
                *self.position.write().await = Some(position);
                tracing::info!(symbol = %self.config.symbol, qty = %position.qty, "持仓已恢复");
            }
        }

        // 恢复频率限制
        if let Some(ts) = record.order_timestamp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            const RATE_LIMIT_INTERVAL_MS: u64 = 300_000; // 5分钟
            if now - (ts as u64) < RATE_LIMIT_INTERVAL_MS {
                self.last_order_ms.store(ts as u64, Ordering::Relaxed);
                tracing::info!(symbol = %self.config.symbol, last_order_ms = ts, "已恢复下单频率限制");
            }
        }
    }

    /// 停止 Trader（优雅停止）
    pub fn stop(&self) {
        *self.is_running.try_write().map(|mut g| { *g = false; }).unwrap_or(());
        self.shutdown.notify_waiters();
    }
}
```

- [ ] **Step 9: 添加异步持仓访问和状态恢复方法**

Run: 修改 trader.rs 添加 async 方法
Expected: `current_position_side` / `current_position_qty` / `restore_from_record` / `stop` 已添加

### Task 3.3: 改造主循环 start()

**Files:**
- Modify: `crates/d_checktable/src/h_15m/trader.rs`（替换原有的 start 方法）

```rust
    /// 启动交易循环（改造后：优雅停止 + 心跳 + WAL）
    pub async fn start(&self) {
        *self.is_running.write().await = true;
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
        while *self.is_running.read().await {
            tokio::select! {
                _ = self.shutdown.notified() => {
                    tracing::info!(symbol = %self.config.symbol, "收到停止信号");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(self.config.interval_ms)) => {
                    // TODO: 心跳更新 + WAL 执行
                    // handle.heartbeat();
                    // self.execute_once_wal().await;
                    tracing::debug!(symbol = %self.config.symbol, "Trader loop tick");
                }
            }
        }

        tracing::info!(symbol = %self.config.symbol, "Trader 已停止");
    }
```

- [ ] **Step 10: 改造 start() 方法为优雅停止循环**

Run: 修改 trader.rs start() 方法
Expected: tokio::select! 优雅停止模式

### Task 3.4: 更新 TraderConfig 添加字段

**Files:**
- Modify: `crates/d_checktable/src/h_15m/trader.rs`（扩展 TraderConfig）

```rust
/// 品种交易器配置
#[derive(Debug, Clone)]
pub struct TraderConfig {
    pub symbol: String,
    pub interval_ms: u64,
    pub max_position: Decimal,
    pub initial_ratio: Decimal,
    pub db_path: String,        // 新增：SQLite 数据库路径
    pub order_interval_ms: u64, // 新增：下单间隔
    pub lot_size: Decimal,      // 新增：步长
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
```

- [ ] **Step 11: 扩展 TraderConfig**

Run: 修改 trader.rs TraderConfig
Expected: TraderConfig 包含 db_path / order_interval_ms / lot_size

### Task 3.5: 改造 TraderHealth

**Files:**
- Modify: `crates/d_checktable/src/h_15m/trader.rs`（更新健康检查）

```rust
/// 交易器健康状态
#[derive(Debug, Clone)]
pub struct TraderHealth {
    pub symbol: String,
    pub is_running: bool,
    pub status: String,
    pub price: Option<String>,
    pub volatility: Option<f64>,
    pub pending_records: Option<i64>,  // 新增：待处理记录数
}

impl Trader {
    /// 健康检查
    pub async fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.symbol.clone(),
            is_running: *self.is_running.read().await,
            status: self.status_machine.read().await.current_status().as_str().to_string(),
            price: self.current_price().map(|p| p.to_string()),
            volatility: self.volatility_value(),
            pending_records: None, // TODO: 从 repository 查询
        }
    }
}
```

- [ ] **Step 12: 改造 TraderHealth**

Run: 修改 trader.rs 健康检查
Expected: TraderHealth 包含 pending_records

---

## Phase 4: mod.rs 导出更新

### Task 4.1: 更新 mod.rs 导出

**Files:**
- Modify: `crates/d_checktable/src/h_15m/mod.rs`

在 pub mod 区域添加：

```rust
pub mod signal;
pub mod status;
pub mod quantity_calculator;
pub mod trader;
pub mod executor;      // 新增
pub mod repository;   // 新增
```

在 use 区域添加：

```rust
pub use executor::{Executor, ExecutorConfig, ExecutorError, OrderType};
pub use repository::{Repository, TradeRecord, RecordStatus, RepoError, PENDING_TIMEOUT_SECS};
```

- [ ] **Step 13: 更新 mod.rs 导出**

Run: 修改 mod.rs
Expected: Executor / Repository / TradeRecord 等已导出

### Task 4.2: 编译验证

- [ ] **Step 14: 运行 cargo check**

Run: `cd crates/d_checktable && cargo check --lib`
Expected: 编译通过（无错误）

---

## Phase 5: strategy_loop.rs（Engine 心跳监控）

### Task 5.1: 新建 strategy_loop.rs

**Files:**
- Create: `crates/f_engine/src/core/strategy_loop.rs`

```rust
//! core/strategy_loop.rs
//!
//! Engine 协程管理 - spawn / stop / 心跳监控 / 指数退避重启

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

const MAX_RESTART_COUNT: u32 = 10;
const HEARTBEAT_TIMEOUT_MS: u64 = 30_000;

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// TraderHandle - Trader 协程句柄
pub struct TraderHandle {
    pub join_handle: RwLock<Option<JoinHandle<()>>>,
    pub last_heartbeat_ms: AtomicU64,
    pub restart_count: AtomicU32,
    pub symbol: String,
}

impl TraderHandle {
    pub fn new(symbol: String) -> Self {
        Self {
            join_handle: RwLock::new(None),
            last_heartbeat_ms: AtomicU64::new(current_time_ms()),
            restart_count: AtomicU32::new(0),
            symbol,
        }
    }

    pub fn heartbeat(&self) {
        self.last_heartbeat_ms.store(current_time_ms(), Ordering::Relaxed);
    }

    pub fn is_stale(&self) -> bool {
        current_time_ms() - self.last_heartbeat_ms.load(Ordering::Relaxed) > HEARTBEAT_TIMEOUT_MS
    }

    pub fn set_join_handle(&self, handle: JoinHandle<()>) {
        self.join_handle.try_write().map(|mut g| *g = Some(handle));
    }

    pub async fn is_finished(&self) -> bool {
        let guard = self.join_handle.read().await;
        guard.as_ref().map(|h| h.is_finished()).unwrap_or(true)
    }

    pub fn reset_restart_count(&self) {
        self.restart_count.store(0, Ordering::Relaxed);
    }

    pub fn load_restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::Relaxed)
    }
}

/// Engine - 协程管理器
pub struct Engine {
    instances: RwLock<std::collections::HashMap<String, Arc<TraderHandle>>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 启动 Trader 协程
    pub async fn spawn(&self, symbol: &str) {
        self.spawn_with_count(symbol, 0).await;
    }

    /// 带计数启动（用于重启时继承 restart_count）
    pub async fn spawn_with_count(&self, symbol: &str, restart_count: u32) {
        let handle = Arc::new(TraderHandle::new(symbol.to_string()));
        handle.restart_count.store(restart_count, Ordering::Relaxed);

        let mut instances = self.instances.write().await;
        instances.insert(symbol.to_string(), handle);

        tracing::info!(symbol = symbol, restart_count = restart_count, "Trader 协程已启动");
    }

    /// 停止 Trader
    pub async fn stop(&self, symbol: &str) {
        let handle = {
            let mut instances = self.instances.write().await;
            instances.remove(symbol)
        };

        if let Some(handle) = handle {
            let join_handle = {
                let mut guard = handle.join_handle.write().await;
                guard.take()
            };
            if let Some(h) = join_handle {
                let _ = h.await;
            }
            tracing::info!(symbol = symbol, "Trader 协程已停止");
        }
    }

    /// 心跳监控协程
    pub async fn monitor_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let mut to_restart = Vec::new();

            let instances = self.instances.read().await;
            for (symbol, handle) in instances.iter() {
                if handle.is_finished().await {
                    tracing::error!(symbol = %symbol, "协程已退出但未清理");
                    to_restart.push(symbol.clone());
                } else if handle.is_stale() {
                    tracing::warn!(symbol = %symbol, "心跳超时");
                    to_restart.push(symbol.clone());
                }
            }
            drop(instances);

            for symbol in to_restart {
                self.restart_with_backoff(&symbol).await;
            }
        }
    }

    /// 指数退避重启
    async fn restart_with_backoff(&self, symbol: &str) {
        let old_count = {
            let instances = self.instances.read().await;
            instances.get(symbol).map(|h| h.load_restart_count()).unwrap_or(0)
        };

        if old_count >= MAX_RESTART_COUNT {
            tracing::critical!(
                symbol = %symbol,
                restart_count = old_count,
                "达到最大重启次数（{}），停止自动重启",
                MAX_RESTART_COUNT
            );
            return;
        }

        self.stop(symbol).await;

        let delay_secs = 2u64.saturating_pow(old_count.min(5));
        tracing::info!(symbol = %symbol, delay_secs = delay_secs, "等待重启");
        tokio::time::sleep(Duration::from_secs(delay_secs)).await;

        self.spawn_with_count(symbol, old_count + 1).await;
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 15: 创建 strategy_loop.rs**

Run: 创建上述文件
Expected: `crates/f_engine/src/core/strategy_loop.rs` 已创建

### Task 5.2: 更新 f_engine core/mod.rs

**Files:**
- Modify: `crates/f_engine/src/core/mod.rs`

添加导出：

```rust
pub mod strategy_loop;
pub use strategy_loop::{Engine, TraderHandle, MAX_RESTART_COUNT, HEARTBEAT_TIMEOUT_MS};
```

- [ ] **Step 16: 更新 f_engine core/mod.rs**

Run: 修改 mod.rs
Expected: strategy_loop 已导出

---

## Phase 6: 集成测试

### Task 6.1: 集成测试 - 完整流程

**Files:**
- Create: `crates/d_checktable/src/h_15m/integration_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trader_lifecycle() {
        // 1. 创建 Repository
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let repo = Repository::new("BTCUSDT", db_path.to_str().unwrap()).unwrap();

        // 2. 创建 Executor
        let executor = Arc::new(Executor::new(ExecutorConfig {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 100,
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            max_position: dec!(0.15),
        }));

        // 3. 创建 Trader
        let config = TraderConfig {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 50,
            db_path: db_path.to_str().unwrap().to_string(),
            order_interval_ms: 100,
            lot_size: dec!(0.001),
            initial_ratio: dec!(0.05),
            max_position: dec!(0.15),
        };
        let trader = Arc::new(Trader::new(config, executor.clone(), repo.clone()));

        // 4. 启动（短暂运行）
        let trader_clone = trader.clone();
        let handle = tokio::spawn(async move {
            trader_clone.start().await;
        });

        tokio::time::sleep(Duration::from_millis(200)).await;

        // 5. 停止
        trader.stop();
        let _ = handle.await;

        // 6. 验证健康状态
        let health = trader.health().await;
        assert_eq!(health.symbol, "BTCUSDT");
        assert!(!health.is_running);
    }

    #[tokio::test]
    async fn test_rate_limit_atomic() {
        let executor = Arc::new(Executor::new(ExecutorConfig {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 50,
            ..Default::default()
        }));

        // 并发下单测试
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let ex = executor.clone();
                tokio::spawn(async move {
                    ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None)
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        let success_count = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();

        // 应该只有 1 个成功（原子 CAS 保证）
        assert_eq!(success_count, 1);
    }
}
```

- [ ] **Step 17: 创建 integration_test.rs**

Run: 创建上述文件
Expected: 集成测试已创建

- [ ] **Step 18: 运行集成测试**

Run: `cd crates/d_checktable && cargo test --lib h_15m::integration_test -- --nocapture`
Expected: 2 tests PASS

---

## 编译验证

- [ ] **Step 19: 全量编译检查**

Run: `cd crates/d_checktable && cargo check --all-targets`
Expected: 编译通过（无错误）

- [ ] **Step 20: 全量测试**

Run: `cd crates/d_checktable && cargo test --lib`
Expected: 所有测试 PASS

---

## 提交记录

- [ ] **Step 21: 提交 Phase 1**

```bash
cd crates/d_checktable/src/h_15m
git add repository.rs repository_test.rs
git commit -m "feat(h_15m): 新建 repository.rs - SQLite WAL 持久化层"
```

- [ ] **Step 22: 提交 Phase 2**

```bash
git add executor.rs executor_test.rs
git commit -m "feat(h_15m): 新建 executor.rs - 下单网关 + AtomicU64 频率限制"
```

- [ ] **Step 23: 提交 Phase 3-4**

```bash
git add trader.rs mod.rs
git commit -m "refactor(h_15m): 改造 trader.rs - tokio 异步锁 + WAL 整合"
```

- [ ] **Step 24: 提交 Phase 5**

```bash
cd crates/f_engine/src/core
git add strategy_loop.rs mod.rs
git commit -m "feat(f_engine): 新建 strategy_loop.rs - 心跳监控 + 指数退避重启"
```

- [ ] **Step 25: 提交集成测试**

```bash
git add integration_test.rs
git commit -m "test(h_15m): 添加集成测试 - 完整流程 + 并发频率限制"
```

---

## 风险提示

1. **SQLite 并发写入**: r2d2 连接池仅支持单写锁，多品种同时 `save_pending` 可能产生 `SQLITE_BUSY`。设计已考虑，但需在生产环境压测验证。

2. **Decimal 性能**: `rust_decimal` 的 `Div` 和 `Mul` 比 `f64` 慢，但下单频率低（秒级），影响可忽略。

3. **时间精度**: `SystemTime::now()` 在部分平台可能非单调。建议生产环境使用 `tokio::time::Instant` 用于间隔计算，仅转换时间戳用于存储。
