//! JSON Lines 日志写入服务
//!
//! 使用 tokio 异步文件 I/O，实现非阻塞日志写入。
//! 单条日志发送 < 1ms，异步后台任务负责实际写入。

use std::path::PathBuf;
use chrono::Local;

/// 日志文件目录
static LOG_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// 初始化日志目录
pub fn init_log_dir(dir: PathBuf) {
    LOG_DIR.set(dir).ok();
}

/// 获取今日日志文件路径
fn today_log_path() -> PathBuf {
    let dir = LOG_DIR.get_or_init(|| PathBuf::from("./logs"));
    let _ = std::fs::create_dir_all(dir);
    let today = Local::now().format("%Y%m%d");
    dir.join(format!("trading.{}.jsonl", today))
}

/// JSON Lines Writer Service（后台任务）
///
/// 接收 JSON 字符串，写入今日日志文件。
/// 使用 tokio 异步文件 I/O，非阻塞。
pub struct JsonLinesWriter {
    tx: tokio::sync::mpsc::Sender<String>,
}

impl JsonLinesWriter {
    /// 创建新的 Writer Service
    pub fn new() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10000);
        let path = today_log_path();

        // 后台写入任务
        tokio::spawn(async move {
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .expect("failed to open log file");

            // 4KB 缓冲，减少系统调用
            let mut writer = tokio::io::BufWriter::with_capacity(4096, file);

            while let Some(line) = rx.recv().await {
                if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut writer, line.as_bytes()).await {
                    tracing::error!("failed to write log: {}", e);
                }
                if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut writer, b"\n").await {
                    tracing::error!("failed to write newline: {}", e);
                }
                if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut writer).await {
                    tracing::error!("failed to flush log: {}", e);
                }
            }
        });

        Self { tx }
    }

    /// 异步发送日志行（< 1ms，非阻塞）
    ///
    /// 使用 try_send 而非 send，超限时丢弃而非阻塞。
    pub async fn write(&self, line: String) {
        let _ = self.tx.try_send(line);
    }

    /// 同步发送日志行（用于 tracing layer 直接调用）
    pub fn try_write(&self, line: String) {
        let _ = self.tx.try_send(line);
    }
}

impl Default for JsonLinesWriter {
    fn default() -> Self {
        Self::new()
    }
}
