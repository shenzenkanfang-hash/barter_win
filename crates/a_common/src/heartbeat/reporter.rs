use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::Serialize;
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

    /// 报到（无延迟信息）
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

    /// 报到（带延迟信息）
    pub async fn report_with_latency(
        &self,
        token: &Token,
        point_id: &str,
        module: &str,
        function: &str,
        file: &str,
        latency_ms: i64,
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
        entry.record_report_with_latency(token.sequence, latency_ms);
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
    pub async fn get_stale_points(&self) -> Vec<StalePoint> {
        let entries = self.entries.read().await;
        entries.values()
            .filter(|e| e.is_stale)
            .map(|e| StalePoint {
                point_id: e.point_id.clone(),
                since_sequence: e.last_heartbeat_seq,
            })
            .collect()
    }

    /// 生成完整报告
    pub async fn generate_report(&self) -> HeartbeatReport {
        let entries = self.entries.read().await;
        let current_seq = self.clock.current_sequence();
        let started_seq = self.clock.started_sequence();

        let _total = entries.len();
        let active = entries.values().filter(|e| !e.is_stale).count();
        let stale = entries.values().filter(|e| e.is_stale).count();
        let reports: u64 = entries.values().map(|e| e.reports_count).sum();

        let points_detail: Vec<PointDetail> = entries.values()
            .map(|e| PointDetail {
                point_id: e.point_id.clone(),
                point_name: e.point_name.clone(),
                module: e.module.clone(),
                function: e.function.clone(),
                file: e.file.clone(),
                reports_count: e.reports_count,
                last_report_at: e.last_report_at,
                is_stale: e.is_stale,
                // 延迟统计
                avg_latency_ms: e.avg_latency_ms(),
                max_latency_ms: if e.max_latency_ms == 0 && e.reports_count == 0 { None } else { Some(e.max_latency_ms) },
                last_latency_ms: if e.reports_count == 0 { None } else { Some(e.last_latency_ms) },
            })
            .collect();

        let stale_points: Vec<StalePoint> = entries.values()
            .filter(|e| e.is_stale)
            .map(|e| StalePoint {
                point_id: e.point_id.clone(),
                since_sequence: e.last_heartbeat_seq,
            })
            .collect();

        HeartbeatReport {
            heartbeat_sequence: current_seq,
            duration_minutes: current_seq.saturating_sub(started_seq),
            total_reports: reports,
            active_points: active,
            stale_points_count: stale,
            points_detail,
            stale_points,
        }
    }

    /// 保存报告到文件
    pub async fn save_report(&self, path: &str) -> std::io::Result<()> {
        let report = self.generate_report().await;
        let json = serde_json::to_string_pretty(&report)?;
        tokio::fs::write(path, json).await
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

/// 心跳报告
#[derive(Clone, Debug, Serialize)]
pub struct HeartbeatReport {
    /// 当前心跳序号
    pub heartbeat_sequence: u64,
    /// 运行时长（心跳周期数）
    pub duration_minutes: u64,
    /// 总报到次数
    pub total_reports: u64,
    /// 活跃测试点数量
    pub active_points: usize,
    /// 失联测试点数量
    pub stale_points_count: usize,
    /// 测试点详情列表
    pub points_detail: Vec<PointDetail>,
    /// 失联点详情列表
    pub stale_points: Vec<StalePoint>,
}

/// 测试点详情
#[derive(Clone, Debug, Serialize)]
pub struct PointDetail {
    /// 测试点ID
    pub point_id: String,
    /// 测试点名称
    pub point_name: String,
    /// 模块名
    pub module: String,
    /// 函数名
    pub function: String,
    /// 文件路径
    pub file: String,
    /// 报到次数
    pub reports_count: u64,
    /// 最后报到时间
    pub last_report_at: DateTime<Utc>,
    /// 是否失联
    pub is_stale: bool,
    // ===== 延迟统计 =====
    /// 平均延迟（毫秒）
    pub avg_latency_ms: Option<i64>,
    /// 最大延迟（毫秒）
    pub max_latency_ms: Option<i64>,
    /// 最后延迟（毫秒）
    pub last_latency_ms: Option<i64>,
}

/// 失联点信息
#[derive(Clone, Debug, Serialize)]
pub struct StalePoint {
    /// 测试点ID
    pub point_id: String,
    /// 自该序号后失联
    pub since_sequence: u64,
}
