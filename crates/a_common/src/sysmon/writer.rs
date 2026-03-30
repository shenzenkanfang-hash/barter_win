//! 心跳写入器 - 模块上报心跳到 tmpfs
//!
//! 使用 tokio 异步写入，不阻塞调用方。

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::types::{Config, Heartbeat, ModuleId, ModuleStatus};

/// 心跳写入器
pub struct HeartbeatWriter {
    config: Config,
    /// 每个模块的序号
    sequences: [AtomicU64; 8],
}

impl HeartbeatWriter {
    /// 创建新的写入器
    pub fn new(config: Config) -> io::Result<Self> {
        // 创建目录（如果不存在）
        std::fs::create_dir_all(&config.path)?;

        Ok(Self {
            config,
            sequences: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        })
    }

    /// 获取配置
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 发送心跳（非阻塞，失败静默跳过）
    pub fn beat(
        &self,
        module: ModuleId,
        status: ModuleStatus,
        metrics: serde_json::Value,
        action: &str,
        next_exp: &str,
    ) {
        // 获取并递增序号
        let idx = module as usize;
        let seq = self.sequences[idx].fetch_add(1, Ordering::Relaxed);

        let heartbeat = Heartbeat {
            t: chrono::Utc::now().timestamp_millis(),
            s: status,
            seq,
            m: metrics,
            a: action.to_string(),
            n: next_exp.to_string(),
        };

        // 异步写入（使用 tokio spawn，不阻塞线程池）
        let path = self.config.path.join(module.file_name());
        let tmp_path = self.config
            .path
            .join(format!("{}.tmp", module.file_name()));

        tokio::spawn(async move {
            if let Err(e) = atomic_write_json(&tmp_path, &path, &heartbeat) {
                // 静默失败，下次心跳再试
                tracing::trace!("[sysmon] heartbeat write failed for {:?}: {}", module, e);
            }
        });
    }

    /// 发送 OK 心跳（简化版）
    pub fn beat_ok(&self, module: ModuleId, metrics: serde_json::Value) {
        self.beat(module, ModuleStatus::Ok, metrics, "", "");
    }

    /// 发送错误心跳
    pub fn beat_error(&self, module: ModuleId, error_msg: &str) {
        let metrics = serde_json::json!({ "error": error_msg });
        self.beat(
            module,
            ModuleStatus::Error,
            metrics,
            error_msg,
            "等待恢复",
        );
    }
}

/// 原子写入 JSON（写临时文件 -> rename）
fn atomic_write_json(
    tmp_path: &Path,
    target_path: &Path,
    heartbeat: &Heartbeat,
) -> io::Result<()> {
    // 序列化为紧凑 JSON（无缩进，减少文件大小）
    let json =
        serde_json::to_string(heartbeat).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // 写入临时文件
    std::fs::write(tmp_path, json)?;

    // rename 覆盖（原子操作，读取方不会看到半写）
    std::fs::rename(tmp_path, target_path)?;

    Ok(())
}

/// 全局写入器（使用 OnceLock 实现）
static WRITER: std::sync::OnceLock<Arc<HeartbeatWriter>> = std::sync::OnceLock::new();

/// 初始化全局写入器
pub fn init(config: Config) -> io::Result<()> {
    let writer = HeartbeatWriter::new(config)?;
    WRITER.set(Arc::new(writer)).ok();
    Ok(())
}

/// 获取全局写入器
pub fn global() -> Option<Arc<HeartbeatWriter>> {
    WRITER.get().cloned()
}
