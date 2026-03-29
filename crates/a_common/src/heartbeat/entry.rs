use chrono::{DateTime, Utc};

/// 报到条目 - 记录每个测试点的报到状态和延迟统计
#[derive(Clone, Debug)]
pub struct ReportEntry {
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
    /// 最后报到对应的心跳序号
    pub last_heartbeat_seq: u64,
    /// 是否失联
    pub is_stale: bool,

    // ===== 延迟统计字段 =====
    /// 总延迟累计（用于计算平均值）
    pub total_latency_ms: i64,
    /// 最大延迟（毫秒）
    pub max_latency_ms: i64,
    /// 最小延迟（毫秒）
    pub min_latency_ms: i64,
    /// 最后延迟（毫秒）
    pub last_latency_ms: i64,
}

impl ReportEntry {
    pub fn new(
        point_id: &str,
        point_name: &str,
        module: &str,
        function: &str,
        file: &str,
    ) -> Self {
        Self {
            point_id: point_id.to_string(),
            point_name: point_name.to_string(),
            module: module.to_string(),
            function: function.to_string(),
            file: file.to_string(),
            reports_count: 0,
            last_report_at: Utc::now(),
            last_heartbeat_seq: 0,
            is_stale: false,
            // 延迟统计初始化
            total_latency_ms: 0,
            max_latency_ms: 0,
            min_latency_ms: i64::MAX,  // 初始为最大值，后续会更新
            last_latency_ms: 0,
        }
    }

    /// 记录一次报到（无延迟信息）
    pub fn record_report(&mut self, seq: u64) {
        self.reports_count += 1;
        self.last_report_at = Utc::now();
        self.last_heartbeat_seq = seq;
        self.is_stale = false;
    }

    /// 记录一次报到（带延迟信息）
    pub fn record_report_with_latency(&mut self, seq: u64, latency_ms: i64) {
        self.reports_count += 1;
        self.last_report_at = Utc::now();
        self.last_heartbeat_seq = seq;
        self.is_stale = false;

        // 更新延迟统计
        self.last_latency_ms = latency_ms;
        self.total_latency_ms += latency_ms;
        if latency_ms > self.max_latency_ms {
            self.max_latency_ms = latency_ms;
        }
        if latency_ms < self.min_latency_ms {
            self.min_latency_ms = latency_ms;
        }
    }

    /// 获取平均延迟（毫秒）
    pub fn avg_latency_ms(&self) -> Option<i64> {
        if self.reports_count == 0 {
            None
        } else {
            Some(self.total_latency_ms / self.reports_count as i64)
        }
    }

    /// 检查是否失联
    pub fn check_stale(&mut self, current_seq: u64, threshold: u64) -> bool {
        // Use checked subtraction to avoid panic
        let diff = current_seq.saturating_sub(self.last_heartbeat_seq);
        if diff > threshold {
            self.is_stale = true;
        } else {
            self.is_stale = false; // Clear stale when active
        }
        self.is_stale
    }
}
