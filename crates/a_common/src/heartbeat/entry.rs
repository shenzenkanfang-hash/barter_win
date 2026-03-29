use chrono::{DateTime, Utc};

/// 报到条目 - 记录每个测试点的报到状态
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
        }
    }

    /// 记录一次报到
    pub fn record_report(&mut self, seq: u64) {
        self.reports_count += 1;
        self.last_report_at = Utc::now();
        self.last_heartbeat_seq = seq;
        self.is_stale = false;
    }

    /// 检查是否失联
    pub fn check_stale(&mut self, current_seq: u64, threshold: u64) -> bool {
        if current_seq - self.last_heartbeat_seq > threshold {
            self.is_stale = true;
        }
        self.is_stale
    }
}
