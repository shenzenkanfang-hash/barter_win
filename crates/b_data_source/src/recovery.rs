//! 市场数据灾备恢复模块
//!
//! Redis 仅作为灾备用途：
//! - 程序崩溃后能快速恢复交易判断
//! - 不用等待 15min 窗口结束
//! - 不用重新 warm-up 指标
//!
//! 需要灾备的数据：
//! - K线历史（1m/15m/1d）
//! - 通道状态（Slow/Fast）
//! - 高波动窗口标记
//! - 最后 checkpoint 时间戳

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::claint::MarketError;

/// Checkpoint 数据 - 用于快速恢复交易判断
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    /// 品种
    pub symbol: String,
    /// checkpoint 时间戳
    pub timestamp: i64,
    /// 当前通道类型
    pub channel_type: String,
    /// 是否在高波动窗口中
    pub is_in_high_vol_window: bool,
    /// 高波动窗口开始时间（如果正在窗口中）
    pub high_vol_window_start: Option<i64>,
    /// 最后计算的指标快照（JSON）
    pub indicator_snapshot: String,
}

/// Redis 灾备存储
pub struct RedisRecovery {
    redis: Arc<Mutex<redis::aio::ConnectionManager>>,
}

impl RedisRecovery {
    /// 创建 RedisRecovery
    pub async fn new(redis_url: &str) -> Result<Self, MarketError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        let conn = client
            .get_connection_manager()
            .await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        Ok(Self {
            redis: Arc::new(Mutex::new(conn)),
        })
    }

    /// 保存 checkpoint
    pub async fn save_checkpoint(&self, data: &CheckpointData) -> Result<(), MarketError> {
        let key = format!("checkpoint:{}", data.symbol);
        let json = serde_json::to_string(data)
            .map_err(|e| MarketError::SerializeError(e.to_string()))?;
        let mut conn = self.redis.lock().await;
        let _: () = conn.set::<_, _, ()>(&key, &json).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        Ok(())
    }

    /// 加载 checkpoint
    pub async fn load_checkpoint(&self, symbol: &str) -> Result<Option<CheckpointData>, MarketError> {
        let key = format!("checkpoint:{}", symbol);
        let mut conn = self.redis.lock().await;
        let data: Option<String> = conn.get(&key).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        drop(conn);
        match data {
            Some(json_str) => {
                let checkpoint: CheckpointData = serde_json::from_str(&json_str)
                    .map_err(|e| MarketError::SerializeError(e.to_string()))?;
                Ok(Some(checkpoint))
            }
            None => Ok(None),
        }
    }

    /// 保存 K线历史（批量）
    pub async fn save_klines(&self, symbol: &str, period: &str, klines_json: &[String]) -> Result<(), MarketError> {
        let key = format!("kline:{}:{}", period, symbol);
        let mut conn = self.redis.lock().await;
        // 先删除旧数据
        let _: () = conn.del::<_, ()>(&key).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        // 批量写入（逆序保证时间正序）
        for kline_json in klines_json.iter().rev() {
            let _: () = conn.lpush::<_, _, ()>(&key, kline_json).await
                .map_err(|e| MarketError::RedisError(e.to_string()))?;
        }
        Ok(())
    }

    /// 加载 K线历史
    pub async fn load_klines(&self, symbol: &str, period: &str) -> Result<Vec<String>, MarketError> {
        let key = format!("kline:{}:{}", period, symbol);
        let mut conn = self.redis.lock().await;
        let klines: Vec<String> = conn.lrange(&key, 0, -1).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        Ok(klines)
    }

    /// 保存指标快照
    pub async fn save_indicator_snapshot(&self, symbol: &str, snapshot_json: &str) -> Result<(), MarketError> {
        let key = format!("indicator:{}", symbol);
        let mut conn = self.redis.lock().await;
        let _: () = conn.set::<_, _, ()>(&key, snapshot_json).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        Ok(())
    }

    /// 加载指标快照
    pub async fn load_indicator_snapshot(&self, symbol: &str) -> Result<Option<String>, MarketError> {
        let key = format!("indicator:{}", symbol);
        let mut conn = self.redis.lock().await;
        let data: Option<String> = conn.get(&key).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        Ok(data)
    }

    /// 删除 checkpoint（恢复完成后）
    pub async fn clear_checkpoint(&self, symbol: &str) -> Result<(), MarketError> {
        let key = format!("checkpoint:{}", symbol);
        let mut conn = self.redis.lock().await;
        let _: () = conn.del::<_, ()>(&key).await
            .map_err(|e| MarketError::RedisError(e.to_string()))?;
        Ok(())
    }

    /// 检查是否有可恢复的 checkpoint
    pub async fn has_checkpoint(&self, symbol: &str) -> Result<bool, MarketError> {
        let checkpoint = self.load_checkpoint(symbol).await?;
        Ok(checkpoint.is_some())
    }
}

/// CheckpointManager - 定时保存 checkpoint
pub struct CheckpointManager {
    recovery: Arc<RedisRecovery>,
    checkpoint_interval_secs: u64,
}

impl CheckpointManager {
    /// 创建 CheckpointManager
    pub fn new(recovery: Arc<RedisRecovery>, checkpoint_interval_secs: u64) -> Self {
        Self {
            recovery,
            checkpoint_interval_secs,
        }
    }

    /// 创建默认 manager（30秒 checkpoint）
    pub fn with_defaults(recovery: Arc<RedisRecovery>) -> Self {
        Self::new(recovery, 30)
    }

    /// 获取 checkpoint 间隔（秒）
    pub fn interval(&self) -> u64 {
        self.checkpoint_interval_secs
    }

    /// 获取恢复器引用
    pub fn recovery(&self) -> &Arc<RedisRecovery> {
        &self.recovery
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_data_serialization() {
        let data = CheckpointData {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1710000000,
            channel_type: "High".to_string(),
            is_in_high_vol_window: true,
            high_vol_window_start: Some(1709999900),
            indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
        };

        let json = serde_json::to_string(&data).unwrap();
        let restored: CheckpointData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.symbol, "BTCUSDT");
        assert_eq!(restored.channel_type, "High");
        assert!(restored.is_in_high_vol_window);
    }
}
