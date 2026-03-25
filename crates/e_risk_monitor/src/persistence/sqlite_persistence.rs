#![forbid(unsafe_code)]

//! SQLite 持久化模块
//!
//! 记录重要事件快照：
//! - 账户快照 (account_snapshots)
//! - 交易所持仓 (exchange_positions)
//! - 本地仓位记录 (local_positions)
//! - 通道切换事件 (channel_events)
//! - 风控事件 (risk_events)
//! - 指标事件 (indicator_events)

use rust_decimal::Decimal;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

use a_common::EngineError;

// ============================================================================
// 数据结构
// ============================================================================

/// 账户快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshotRecord {
    pub id: Option<i64>,
    pub ts: i64,
    pub account_id: String,
    pub total_equity: String,      // Decimal -> String 存储
    pub available: String,
    pub frozen_margin: String,
    pub unrealized_pnl: String,
    pub margin_ratio: String,
}

/// 交易所持仓记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangePositionRecord {
    pub id: Option<i64>,
    pub ts: i64,
    pub symbol: String,
    pub side: String,              // "long" or "short"
    pub qty: String,
    pub avg_price: String,
    pub unrealized_pnl: String,
    pub margin_used: String,
}

/// 本地仓位记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPositionRecord {
    pub id: Option<i64>,
    pub ts: i64,
    pub symbol: String,
    pub strategy_id: String,
    pub direction: String,          // "long" or "short"
    pub qty: String,
    pub avg_price: String,
    pub entry_ts: i64,
    pub remark: String,
}

/// 通道切换事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEventRecord {
    pub id: Option<i64>,
    pub ts: i64,
    pub event: String,            // "SLOW_TO_FAST" / "FAST_TO_SLOW" / "ENTER_FAST" / "EXIT_FAST"
    pub from_channel: String,
    pub to_channel: String,
    pub tr_ratio: String,
    pub ma5_in_20d_pos: String,
    pub pine_color: String,
    pub details: String,
}

/// 风控事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskEventRecord {
    pub id: Option<i64>,
    pub ts: i64,
    pub event_type: String,       // "REJECT" / "LIQUIDATION" / "MARGIN_CALL"
    pub symbol: String,
    pub order_id: String,
    pub reason: String,
    pub available_before: String,
    pub margin_ratio_before: String,
    pub action_taken: String,
    pub details: String,
}

/// 指标重要变化事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorEventRecord {
    pub id: Option<i64>,
    pub ts: i64,
    pub symbol: String,
    pub event: String,            // "TR_RATIO_BREAK" / "PINE_COLOR_CHANGE" / "ENTER_HIGH_VOL" / "EXIT_HIGH_VOL"
    pub tr_ratio_5d_20d: String,
    pub tr_ratio_20d_60d: String,
    pub pos_norm_20: String,
    pub ma5_in_20d_pos: String,
    pub ma20_in_60d_pos: String,
    pub pine_color_20_50: String,
    pub pine_color_100_200: String,
    pub pine_color_12_26: String,
    pub channel_type: String,
    pub details: String,
}

/// 指标对比 CSV 行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorComparisonRow {
    pub timestamp: i64,
    pub symbol: String,
    pub tr_ratio_5d_20d: String,
    pub tr_ratio_20d_60d: String,
    pub pos_norm_20: String,
    pub ma5_in_20d_pos: String,
    pub ma20_in_60d_pos: String,
    pub pine_color_20_50: String,
    pub pine_color_100_200: String,
    pub pine_color_12_26: String,
    pub vel_percentile: String,
    pub acc_percentile: String,
    pub power: String,
    pub channel_type: String,
}

/// 订单记录（用于灾备恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    pub order_id: String,
    pub symbol: String,
    pub side: String,
    pub qty: String,
    pub price: String,
    pub status: String,
    pub created_at: String,
    pub filled_at: Option<String>,
}

/// 同步日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncLogRecord {
    pub sync_type: String,
    pub source: String,
    pub target: String,
    pub timestamp: String,
    pub details: String,
}

// ============================================================================
// SQLite 记录服务
// ============================================================================

/// SQLite 记录服务
pub struct SqliteRecordService {
    conn: Arc<Mutex<Connection>>,
    /// 写入通道（用于异步写入）
    write_tx: Option<mpsc::Sender<WriteTask>>,
}

/// 写入任务枚举
#[derive(Debug)]
pub enum WriteTask {
    /// 订单写入任务
    Order(OrderRecord),
    /// 持仓写入任务
    Position(LocalPositionRecord),
}

impl SqliteRecordService {
    /// 获取数据库连接的锁
    pub fn lock(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.conn.lock()
    }
    /// 创建 SQLite 记录服务
    pub fn new(db_path: PathBuf) -> Result<Self, EngineError> {
        // 确保目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EngineError::Other(format!("创建目录失败: {}", e)))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| EngineError::Other(format!("打开数据库失败: {}", e)))?;

        let service = Self {
            conn: Arc::new(Mutex::new(conn)),
            write_tx: None,
        };

        service.init_tables()?;
        info!("SQLite 记录服务初始化完成: {:?}", db_path);

        Ok(service)
    }

    /// 初始化表结构
    fn init_tables(&self) -> Result<(), EngineError> {
        let conn = self.conn.lock();

        conn.execute_batch(r#"
            -- 账户快照表
            CREATE TABLE IF NOT EXISTS account_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                account_id TEXT NOT NULL,
                total_equity TEXT NOT NULL,
                available TEXT NOT NULL,
                frozen_margin TEXT NOT NULL,
                unrealized_pnl TEXT NOT NULL,
                margin_ratio TEXT NOT NULL
            );

            -- 交易所持仓表
            CREATE TABLE IF NOT EXISTS exchange_positions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                side TEXT NOT NULL,
                qty TEXT NOT NULL,
                avg_price TEXT NOT NULL,
                unrealized_pnl TEXT NOT NULL,
                margin_used TEXT NOT NULL
            );

            -- 本地仓位记录表
            CREATE TABLE IF NOT EXISTS local_positions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                strategy_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                qty TEXT NOT NULL,
                avg_price TEXT NOT NULL,
                entry_ts INTEGER NOT NULL,
                remark TEXT NOT NULL DEFAULT ''
            );

            -- 通道切换事件表
            CREATE TABLE IF NOT EXISTS channel_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                event TEXT NOT NULL,
                from_channel TEXT NOT NULL,
                to_channel TEXT NOT NULL,
                tr_ratio TEXT NOT NULL,
                ma5_in_20d_pos TEXT NOT NULL,
                pine_color TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT ''
            );

            -- 风控事件表
            CREATE TABLE IF NOT EXISTS risk_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                symbol TEXT NOT NULL,
                order_id TEXT NOT NULL,
                reason TEXT NOT NULL,
                available_before TEXT NOT NULL,
                margin_ratio_before TEXT NOT NULL,
                action_taken TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT ''
            );

            -- 指标事件表
            CREATE TABLE IF NOT EXISTS indicator_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                event TEXT NOT NULL,
                tr_ratio_5d_20d TEXT NOT NULL,
                tr_ratio_20d_60d TEXT NOT NULL,
                pos_norm_20 TEXT NOT NULL,
                ma5_in_20d_pos TEXT NOT NULL,
                ma20_in_60d_pos TEXT NOT NULL,
                pine_color_20_50 TEXT NOT NULL,
                pine_color_100_200 TEXT NOT NULL,
                pine_color_12_26 TEXT NOT NULL,
                channel_type TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT ''
            );

            -- 订单记录表（用于灾备恢复）
            CREATE TABLE IF NOT EXISTS orders (
                order_id TEXT PRIMARY KEY,
                symbol TEXT NOT NULL,
                side TEXT NOT NULL,
                qty TEXT NOT NULL,
                price TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                filled_at TEXT
            );

            -- 同步日志表
            CREATE TABLE IF NOT EXISTS sync_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sync_type TEXT NOT NULL,
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT ''
            );

            -- 创建索引
            CREATE INDEX IF NOT EXISTS idx_account_snapshots_ts ON account_snapshots(ts);
            CREATE INDEX IF NOT EXISTS idx_exchange_positions_ts ON exchange_positions(ts);
            CREATE INDEX IF NOT EXISTS idx_local_positions_ts ON local_positions(ts);
            CREATE INDEX IF NOT EXISTS idx_channel_events_ts ON channel_events(ts);
            CREATE INDEX IF NOT EXISTS idx_risk_events_ts ON risk_events(ts);
            CREATE INDEX IF NOT EXISTS idx_indicator_events_ts ON indicator_events(ts);
            CREATE INDEX IF NOT EXISTS idx_orders_symbol ON orders(symbol);
            CREATE INDEX IF NOT EXISTS idx_orders_created_at ON orders(created_at);
            CREATE INDEX IF NOT EXISTS idx_sync_log_timestamp ON sync_log(timestamp);
        "#).map_err(|e| EngineError::Other(format!("初始化表失败: {}", e)))?;

        Ok(())
    }

    // ========== 账户快照 ==========

    /// 记录账户快照
    pub fn save_account_snapshot(&self, record: AccountSnapshotRecord) -> Result<i64, EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO account_snapshots (ts, account_id, total_equity, available, frozen_margin, unrealized_pnl, margin_ratio) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.ts,
                record.account_id,
                record.total_equity,
                record.available,
                record.frozen_margin,
                record.unrealized_pnl,
                record.margin_ratio
            ],
        ).map_err(|e| EngineError::Other(format!("插入账户快照失败: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    // ========== 交易所持仓 ==========

    /// 记录交易所持仓变化
    pub fn save_exchange_position(&self, record: ExchangePositionRecord) -> Result<i64, EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO exchange_positions (ts, symbol, side, qty, avg_price, unrealized_pnl, margin_used) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.ts,
                record.symbol,
                record.side,
                record.qty,
                record.avg_price,
                record.unrealized_pnl,
                record.margin_used
            ],
        ).map_err(|e| EngineError::Other(format!("插入交易所持仓失败: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    // ========== 本地仓位记录 ==========

    /// 记录本地仓位变化
    pub fn save_local_position(&self, record: LocalPositionRecord) -> Result<i64, EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO local_positions (ts, symbol, strategy_id, direction, qty, avg_price, entry_ts, remark) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.ts,
                record.symbol,
                record.strategy_id,
                record.direction,
                record.qty,
                record.avg_price,
                record.entry_ts,
                record.remark
            ],
        ).map_err(|e| EngineError::Other(format!("插入本地仓位失败: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    // ========== 通道事件 ==========

    /// 记录通道切换事件
    pub fn save_channel_event(&self, record: ChannelEventRecord) -> Result<i64, EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO channel_events (ts, event, from_channel, to_channel, tr_ratio, ma5_in_20d_pos, pine_color, details) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.ts,
                record.event,
                record.from_channel,
                record.to_channel,
                record.tr_ratio,
                record.ma5_in_20d_pos,
                record.pine_color,
                record.details
            ],
        ).map_err(|e| EngineError::Other(format!("插入通道事件失败: {}", e)))?;

        info!("通道事件记录: {} {} -> {}", record.event, record.from_channel, record.to_channel);
        Ok(conn.last_insert_rowid())
    }

    // ========== 风控事件 ==========

    /// 记录风控事件
    pub fn save_risk_event(&self, record: RiskEventRecord) -> Result<i64, EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO risk_events (ts, event_type, symbol, order_id, reason, available_before, margin_ratio_before, action_taken, details) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.ts,
                record.event_type,
                record.symbol,
                record.order_id,
                record.reason,
                record.available_before,
                record.margin_ratio_before,
                record.action_taken,
                record.details
            ],
        ).map_err(|e| EngineError::Other(format!("插入风控事件失败: {}", e)))?;

        warn!("风控事件记录: {} - {}", record.event_type, record.reason);
        Ok(conn.last_insert_rowid())
    }

    // ========== 指标事件 ==========

    /// 记录指标事件
    pub fn save_indicator_event(&self, record: IndicatorEventRecord) -> Result<i64, EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO indicator_events (ts, symbol, event, tr_ratio_5d_20d, tr_ratio_20d_60d, pos_norm_20, ma5_in_20d_pos, ma20_in_60d_pos, pine_color_20_50, pine_color_100_200, pine_color_12_26, channel_type, details) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.ts,
                record.symbol,
                record.event,
                record.tr_ratio_5d_20d,
                record.tr_ratio_20d_60d,
                record.pos_norm_20,
                record.ma5_in_20d_pos,
                record.ma20_in_60d_pos,
                record.pine_color_20_50,
                record.pine_color_100_200,
                record.pine_color_12_26,
                record.channel_type,
                record.details
            ],
        ).map_err(|e| EngineError::Other(format!("插入指标事件失败: {}", e)))?;

        info!("指标事件记录: {} - {}", record.symbol, record.event);
        Ok(conn.last_insert_rowid())
    }

    // ========== 查询方法 ==========

    /// 获取最近的账户快照
    pub fn get_latest_account_snapshot(&self, account_id: &str) -> Result<Option<AccountSnapshotRecord>, EngineError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, ts, account_id, total_equity, available, frozen_margin, unrealized_pnl, margin_ratio FROM account_snapshots WHERE account_id = ?1 ORDER BY ts DESC LIMIT 1"
        ).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let result = stmt.query_row(params![account_id], |row| {
            Ok(AccountSnapshotRecord {
                id: Some(row.get(0)?),
                ts: row.get(1)?,
                account_id: row.get(2)?,
                total_equity: row.get(3)?,
                available: row.get(4)?,
                frozen_margin: row.get(5)?,
                unrealized_pnl: row.get(6)?,
                margin_ratio: row.get(7)?,
            })
        });

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(EngineError::Other(format!("查询失败: {}", e))),
        }
    }

    /// 获取最近的通道事件
    pub fn get_latest_channel_event(&self) -> Result<Option<ChannelEventRecord>, EngineError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, ts, event, from_channel, to_channel, tr_ratio, ma5_in_20d_pos, pine_color, details FROM channel_events ORDER BY ts DESC LIMIT 1"
        ).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let result = stmt.query_row([], |row| {
            Ok(ChannelEventRecord {
                id: Some(row.get(0)?),
                ts: row.get(1)?,
                event: row.get(2)?,
                from_channel: row.get(3)?,
                to_channel: row.get(4)?,
                tr_ratio: row.get(5)?,
                ma5_in_20d_pos: row.get(6)?,
                pine_color: row.get(7)?,
                details: row.get(8)?,
            })
        });

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(EngineError::Other(format!("查询失败: {}", e))),
        }
    }

    /// 获取所有风控事件
    pub fn get_all_risk_events(&self) -> Result<Vec<RiskEventRecord>, EngineError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, ts, event_type, symbol, order_id, reason, available_before, margin_ratio_before, action_taken, details FROM risk_events ORDER BY ts DESC"
        ).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let records = stmt.query_map([], |row| {
            Ok(RiskEventRecord {
                id: Some(row.get(0)?),
                ts: row.get(1)?,
                event_type: row.get(2)?,
                symbol: row.get(3)?,
                order_id: row.get(4)?,
                reason: row.get(5)?,
                available_before: row.get(6)?,
                margin_ratio_before: row.get(7)?,
                action_taken: row.get(8)?,
                details: row.get(9)?,
            })
        }).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        records.collect::<Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))
    }

    // ========== 订单记录 ==========

    /// 保存订单记录
    pub fn save_order(&self, order: OrderRecord) -> Result<(), EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO orders (order_id, symbol, side, qty, price, status, created_at, filled_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                order.order_id,
                order.symbol,
                order.side,
                order.qty,
                order.price,
                order.status,
                order.created_at,
                order.filled_at,
            ],
        ).map_err(|e| EngineError::Other(format!("保存订单失败: {}", e)))?;
        Ok(())
    }

    /// 异步保存订单（不阻塞主线程）
    pub async fn save_order_async(&self, order: OrderRecord) -> Result<(), EngineError> {
        let tx = self.write_tx.as_ref()
            .ok_or_else(|| EngineError::Other("写入通道未初始化，请先调用 start_write_worker".to_string()))?;

        tx.send(WriteTask::Order(order)).await
            .map_err(|e| EngineError::Other(format!("发送写入任务失败: {}", e)))?;
        Ok(())
    }

    /// 异步保存持仓（不阻塞主线程）
    pub async fn save_position_async(&self, position: LocalPositionRecord) -> Result<(), EngineError> {
        let tx = self.write_tx.as_ref()
            .ok_or_else(|| EngineError::Other("写入通道未初始化，请先调用 start_write_worker".to_string()))?;

        tx.send(WriteTask::Position(position)).await
            .map_err(|e| EngineError::Other(format!("发送写入任务失败: {}", e)))?;
        Ok(())
    }

    /// 启动写入工作线程
    pub fn start_write_worker(&mut self) {
        let conn = self.conn.clone();
        let (tx, mut rx) = mpsc::channel::<WriteTask>(100);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("创建 tokio runtime 失败");

            rt.block_on(async {
                while let Some(task) = rx.recv().await {
                    match task {
                        WriteTask::Order(order) => {
                            let conn_guard = conn.lock();
                            if let Err(e) = conn_guard.execute(
                                "INSERT OR REPLACE INTO orders (order_id, symbol, side, qty, price, status, created_at, filled_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                                params![
                                    order.order_id, order.symbol, order.side, order.qty,
                                    order.price, order.status, order.created_at, order.filled_at,
                                ],
                            ) {
                                error!("异步写入订单失败: {}", e);
                            }
                        }
                        WriteTask::Position(pos) => {
                            let conn_guard = conn.lock();
                            if let Err(e) = conn_guard.execute(
                                "INSERT OR REPLACE INTO local_positions (id, ts, symbol, strategy_id, direction, qty, avg_price, entry_ts, remark) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                                params![
                                    pos.id, pos.ts, pos.symbol, pos.strategy_id, pos.direction,
                                    pos.qty, pos.avg_price, pos.entry_ts, pos.remark,
                                ],
                            ) {
                                error!("异步写入持仓失败: {}", e);
                            }
                        }
                    }
                }
            });
        });

        self.write_tx = Some(tx);
    }

    /// 获取所有订单
    pub fn get_all_orders(&self) -> Result<Vec<OrderRecord>, EngineError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT order_id, symbol, side, qty, price, status, created_at, filled_at FROM orders ORDER BY created_at DESC")
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let records = stmt.query_map([], |row| {
            Ok(OrderRecord {
                order_id: row.get(0)?,
                symbol: row.get(1)?,
                side: row.get(2)?,
                qty: row.get(3)?,
                price: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                filled_at: row.get(7)?,
            })
        }).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        records.collect::<Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))
    }

    /// 获取指定交易对的所有订单
    pub fn get_orders_by_symbol(&self, symbol: &str) -> Result<Vec<OrderRecord>, EngineError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT order_id, symbol, side, qty, price, status, created_at, filled_at FROM orders WHERE symbol = ?1 ORDER BY created_at DESC")
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let records = stmt.query_map(params![symbol], |row| {
            Ok(OrderRecord {
                order_id: row.get(0)?,
                symbol: row.get(1)?,
                side: row.get(2)?,
                qty: row.get(3)?,
                price: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                filled_at: row.get(7)?,
            })
        }).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        records.collect::<Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))
    }

    // ========== 同步日志 ==========

    /// 保存同步日志
    pub fn save_sync_log(&self, log: SyncLogRecord) -> Result<(), EngineError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO sync_log (sync_type, source, target, timestamp, details) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                log.sync_type,
                log.source,
                log.target,
                log.timestamp,
                log.details,
            ],
        ).map_err(|e| EngineError::Other(format!("保存同步日志失败: {}", e)))?;
        Ok(())
    }

    /// 获取最近的同步日志
    pub fn get_recent_sync_logs(&self, limit: usize) -> Result<Vec<SyncLogRecord>, EngineError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT sync_type, source, target, timestamp, details FROM sync_log ORDER BY id DESC LIMIT ?1")
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let records = stmt.query_map([limit], |row| {
            Ok(SyncLogRecord {
                sync_type: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
                timestamp: row.get(3)?,
                details: row.get(4)?,
            })
        }).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        records.collect::<Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))
    }

    // ========== 仓位恢复（用于灾备） ==========

    /// 获取最新的本地仓位（按 symbol 分组）
    pub fn get_latest_positions(&self) -> Result<Vec<(String, String, String, String, String, String)>, EngineError> {
        let conn = self.conn.lock();
        // 按 symbol 分组，每组取最新的一条记录
        let mut stmt = conn
            .prepare(
                "SELECT ts, symbol, direction, qty, avg_price, remark
                 FROM local_positions lp1
                 WHERE ts = (
                     SELECT MAX(ts) FROM local_positions lp2 WHERE lp2.symbol = lp1.symbol
                 )
                 ORDER BY ts DESC"
            )
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        let records = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?.to_string(),
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        }).map_err(|e| EngineError::Other(format!("查询失败: {}", e)))?;

        records.collect::<Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Other(format!("查询失败: {}", e)))
    }
}

// ============================================================================
// CSV 输出服务
// ============================================================================

/// 指标对比 CSV 写入器
pub struct IndicatorCsvWriter {
    file_path: PathBuf,
}

impl IndicatorCsvWriter {
    /// 创建新的 CSV 写入器
    pub fn new(file_path: PathBuf) -> Result<Self, EngineError> {
        // 确保目录存在
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EngineError::Other(format!("创建目录失败: {}", e)))?;
        }

        // 写入 CSV 头部
        let header = "timestamp,symbol,tr_ratio_5d_20d,tr_ratio_20d_60d,pos_norm_20,ma5_in_20d_pos,ma20_in_60d_pos,pine_color_20_50,pine_color_100_200,pine_color_12_26,vel_percentile,acc_percentile,power,channel_type\n";
        std::fs::write(&file_path, header)
            .map_err(|e| EngineError::Other(format!("创建CSV文件失败: {}", e)))?;

        info!("指标对比 CSV 写入器初始化: {:?}", file_path);
        Ok(Self { file_path })
    }

    /// 写入一行指标数据
    pub fn write_row(&self, row: &IndicatorComparisonRow) -> Result<(), EngineError> {
        let line = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            row.timestamp,
            row.symbol,
            row.tr_ratio_5d_20d,
            row.tr_ratio_20d_60d,
            row.pos_norm_20,
            row.ma5_in_20d_pos,
            row.ma20_in_60d_pos,
            row.pine_color_20_50,
            row.pine_color_100_200,
            row.pine_color_12_26,
            row.vel_percentile,
            row.acc_percentile,
            row.power,
            row.channel_type
        );

        std::fs::OpenOptions::new()
            .append(true)
            .open(&self.file_path)
            .map_err(|e| EngineError::Other(format!("打开CSV失败: {}", e)))?
            .write_all(line.as_bytes())
            .map_err(|e| EngineError::Other(format!("写入CSV失败: {}", e)))?;

        Ok(())
    }

    /// 获取文件路径
    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

// ============================================================================
// 便捷构造函数
// ============================================================================

impl SqliteRecordService {
    /// 使用默认路径创建服务（自动检测平台）
    pub fn with_default_path() -> Result<Self, EngineError> {
        let db_path = a_common::Paths::new().sqlite_db();
        Self::new(db_path)
    }
}

impl IndicatorCsvWriter {
    /// 使用默认路径创建写入器（自动检测平台）
    pub fn with_default_path() -> Result<Self, EngineError> {
        let file_path = a_common::Paths::new().csv_output();
        Self::new(file_path)
    }
}

// ============================================================================
// 辅助函数：将 Decimal 转换为 String
// ============================================================================

/// 将 Decimal 格式化为字符串，保留合理精度
pub fn format_decimal(d: &Decimal) -> String {
    // 保留8位小数，避免精度丢失
    let s = format!("{}", d);
    // 如果字符串过长，截断
    if s.len() > 20 {
        format!("{:.8}", d)
    } else {
        s
    }
}

// ============================================================================
// 事件记录器 trait - 用于集成到 MockBinanceGateway
// ============================================================================

/// 事件记录器 trait
/// 实现此 trait 来定制事件记录行为
pub trait EventRecorder: Send + Sync {
    /// 记录账户快照
    fn record_account_snapshot(&self, record: AccountSnapshotRecord);

    /// 记录交易所持仓变化
    fn record_exchange_position(&self, record: ExchangePositionRecord);

    /// 记录本地仓位变化
    fn record_local_position(&self, record: LocalPositionRecord);

    /// 记录通道切换事件
    fn record_channel_event(&self, record: ChannelEventRecord);

    /// 记录风控事件
    fn record_risk_event(&self, record: RiskEventRecord);

    /// 记录指标事件
    fn record_indicator_event(&self, record: IndicatorEventRecord);
}

/// 空记录器 - 不记录任何事件
pub struct NoOpEventRecorder;

impl EventRecorder for NoOpEventRecorder {
    fn record_account_snapshot(&self, _record: AccountSnapshotRecord) {}
    fn record_exchange_position(&self, _record: ExchangePositionRecord) {}
    fn record_local_position(&self, _record: LocalPositionRecord) {}
    fn record_channel_event(&self, _record: ChannelEventRecord) {}
    fn record_risk_event(&self, _record: RiskEventRecord) {}
    fn record_indicator_event(&self, _record: IndicatorEventRecord) {}
}

/// SQLite 事件记录器实现
pub struct SqliteEventRecorder {
    service: Arc<SqliteRecordService>,
}

impl SqliteEventRecorder {
    /// 创建新的 SQLite 事件记录器
    pub fn new(service: SqliteRecordService) -> Self {
        Self {
            service: Arc::new(service),
        }
    }
}

impl EventRecorder for SqliteEventRecorder {
    fn record_account_snapshot(&self, record: AccountSnapshotRecord) {
        if let Err(e) = self.service.save_account_snapshot(record) {
            warn!("记录账户快照失败: {}", e);
        }
    }

    fn record_exchange_position(&self, record: ExchangePositionRecord) {
        if let Err(e) = self.service.save_exchange_position(record) {
            warn!("记录交易所持仓失败: {}", e);
        }
    }

    fn record_local_position(&self, record: LocalPositionRecord) {
        if let Err(e) = self.service.save_local_position(record) {
            warn!("记录本地仓位失败: {}", e);
        }
    }

    fn record_channel_event(&self, record: ChannelEventRecord) {
        if let Err(e) = self.service.save_channel_event(record) {
            warn!("记录通道事件失败: {}", e);
        }
    }

    fn record_risk_event(&self, record: RiskEventRecord) {
        if let Err(e) = self.service.save_risk_event(record) {
            warn!("记录风控事件失败: {}", e);
        }
    }

    fn record_indicator_event(&self, record: IndicatorEventRecord) {
        if let Err(e) = self.service.save_indicator_event(record) {
            warn!("记录指标事件失败: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // E5.1 SqliteRecordService 单元测试
    // =========================================================================

    #[test]
    fn test_service_creation_and_table_init() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_events.db");

        let service = SqliteRecordService::new(db_path.clone()).unwrap();

        // 验证数据库文件已创建
        assert!(db_path.exists(), "数据库文件应该被创建");

        // 验证 6 张表已创建
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        let tables = vec![
            "account_snapshots",
            "exchange_positions",
            "local_positions",
            "channel_events",
            "risk_events",
            "indicator_events",
        ];

        for table_name in tables {
            let count: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table_name],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "表 {} 应该被创建", table_name);
        }

        // 验证索引也已创建
        let index_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 9, "应该有 9 个索引");
    }

    #[test]
    fn test_save_and_get_account_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_account.db");

        let service = SqliteRecordService::new(db_path.clone()).unwrap();

        // 保存账户快照
        let record = AccountSnapshotRecord {
            id: None,
            ts: 1710931200,
            account_id: "test_account".to_string(),
            total_equity: "100000.0".to_string(),
            available: "95000.0".to_string(),
            frozen_margin: "5000.0".to_string(),
            unrealized_pnl: "0.0".to_string(),
            margin_ratio: "0.05".to_string(),
        };

        let id = service.save_account_snapshot(record.clone()).unwrap();
        assert!(id > 0, "插入应该返回有效的 ID");

        // 查询账户快照
        let retrieved = service.get_latest_account_snapshot("test_account").unwrap();
        assert!(retrieved.is_some(), "应该能查询到账户快照");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.account_id, "test_account");
        assert_eq!(retrieved.total_equity, "100000.0");
        assert_eq!(retrieved.available, "95000.0");
        assert_eq!(retrieved.frozen_margin, "5000.0");
    }

    #[test]
    fn test_save_and_get_channel_event() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_channel.db");

        let service = SqliteRecordService::new(db_path.clone()).unwrap();

        // 保存通道事件
        let record = ChannelEventRecord {
            id: None,
            ts: 1710931200,
            event: "SLOW_TO_FAST".to_string(),
            from_channel: "Slow".to_string(),
            to_channel: "Fast".to_string(),
            tr_ratio: "1.5".to_string(),
            ma5_in_20d_pos: "0.7".to_string(),
            pine_color: "Green".to_string(),
            details: "High volatility detected".to_string(),
        };

        let id = service.save_channel_event(record.clone()).unwrap();
        assert!(id > 0, "插入应该返回有效的 ID");

        // 查询通道事件
        let retrieved = service.get_latest_channel_event().unwrap();
        assert!(retrieved.is_some(), "应该能查询到通道事件");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.event, "SLOW_TO_FAST");
        assert_eq!(retrieved.from_channel, "Slow");
        assert_eq!(retrieved.to_channel, "Fast");
        assert_eq!(retrieved.tr_ratio, "1.5");
    }

    #[test]
    fn test_save_and_get_risk_event() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_risk.db");

        let service = SqliteRecordService::new(db_path.clone()).unwrap();

        // 保存风控事件
        let record = RiskEventRecord {
            id: None,
            ts: 1710931200,
            event_type: "REJECT".to_string(),
            symbol: "BTCUSDT".to_string(),
            order_id: "ORDER_001".to_string(),
            reason: "Insufficient margin".to_string(),
            available_before: "1000.0".to_string(),
            margin_ratio_before: "0.15".to_string(),
            action_taken: "Order rejected".to_string(),
            details: "Margin ratio below minimum".to_string(),
        };

        let id = service.save_risk_event(record.clone()).unwrap();
        assert!(id > 0, "插入应该返回有效的 ID");

        // 查询所有风控事件
        let events = service.get_all_risk_events().unwrap();
        assert_eq!(events.len(), 1, "应该只有 1 条风控事件");

        let retrieved = &events[0];
        assert_eq!(retrieved.event_type, "REJECT");
        assert_eq!(retrieved.symbol, "BTCUSDT");
        assert_eq!(retrieved.order_id, "ORDER_001");
        assert_eq!(retrieved.reason, "Insufficient margin");
    }

    #[test]
    fn test_multiple_records_ordering() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_ordering.db");

        let service = SqliteRecordService::new(db_path.clone()).unwrap();

        // 按不同时间戳保存多个账户快照
        let timestamps = vec![1710931200, 1710931300, 1710931400, 1710931500];

        for (i, ts) in timestamps.iter().enumerate() {
            let record = AccountSnapshotRecord {
                id: None,
                ts: *ts,
                account_id: "test_account".to_string(),
                total_equity: format!("{}", 100000.0 + i as f64 * 100.0),
                available: "95000.0".to_string(),
                frozen_margin: "5000.0".to_string(),
                unrealized_pnl: "0.0".to_string(),
                margin_ratio: "0.05".to_string(),
            };
            service.save_account_snapshot(record).unwrap();
        }

        // 查询最新的账户快照（应该按时间倒序）
        let retrieved = service.get_latest_account_snapshot("test_account").unwrap();
        assert!(retrieved.is_some(), "应该能查询到账户快照");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.ts, 1710931500, "最新记录的时间戳应该是 1710931500");
        // rust_decimal 格式化为字符串时不带小数点后缀
        assert_eq!(retrieved.total_equity, "100300", "最新记录的 equity 应该是 100300");
    }

    // =========================================================================
    // E5.2 IndicatorCsvWriter 单元测试
    // =========================================================================

    #[test]
    fn test_csv_writer_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("indicators.csv");

        let writer = IndicatorCsvWriter::new(csv_path.clone()).unwrap();

        // 验证文件已创建
        assert!(csv_path.exists(), "CSV 文件应该被创建");

        // 验证文件路径正确
        assert_eq!(writer.file_path(), &csv_path);

        // 验证 CSV 头部已写入
        let content = std::fs::read_to_string(&csv_path).unwrap();
        let expected_header = "timestamp,symbol,tr_ratio_5d_20d,tr_ratio_20d_60d,pos_norm_20,ma5_in_20d_pos,ma20_in_60d_pos,pine_color_20_50,pine_color_100_200,pine_color_12_26,vel_percentile,acc_percentile,power,channel_type\n";
        assert_eq!(content, expected_header, "CSV 头部应该正确");
    }

    #[test]
    fn test_csv_writer_write_row() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("indicators.csv");

        let writer = IndicatorCsvWriter::new(csv_path.clone()).unwrap();

        // 写入一行数据
        let row = IndicatorComparisonRow {
            timestamp: 1710931200,
            symbol: "BTCUSDT".to_string(),
            tr_ratio_5d_20d: "1.5".to_string(),
            tr_ratio_20d_60d: "1.2".to_string(),
            pos_norm_20: "0.6".to_string(),
            ma5_in_20d_pos: "0.7".to_string(),
            ma20_in_60d_pos: "0.5".to_string(),
            pine_color_20_50: "Green".to_string(),
            pine_color_100_200: "Green".to_string(),
            pine_color_12_26: "Red".to_string(),
            vel_percentile: "0.8".to_string(),
            acc_percentile: "0.75".to_string(),
            power: "0.65".to_string(),
            channel_type: "Fast".to_string(),
        };

        writer.write_row(&row).unwrap();

        // 验证 CSV 内容
        let content = std::fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "应该有两行：头部 + 1行数据");

        let data_line = lines[1];
        assert!(data_line.starts_with("1710931200,BTCUSDT,"), "数据行应该以 timestamp 和 symbol 开头");
        assert!(data_line.ends_with(",Fast"), "数据行应该以 channel_type 结尾");
    }

    #[test]
    fn test_csv_writer_multiple_rows() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("indicators.csv");

        let writer = IndicatorCsvWriter::new(csv_path.clone()).unwrap();

        // 写入多行数据
        for i in 0..5 {
            let row = IndicatorComparisonRow {
                timestamp: 1710931200 + i * 60,
                symbol: "BTCUSDT".to_string(),
                tr_ratio_5d_20d: format!("1.{}", i),
                tr_ratio_20d_60d: "1.2".to_string(),
                pos_norm_20: "0.6".to_string(),
                ma5_in_20d_pos: "0.7".to_string(),
                ma20_in_60d_pos: "0.5".to_string(),
                pine_color_20_50: "Green".to_string(),
                pine_color_100_200: "Green".to_string(),
                pine_color_12_26: "Red".to_string(),
                vel_percentile: "0.8".to_string(),
                acc_percentile: "0.75".to_string(),
                power: "0.65".to_string(),
                channel_type: "Fast".to_string(),
            };
            writer.write_row(&row).unwrap();
        }

        // 验证 CSV 内容
        let content = std::fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 6, "应该有 6 行：头部 + 5行数据");
    }

    #[test]
    fn test_csv_writer_content() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("indicators.csv");

        let writer = IndicatorCsvWriter::new(csv_path.clone()).unwrap();

        // 写入一行完整数据
        let row = IndicatorComparisonRow {
            timestamp: 1710931200,
            symbol: "ETHUSDT".to_string(),
            tr_ratio_5d_20d: "2.1".to_string(),
            tr_ratio_20d_60d: "1.8".to_string(),
            pos_norm_20: "0.45".to_string(),
            ma5_in_20d_pos: "0.55".to_string(),
            ma20_in_60d_pos: "0.62".to_string(),
            pine_color_20_50: "Red".to_string(),
            pine_color_100_200: "Red".to_string(),
            pine_color_12_26: "Green".to_string(),
            vel_percentile: "0.92".to_string(),
            acc_percentile: "0.88".to_string(),
            power: "0.78".to_string(),
            channel_type: "Slow".to_string(),
        };

        writer.write_row(&row).unwrap();

        // 验证 CSV 内容正确
        let content = std::fs::read_to_string(&csv_path).unwrap();

        // 检查头部
        assert!(content.contains("timestamp,symbol,tr_ratio_5d_20d,tr_ratio_20d_60d"));

        // 检查数据行
        assert!(content.contains("1710931200,ETHUSDT,2.1,1.8,0.45,0.55,0.62,Red,Red,Green,0.92,0.88,0.78,Slow"));
    }

    // =========================================================================
    // E5.3 EventRecorder trait 集成测试
    // =========================================================================

    #[test]
    fn test_noop_event_recorder() {
        let recorder = NoOpEventRecorder;

        // NoOpEventRecorder 不记录任何数据，所以这些调用不应该panic
        recorder.record_account_snapshot(AccountSnapshotRecord {
            id: None,
            ts: 1710931200,
            account_id: "test".to_string(),
            total_equity: "1000".to_string(),
            available: "900".to_string(),
            frozen_margin: "100".to_string(),
            unrealized_pnl: "0".to_string(),
            margin_ratio: "0.1".to_string(),
        });

        recorder.record_exchange_position(ExchangePositionRecord {
            id: None,
            ts: 1710931200,
            symbol: "BTCUSDT".to_string(),
            side: "long".to_string(),
            qty: "1.0".to_string(),
            avg_price: "50000".to_string(),
            unrealized_pnl: "0".to_string(),
            margin_used: "500".to_string(),
        });

        recorder.record_local_position(LocalPositionRecord {
            id: None,
            ts: 1710931200,
            symbol: "BTCUSDT".to_string(),
            strategy_id: "strategy1".to_string(),
            direction: "long".to_string(),
            qty: "1.0".to_string(),
            avg_price: "50000".to_string(),
            entry_ts: 1710931200,
            remark: "".to_string(),
        });

        recorder.record_channel_event(ChannelEventRecord {
            id: None,
            ts: 1710931200,
            event: "SLOW_TO_FAST".to_string(),
            from_channel: "Slow".to_string(),
            to_channel: "Fast".to_string(),
            tr_ratio: "1.5".to_string(),
            ma5_in_20d_pos: "0.7".to_string(),
            pine_color: "Green".to_string(),
            details: "".to_string(),
        });

        recorder.record_risk_event(RiskEventRecord {
            id: None,
            ts: 1710931200,
            event_type: "REJECT".to_string(),
            symbol: "BTCUSDT".to_string(),
            order_id: "ORDER_001".to_string(),
            reason: "Insufficient margin".to_string(),
            available_before: "1000".to_string(),
            margin_ratio_before: "0.15".to_string(),
            action_taken: "Rejected".to_string(),
            details: "".to_string(),
        });

        recorder.record_indicator_event(IndicatorEventRecord {
            id: None,
            ts: 1710931200,
            symbol: "BTCUSDT".to_string(),
            event: "TR_RATIO_BREAK".to_string(),
            tr_ratio_5d_20d: "1.5".to_string(),
            tr_ratio_20d_60d: "1.2".to_string(),
            pos_norm_20: "0.6".to_string(),
            ma5_in_20d_pos: "0.7".to_string(),
            ma20_in_60d_pos: "0.5".to_string(),
            pine_color_20_50: "Green".to_string(),
            pine_color_100_200: "Green".to_string(),
            pine_color_12_26: "Red".to_string(),
            channel_type: "Fast".to_string(),
            details: "".to_string(),
        });

        // 如果没有panic，说明测试通过
    }

    #[test]
    fn test_sqlite_event_recorder() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_recorder.db");

        let service = SqliteRecordService::new(db_path.clone()).unwrap();
        let recorder = SqliteEventRecorder::new(service);

        // 通过 recorder 记录账户快照
        recorder.record_account_snapshot(AccountSnapshotRecord {
            id: None,
            ts: 1710931200,
            account_id: "test_account".to_string(),
            total_equity: "100000.0".to_string(),
            available: "95000.0".to_string(),
            frozen_margin: "5000.0".to_string(),
            unrealized_pnl: "0.0".to_string(),
            margin_ratio: "0.05".to_string(),
        });

        // 通过 recorder 记录通道事件
        recorder.record_channel_event(ChannelEventRecord {
            id: None,
            ts: 1710931200,
            event: "SLOW_TO_FAST".to_string(),
            from_channel: "Slow".to_string(),
            to_channel: "Fast".to_string(),
            tr_ratio: "1.5".to_string(),
            ma5_in_20d_pos: "0.7".to_string(),
            pine_color: "Green".to_string(),
            details: "".to_string(),
        });

        // 通过 recorder 记录风控事件
        recorder.record_risk_event(RiskEventRecord {
            id: None,
            ts: 1710931200,
            event_type: "REJECT".to_string(),
            symbol: "BTCUSDT".to_string(),
            order_id: "ORDER_001".to_string(),
            reason: "Insufficient margin".to_string(),
            available_before: "1000.0".to_string(),
            margin_ratio_before: "0.15".to_string(),
            action_taken: "Order rejected".to_string(),
            details: "".to_string(),
        });

        // 重新创建 service 来验证数据
        let service2 = SqliteRecordService::new(db_path.clone()).unwrap();

        // 验证账户快照
        let account = service2.get_latest_account_snapshot("test_account").unwrap();
        assert!(account.is_some(), "应该能查询到账户快照");
        assert_eq!(account.unwrap().total_equity, "100000.0");

        // 验证通道事件
        let channel = service2.get_latest_channel_event().unwrap();
        assert!(channel.is_some(), "应该能查询到通道事件");
        assert_eq!(channel.unwrap().event, "SLOW_TO_FAST");

        // 验证风控事件
        let risks = service2.get_all_risk_events().unwrap();
        assert_eq!(risks.len(), 1, "应该只有 1 条风控事件");
        assert_eq!(risks[0].event_type, "REJECT");
    }

    // =========================================================================
    // 原有基础测试（保留）
    // =========================================================================

    #[test]
    fn test_account_snapshot_record() {
        let record = AccountSnapshotRecord {
            id: None,
            ts: 1710931200,
            account_id: "test_account".to_string(),
            total_equity: "100000.0".to_string(),
            available: "95000.0".to_string(),
            frozen_margin: "5000.0".to_string(),
            unrealized_pnl: "0.0".to_string(),
            margin_ratio: "0.05".to_string(),
        };

        assert_eq!(record.account_id, "test_account");
    }

    #[test]
    fn test_indicator_event_record() {
        let record = IndicatorEventRecord {
            id: None,
            ts: 1710931200,
            symbol: "BTCUSDT".to_string(),
            event: "TR_RATIO_BREAK".to_string(),
            tr_ratio_5d_20d: "1.5".to_string(),
            tr_ratio_20d_60d: "1.2".to_string(),
            pos_norm_20: "0.6".to_string(),
            ma5_in_20d_pos: "0.7".to_string(),
            ma20_in_60d_pos: "0.5".to_string(),
            pine_color_20_50: "Green".to_string(),
            pine_color_100_200: "Green".to_string(),
            pine_color_12_26: "Red".to_string(),
            channel_type: "Fast".to_string(),
            details: "".to_string(),
        };

        assert_eq!(record.symbol, "BTCUSDT");
        assert_eq!(record.event, "TR_RATIO_BREAK");
    }

    #[test]
    fn test_format_decimal() {
        use rust_decimal_macros::dec;

        let d = dec!(123.456789012345);
        assert_eq!(format_decimal(&d), "123.456789012345");

        let d2 = dec!(123.4567890123456789);
        assert!(format_decimal(&d2).len() <= 20);
    }
}
