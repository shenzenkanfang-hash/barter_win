//! 市场数据灾备恢复模块 - 简化版
//!
//! 不依赖 Redis，纯文件存储

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use parking_lot::RwLock;

/// Checkpoint 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub symbol: String,
    pub timestamp: i64,
    pub channel_type: String,
    pub is_in_high_vol_window: bool,
    pub high_vol_window_start: Option<i64>,
    pub indicator_snapshot: String,
}

/// 模拟灾备存储（纯文件）
pub struct MockRecovery {
    checkpoints: RwLock<HashMap<String, CheckpointData>>,
    backup_dir: PathBuf,
}

impl MockRecovery {
    pub fn new(backup_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&backup_dir).ok();
        Self {
            checkpoints: RwLock::new(HashMap::new()),
            backup_dir,
        }
    }

    pub fn save_checkpoint(&self, data: &CheckpointData) -> Result<(), a_common::MarketError> {
        // 保存到内存
        self.checkpoints.write().insert(data.symbol.clone(), data.clone());

        // 保存到文件
        let path = self.backup_dir.join(format!("checkpoint_{}.json", data.symbol.to_lowercase()));
        let json = serde_json::to_string(data)
            .map_err(|e| a_common::MarketError::SerializeError(e.to_string()))?;
        std::fs::write(&path, json)
            .map_err(|e| a_common::MarketError::ParseError(e.to_string()))?;

        Ok(())
    }

    pub fn load_checkpoint(&self, symbol: &str) -> Result<Option<CheckpointData>, a_common::MarketError> {
        // 先检查内存
        if let Some(cp) = self.checkpoints.read().get(symbol).cloned() {
            return Ok(Some(cp));
        }

        // 从文件加载
        let path = self.backup_dir.join(format!("checkpoint_{}.json", symbol.to_lowercase()));
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| a_common::MarketError::ParseError(e.to_string()))?;
        let checkpoint: CheckpointData = serde_json::from_str(&content)
            .map_err(|e| a_common::MarketError::SerializeError(e.to_string()))?;

        Ok(Some(checkpoint))
    }

    pub fn clear_checkpoint(&self, symbol: &str) -> Result<(), a_common::MarketError> {
        self.checkpoints.write().remove(symbol);
        let path = self.backup_dir.join(format!("checkpoint_{}.json", symbol.to_lowercase()));
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        Ok(())
    }
}

/// Checkpoint 管理器（简化版）
pub struct CheckpointManager {
    recovery: MockRecovery,
}

impl CheckpointManager {
    pub fn new(backup_dir: PathBuf) -> Self {
        Self {
            recovery: MockRecovery::new(backup_dir),
        }
    }

    pub fn recovery(&self) -> &MockRecovery {
        &self.recovery
    }
}
