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
        conn.execute_batch(
            r#"
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
            "#,
        )?;
        Ok(())
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
            status: match row.get::<_, String>(4)?.as_str() {
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
            Err(RepoError::Database(rusqlite::Error::SqliteFailure(code, _))) => {
                // 检测 UNIQUE 约束冲突
                // code 在 rusqlite 0.32 中是 libsqlite3_sys::Error (字段 extended_code: i32)
                let extended = code.extended_code;
                if extended == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE {
                    return Err(RepoError::UniqueViolation);
                }
                Err(RepoError::Database(rusqlite::Error::SqliteFailure(code, None)))
            }
            Err(e) => Err(e),
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
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(params![symbol, timestamp])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_record(row)?))
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
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(params![symbol])?;
        if let Some(row) = rows.next()? {
            let record = Self::row_to_record(row)?;
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

impl Default for Repository {
    fn default() -> Self {
        Self::new("BTCUSDT", "./data/trade_records.db").expect("Failed to create default repository")
    }
}
