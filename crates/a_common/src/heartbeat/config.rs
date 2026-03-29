/// 报告器配置
#[derive(Clone, Debug)]
pub struct ReporterConfig {
    /// 失联阈值 (心跳周期数)
    pub stale_threshold: u64,
    /// 报告生成间隔 (秒)
    pub report_interval_secs: u64,
    /// 文件保留时间 (小时)
    pub max_file_age_hours: u64,
    /// 文件大小限制 (MB)
    pub max_file_size_mb: u64,
}

impl Default for ReporterConfig {
    fn default() -> Self {
        Self {
            stale_threshold: 3,
            report_interval_secs: 300,
            max_file_age_hours: 24,
            max_file_size_mb: 100,
        }
    }
}
