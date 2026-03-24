//! SQLite 持久化层

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{params, Connection};

use super::error::{Result, StrategyStateError};
use super::state::StrategyState;

const CREATE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS strategy_states (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_strategy_states_updated 
ON strategy_states(updated_at);
"#;

/// 数据库服务
pub struct StrategyStateDb {
    conn: Arc<Mutex<Connection>>,
}

impl StrategyStateDb {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(CREATE_TABLE_SQL)?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(CREATE_TABLE_SQL)?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 保存策略状态
    pub fn save(&self, state: &StrategyState) -> Result<()> {
        let conn = self.conn.lock();
        let data = serde_json::to_string(state)?;
        let updated_at = state.last_update_time.timestamp();
        
        conn.execute(
            "INSERT OR REPLACE INTO strategy_states (id, data, updated_at) VALUES (?1, ?2, ?3)",
            params![state.id(), data, updated_at],
        )?;
        
        Ok(())
    }

    /// 批量保存
    pub fn save_batch(&self, states: &[StrategyState]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        
        for state in states {
            let data = serde_json::to_string(state)?;
            let updated_at = state.last_update_time.timestamp();
            
            tx.execute(
                "INSERT OR REPLACE INTO strategy_states (id, data, updated_at) VALUES (?1, ?2, ?3)",
                params![state.id(), data, updated_at],
            )?;
        }
        
        tx.commit()?;
        Ok(())
    }

    /// 根据 ID 加载
    pub fn load(&self, id: &str) -> Result<Option<StrategyState>> {
        let conn = self.conn.lock();
        
        let mut stmt = conn.prepare("SELECT data FROM strategy_states WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        
        if let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let state: StrategyState = serde_json::from_str(&data)?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    /// 加载所有策略状态
    pub fn load_all(&self) -> Result<Vec<StrategyState>> {
        let conn = self.conn.lock();
        
        let mut stmt = conn.prepare("SELECT data FROM strategy_states")?;
        let mut rows = stmt.query([])?;
        
        let mut states = Vec::new();
        while let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let state: StrategyState = serde_json::from_str(&data)?;
            states.push(state);
        }
        
        Ok(states)
    }

    /// 根据 instrument_id 前缀加载
    pub fn load_by_instrument(&self, instrument_id: &str) -> Result<Vec<StrategyState>> {
        let conn = self.conn.lock();
        
        let pattern = format!("{}%", instrument_id);
        let mut stmt = conn.prepare("SELECT data FROM strategy_states WHERE id LIKE ?1")?;
        let mut rows = stmt.query(params![pattern])?;
        
        let mut states = Vec::new();
        while let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let state: StrategyState = serde_json::from_str(&data)?;
            states.push(state);
        }
        
        Ok(states)
    }

    /// 删除策略状态
    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM strategy_states WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// 清空所有数据
    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM strategy_states", [])?;
        Ok(())
    }

    /// 获取记录数
    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM strategy_states", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

impl Clone for StrategyStateDb {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_and_load() {
        let db = StrategyStateDb::in_memory().unwrap();
        
        let state = StrategyState::new(
            "BTC-USDT".to_string(),
            "trend_v1".to_string(),
            "binance".to_string(),
            "1h".to_string(),
        );
        
        db.save(&state).unwrap();
        
        let loaded = db.load(&state.id()).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.instrument_id, "BTC-USDT");
        assert_eq!(loaded.strategy_id, "trend_v1");
    }

    #[test]
    fn test_save_batch() {
        let db = StrategyStateDb::in_memory().unwrap();
        
        let states = vec![
            StrategyState::new("BTC-USDT".to_string(), "trend_v1".to_string(), "binance".to_string(), "1h".to_string()),
            StrategyState::new("ETH-USDT".to_string(), "trend_v1".to_string(), "binance".to_string(), "1h".to_string()),
        ];
        
        db.save_batch(&states).unwrap();
        assert_eq!(db.count().unwrap(), 2);
    }

    #[test]
    fn test_delete() {
        let db = StrategyStateDb::in_memory().unwrap();
        
        let state = StrategyState::new("BTC-USDT".to_string(), "trend_v1".to_string(), "binance".to_string(), "1h".to_string());
        db.save(&state).unwrap();
        assert_eq!(db.count().unwrap(), 1);
        
        db.delete(&state.id()).unwrap();
        assert_eq!(db.count().unwrap(), 0);
    }
}
