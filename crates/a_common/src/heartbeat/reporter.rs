use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{Clock, Config, Entry, Mode, Token};
use super::points::get_point_name;

/// 心跳报告器核心
pub struct HeartbeatReporter {
    clock: Clock,
    entries: RwLock<HashMap<String, Entry>>,
    config: Config,
    mode: RwLock<Arc<Mode>>,
}

impl HeartbeatReporter {
    pub fn new(config: Config) -> Self {
        Self {
            clock: Clock::new(),
            entries: RwLock::new(HashMap::new()),
            config,
            mode: RwLock::new(Arc::new(Mode::default())),
        }
    }

    /// 生成新的心跳标记
    pub async fn generate_token(&self) -> Token {
        let seq = self.clock.next_sequence();
        // Update stale status for all entries
        let mut entries = self.entries.write().await;
        for entry in entries.values_mut() {
            entry.check_stale(seq, self.config.stale_threshold);
        }
        drop(entries);
        let started_at = std::time::Instant::now();
        Token::new(seq, started_at)
    }

    /// 报到
    pub async fn report(
        &self,
        token: &Token,
        point_id: &str,
        module: &str,
        function: &str,
        file: &str,
    ) {
        // 检查模式
        let mode = self.mode.read().await;
        if !mode.should_report() {
            return;
        }

        let mut entries = self.entries.write().await;
        let entry = entries.entry(point_id.to_string()).or_insert_with(|| {
            let name = get_point_name(point_id).unwrap_or("Unknown");
            Entry::new(point_id, name, module, function, file)
        });
        entry.record_report(token.sequence);
    }

    /// 获取摘要
    pub async fn summary(&self) -> Summary {
        let entries = self.entries.read().await;
        let total = entries.len();
        let active = entries.values().filter(|e| !e.is_stale).count();
        let stale = entries.values().filter(|e| e.is_stale).count();
        let reports: u64 = entries.values().map(|e| e.reports_count).sum();
        Summary { total_points: total, active_count: active, inactive_count: stale, reports_count: reports }
    }

    /// 获取失联点
    pub async fn get_stale_points(&self) -> Vec<String> {
        let entries = self.entries.read().await;
        entries.values()
            .filter(|e| e.is_stale)
            .map(|e| e.point_id.clone())
            .collect()
    }

    /// 设置模式
    pub async fn set_mode(&self, mode: Mode) {
        *self.mode.write().await = Arc::new(mode);
    }
}

/// 摘要统计
#[derive(Clone, Debug)]
pub struct Summary {
    pub total_points: usize,
    pub active_count: usize,
    pub inactive_count: usize,
    pub reports_count: u64,
}
