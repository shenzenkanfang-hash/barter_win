//! 终端表格展示
//!
//! 使用 ANSI 颜色码输出彩色状态表格。

use super::types::{ModuleStatus, SystemOverview};

/// 渲染系统总览到终端表格
pub fn render(overview: &SystemOverview) {
    use chrono::Local;

    // 头部
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    let uptime = format_uptime(overview.uptime_sec);
    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!(
        "║  {}  运行{} | 刷新10.0s                                    ║",
        now, uptime
    );
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    // 模块表格
    println!("{:<8} {:<6} {:<10} {}", "模块", "状态", "延迟", "摘要");
    println!("─".repeat(80));

    for module in &overview.modules {
        let status_str = match module.status {
            ModuleStatus::Ok => "OK",
            ModuleStatus::Warn => "WARN",
            ModuleStatus::Error => "ERROR",
            ModuleStatus::Unknown => "UNK",
        };

        let status_color = match module.status {
            ModuleStatus::Ok => "\x1b[32m",      // 绿色
            ModuleStatus::Warn => "\x1b[33m",   // 黄色
            ModuleStatus::Error => "\x1b[31m",   // 红色
            ModuleStatus::Unknown => "\x1b[90m", // 灰色
        };
        let reset = "\x1b[0m";

        println!(
            "{:<8} {}{:<6}{} {:>6}ms  {}",
            module.id.display_name(),
            status_color,
            status_str,
            reset,
            module.fresh_ms,
            module.summary
        );
    }

    println!("─".repeat(80));

    // 统计摘要
    let stats = &overview.stats;
    println!(
        "总计: \x1b[32mOK={}\x1b[0m  \x1b[33mWARN={}\x1b[0m  \x1b[31mERROR={}\x1b[0m  \x1b[90mUNKNOWN={}\x1b[0m",
        stats.ok, stats.warn, stats.error, stats.unknown
    );

    // 告警
    if let Some(alert) = &overview.alert {
        println!();
        println!("\x1b[1;31m[告警] {}\x1b[0m", alert);
    } else {
        println!();
        println!("\x1b[32m[状态] 全部正常\x1b[0m");
    }

    println!();
}

/// 格式化运行时间
fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
