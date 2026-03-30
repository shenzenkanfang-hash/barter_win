//! JSON Lines 日志写入服务
//!
//! 使用 tokio 异步文件 I/O，实现非阻塞日志写入。
//! 单条日志发送 < 1ms，异步后台任务负责实际写入。

use std::path::PathBuf;
use chrono::Local;
use tracing_subscriber::layer::Layer;

/// 日志文件目录
static LOG_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// JSON Lines Writer 全局句柄
static WRITER: std::sync::OnceLock<JsonLinesWriter> = std::sync::OnceLock::new();

/// 初始化日志目录
pub fn init_log_dir(dir: PathBuf) {
    LOG_DIR.set(dir).ok();
}

/// 获取全局 JsonLinesWriter（必须在 init_log_dir 之后调用）
pub fn get_writer() -> Option<&'static JsonLinesWriter> {
    WRITER.get()
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
    /// 创建新的 Writer Service 并注册为全局单例
    pub fn new() -> &'static Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10000);
        let path = today_log_path();

        // 后台写入任务
        tokio::spawn(async move {
            let file = match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("[JsonLinesWriter] failed to open log file {}: {}", path.display(), e);
                    return;
                }
            };

            // 4KB 缓冲，减少系统调用
            let mut writer = tokio::io::BufWriter::with_capacity(4096, file);

            while let Some(line) = rx.recv().await {
                if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut writer, line.as_bytes()).await {
                    tracing::error!("[JsonLinesWriter] failed to write log: {}", e);
                    continue;
                }
                if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut writer, b"\n").await {
                    tracing::error!("[JsonLinesWriter] failed to write newline: {}", e);
                    continue;
                }
                if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut writer).await {
                    tracing::error!("[JsonLinesWriter] failed to flush log: {}", e);
                }
            }
        });

        WRITER.get_or_init(|| Self { tx })
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
        panic!("JsonLinesWriter::default() called — use JsonLinesWriter::new() instead")
    }
}

// ============================================================================
// Tracing Layer — 将 tracing::info! 等调用写入 JSON Lines 文件
// ============================================================================

/// JSON Lines tracing layer
pub struct JsonLinesLayer;

impl JsonLinesLayer {
    /// 构建带 JSON Lines layer 的 tracing subscriber
    pub fn build() -> impl Layer<tracing_subscriber::registry::Registry> {
        Self
    }
}

impl Layer<tracing_subscriber::registry::Registry> for JsonLinesLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, tracing_subscriber::registry::Registry>,
    ) {
        let writer = match get_writer() {
            Some(w) => w,
            None => return, // writer 尚未初始化，静默跳过
        };

        // 提取关键字段
        let mut component = String::new();
        let mut log_event = String::new();
        let mut symbol = String::new();
        let mut tick_id: Option<u64> = None;
        let mut reason = String::new();

        let mut visitor = JsonEventVisitor {
            component: &mut component,
            event: &mut log_event,
            symbol: &mut symbol,
            tick_id: &mut tick_id,
            reason: &mut reason,
        };

        event.record(&mut visitor);

        // 构造 JSON Line
        let ts = chrono::Utc::now().to_rfc3339();
        let level = format!("{:?}", event.metadata().level());
        let target = event.metadata().target();

        // 从 metadata 获取 event name（作为 message 兜底）
        let message = event.metadata().name();

        // 只包含有值的字段（避免空字段噪声）
        let mut obj = serde_json::Map::with_capacity(6);
        obj.insert("ts".to_string(), serde_json::Value::String(ts));
        obj.insert("level".to_string(), serde_json::Value::String(level));
        obj.insert("target".to_string(), serde_json::Value::String(target.to_string()));
        obj.insert("message".to_string(), serde_json::Value::String(message.to_string()));
        if !component.is_empty() {
            obj.insert("component".to_string(), serde_json::Value::String(component));
        }
        if !log_event.is_empty() {
            obj.insert("event".to_string(), serde_json::Value::String(log_event));
        }
        if !symbol.is_empty() {
            obj.insert("symbol".to_string(), serde_json::Value::String(symbol));
        }
        if let Some(tid) = tick_id {
            obj.insert("tick_id".to_string(), serde_json::Value::String(tid.to_string()));
        }
        if !reason.is_empty() {
            obj.insert("reason".to_string(), serde_json::Value::String(reason));
        }

        if let Ok(json) = serde_json::to_string(&serde_json::Value::Object(obj)) {
            writer.try_write(json);
        }
    }
}

/// tracing::field::Visit 实现 — 从事件字段中提取数据
struct JsonEventVisitor<'a> {
    component: &'a mut String,
    event: &'a mut String,
    symbol: &'a mut String,
    tick_id: &'a mut Option<u64>,
    reason: &'a mut String,
}

impl<'a> tracing::field::Visit for JsonEventVisitor<'a> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "component" => self.component.push_str(value),
            "event" => self.event.push_str(value),
            "symbol" => self.symbol.push_str(value),
            "reason" => self.reason.push_str(value),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        match field.name() {
            "tick_id" => *self.tick_id = Some(value),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        // 处理其他数值字段（Decimal 等 Debug 打印）
        match field.name() {
            "component" | "event" | "symbol" | "reason" => {}
            _ => {
                // 将其他字段序列化为 Debug 字符串备用
                let _ = value;
            }
        }
    }
}
