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
