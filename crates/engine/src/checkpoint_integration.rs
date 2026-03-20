//! Checkpoint 集成模块
//!
//! 将 VolatilityChannel 的通道状态通过 Redis 持久化

use crate::channel::{ChannelCheckpointCallback, ChannelType};
use market::{CheckpointData, RedisRecovery};
use std::sync::Arc;

/// Redis Checkpoint 回调实现
///
/// 注意：由于 Redis 操作需要 &mut self，这个回调必须在 async 上下文中使用
/// 通道切换时会调用回调，但实际的保存操作需要调用方在 async 上下文中处理
pub struct RedisCheckpointCallback {
    recovery: Arc<RedisRecovery>,
    /// 指标快照序列化字符串
    indicator_snapshot: std::sync::Mutex<String>,
}

impl RedisCheckpointCallback {
    /// 创建新的 RedisCheckpointCallback
    pub fn new(recovery: Arc<RedisRecovery>) -> Self {
        Self {
            recovery,
            indicator_snapshot: std::sync::Mutex::new(String::new()),
        }
    }

    /// 更新指标快照
    pub fn update_indicator_snapshot(&self, snapshot: String) {
        let mut guard = self.indicator_snapshot.lock().unwrap();
        *guard = snapshot;
    }

    /// 异步保存当前状态到 Redis
    pub async fn save_current_checkpoint(&self, symbol: &str) {
        let snapshot = {
            let guard = self.indicator_snapshot.lock().unwrap();
            guard.clone()
        };

        // 需要从 VolatilityChannel 获取实际状态，这里先做占位
        let data = CheckpointData {
            symbol: symbol.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            channel_type: "Slow".to_string(), // 实际应该从 channel 获取
            is_in_high_vol_window: false,
            high_vol_window_start: None,
            indicator_snapshot: snapshot,
        };

        if let Err(e) = self.recovery.save_checkpoint(&data).await {
            tracing::warn!("Failed to save checkpoint for {}: {}", symbol, e);
        }
    }
}

impl ChannelCheckpointCallback for RedisCheckpointCallback {
    fn on_channel_switch(
        &self,
        symbol: &str,
        channel: ChannelType,
        is_high_vol: bool,
        high_vol_start: Option<i64>,
    ) {
        let snapshot = {
            let guard = self.indicator_snapshot.lock().unwrap();
            guard.clone()
        };

        let data = CheckpointData {
            symbol: symbol.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            channel_type: channel.to_string(),
            is_in_high_vol_window: is_high_vol,
            high_vol_window_start: high_vol_start,
            indicator_snapshot: snapshot,
        };

        // 创建 async 任务来保存（不等待完成，避免阻塞主流程）
        let recovery = self.recovery.clone();
        let data_for_task = data;
        let symbol_for_task = symbol.to_string();

        // 使用 tokio::spawn 但需要确保 ConnectionManager 不会被跨线程发送
        // 由于 Arc 内部是同一线程的 MutexGuard，这个操作是安全的
        tokio::spawn(async move {
            if let Err(e) = recovery.save_checkpoint(&data_for_task).await {
                tracing::warn!("Failed to save checkpoint for {}: {}", symbol_for_task, e);
            }
        });
    }
}

/// 恢复检查点数据
pub async fn load_checkpoint(
    recovery: &RedisRecovery,
    symbol: &str,
) -> Option<CheckpointData> {
    match recovery.load_checkpoint(symbol).await {
        Ok(Some(data)) => {
            tracing::info!(
                "Loaded checkpoint for {}: channel={} is_high_vol={}",
                symbol,
                data.channel_type,
                data.is_in_high_vol_window
            );
            Some(data)
        }
        Ok(None) => {
            tracing::debug!("No checkpoint found for {}", symbol);
            None
        }
        Err(e) => {
            tracing::warn!("Failed to load checkpoint for {}: {}", symbol, e);
            None
        }
    }
}

/// 检查是否有可恢复的 checkpoint
pub async fn has_checkpoint(recovery: &RedisRecovery, symbol: &str) -> bool {
    match recovery.has_checkpoint(symbol).await {
        Ok(has) => has,
        Err(e) => {
            tracing::warn!("Failed to check checkpoint for {}: {}", symbol, e);
            false
        }
    }
}
