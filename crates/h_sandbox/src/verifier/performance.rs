//! Performance Benchmark - 性能基准测试
//!
//! 测量 Tick-to-Decision、Decision-to-Order、端到端延迟

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use chrono::{DateTime, Utc};

/// 延迟记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyRecord {
    /// 记录时间
    pub timestamp: DateTime<Utc>,
    /// 阶段名称
    pub stage: LatencyStage,
    /// 延迟（微秒）
    pub latency_us: u64,
    /// 关联数据
    pub metadata: String,
}

/// 延迟阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LatencyStage {
    TickArrival,        // Tick 到达
    DataProcessing,     // 数据处理完成
    IndicatorCalc,      // 指标计算完成
    SignalGen,          // 信号生成完成
    DecisionMade,       // 决策完成
    OrderSubmitted,     // 订单提交
    OrderAck,          // 订单确认
}

/// 延迟统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    /// 阶段
    pub stage: LatencyStage,
    /// 样本数
    pub count: usize,
    /// 平均值 (微秒)
    pub avg_us: u64,
    /// 最小值 (微秒)
    pub min_us: u64,
    /// 最大值 (微秒)
    pub max_us: u64,
    /// P50 (微秒)
    pub p50_us: u64,
    /// P99 (微秒)
    pub p99_us: u64,
    /// P99.9 (微秒)
    pub p999_us: u64,
}

/// 性能报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    /// 测试开始时间
    pub test_start: DateTime<Utc>,
    /// 测试结束时间
    pub test_end: DateTime<Utc>,
    /// 总处理数量
    pub total_ticks: usize,
    /// 总订单数
    pub total_orders: usize,
    /// Tick 处理速率 (每秒)
    pub ticks_per_second: f64,
    /// 订单处理速率 (每秒)
    pub orders_per_second: f64,
    /// 各阶段延迟统计
    pub latency_stats: Vec<LatencyStats>,
    /// 端到端延迟统计
    pub e2e_latency: LatencyStats,
    /// 目标达成情况
    pub target_achievement: TargetAchievement,
}

/// 目标达成情况
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetAchievement {
    /// Tick-to-Decision 目标: < 100μs
    pub tick_to_decision_target: f64,  // 实际达成百分比
    /// Decision-to-Order 目标: < 50μs
    pub decision_to_order_target: f64,
    /// 端到端延迟目标: < 10ms
    pub e2e_target: f64,
}

/// 性能基准测试器
pub struct PerformanceBenchmark {
    /// 延迟记录
    records: VecDeque<LatencyRecord>,
    /// 待处理的 tick 时间戳
    pending_ticks: VecDeque<DateTime<Utc>>,
    /// 待处理的决策时间戳
    pending_decisions: VecDeque<DateTime<Utc>>,
    /// 待处理的订单时间戳
    pending_orders: VecDeque<DateTime<Utc>>,
    /// 开始时间
    test_start: DateTime<Utc>,
    /// 当前阶段开始时间
    stage_start: DateTime<Utc>,
    /// 订单计数
    order_count: usize,
}

impl PerformanceBenchmark {
    /// 创建基准测试器
    pub fn new() -> Self {
        Self {
            records: VecDeque::new(),
            pending_ticks: VecDeque::new(),
            pending_decisions: VecDeque::new(),
            pending_orders: VecDeque::new(),
            test_start: Utc::now(),
            stage_start: Utc::now(),
            order_count: 0,
        }
    }

    /// 记录 Tick 到达
    pub fn record_tick_arrival(&mut self) {
        let now = Utc::now();
        self.pending_ticks.push_back(now);

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::TickArrival,
            latency_us: 0,  // 起始点
            metadata: String::new(),
        });

        self.stage_start = now;
    }

    /// 记录数据处理完成
    pub fn record_data_processed(&mut self) {
        let now = Utc::now();
        let latency = (now - self.stage_start).num_microseconds().unwrap_or(0) as u64;

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::DataProcessing,
            latency_us: latency,
            metadata: String::new(),
        });

        self.stage_start = now;
    }

    /// 记录指标计算完成
    pub fn record_indicator_calculated(&mut self) {
        let now = Utc::now();
        let latency = (now - self.stage_start).num_microseconds().unwrap_or(0) as u64;

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::IndicatorCalc,
            latency_us: latency,
            metadata: String::new(),
        });

        self.stage_start = now;
    }

    /// 记录信号生成完成
    pub fn record_signal_generated(&mut self) {
        let now = Utc::now();
        let latency = (now - self.stage_start).num_microseconds().unwrap_or(0) as u64;

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::SignalGen,
            latency_us: latency,
            metadata: String::new(),
        });

        self.stage_start = now;
    }

    /// 记录决策完成（计算 Tick-to-Decision）
    pub fn record_decision_made(&mut self) -> u64 {
        let now = Utc::now();

        // 计算 Tick-to-Decision 延迟
        let tick_to_decision = if let Some(tick_time) = self.pending_ticks.pop_front() {
            (now - tick_time).num_microseconds().unwrap_or(0) as u64
        } else {
            0
        };

        self.pending_decisions.push_back(now);

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::DecisionMade,
            latency_us: tick_to_decision,
            metadata: format!("Tick-to-Decision: {} μs", tick_to_decision),
        });

        self.stage_start = now;
        tick_to_decision
    }

    /// 记录订单提交（计算 Decision-to-Order）
    pub fn record_order_submitted(&mut self) -> u64 {
        let now = Utc::now();
        self.order_count += 1;

        // 计算 Decision-to-Order 延迟
        let decision_to_order = if let Some(decision_time) = self.pending_decisions.pop_front() {
            (now - decision_time).num_microseconds().unwrap_or(0) as u64
        } else {
            0
        };

        self.pending_orders.push_back(now);

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::OrderSubmitted,
            latency_us: decision_to_order,
            metadata: format!("Decision-to-Order: {} μs", decision_to_order),
        });

        decision_to_order
    }

    /// 记录订单确认（计算端到端延迟）
    pub fn record_order_ack(&mut self) -> u64 {
        let now = Utc::now();

        // 计算端到端延迟（从第一个 tick 到订单确认）
        let e2e_latency = if let Some(tick_time) = self.pending_ticks.front() {
            (now - *tick_time).num_microseconds().unwrap_or(0) as u64
        } else {
            0
        };

        // 不弹出 tick，因为一个 tick 可能产生多个订单
        self.pending_orders.pop_front();

        self.records.push_back(LatencyRecord {
            timestamp: now,
            stage: LatencyStage::OrderAck,
            latency_us: e2e_latency,
            metadata: format!("E2E: {} μs", e2e_latency),
        });

        e2e_latency
    }

    /// 生成性能报告
    pub fn generate_report(&self) -> PerformanceReport {
        let test_end = Utc::now();
        let test_duration_secs = (test_end - self.test_start).num_seconds() as f64;

        // 计算处理速率
        let total_ticks = self.pending_ticks.len() + self.records.iter()
            .filter(|r| r.stage == LatencyStage::TickArrival)
            .count();
        let ticks_per_second = if test_duration_secs > 0.0 {
            total_ticks as f64 / test_duration_secs
        } else {
            0.0
        };
        let orders_per_second = if test_duration_secs > 0.0 {
            self.order_count as f64 / test_duration_secs
        } else {
            0.0
        };

        // 计算各阶段延迟统计
        let latency_stats = self.calculate_latency_stats();

        // 计算端到端延迟
        let e2e_latency = self.calculate_stage_latency(LatencyStage::OrderAck);

        // 计算目标达成
        let tick_to_decision = latency_stats.iter()
            .find(|s| s.stage == LatencyStage::DecisionMade)
            .map(|s| s.avg_us)
            .unwrap_or(0) as f64;
        let decision_to_order = latency_stats.iter()
            .find(|s| s.stage == LatencyStage::OrderSubmitted)
            .map(|s| s.avg_us)
            .unwrap_or(0) as f64;

        let target_achievement = TargetAchievement {
            // 目标 < 100μs，实际 < 100μs 则 100% 达成
            tick_to_decision_target: if tick_to_decision < 100.0 {
                100.0
            } else {
                (100.0 / tick_to_decision * 100.0).min(100.0)
            },
            // 目标 < 50μs
            decision_to_order_target: if decision_to_order < 50.0 {
                100.0
            } else {
                (50.0 / decision_to_order * 100.0).min(100.0)
            },
            // 目标 < 10ms = 10000μs
            e2e_target: if e2e_latency.avg_us as f64 < 10000.0 {
                100.0
            } else {
                (10000.0 / e2e_latency.avg_us as f64 * 100.0).min(100.0)
            },
        };

        PerformanceReport {
            test_start: self.test_start,
            test_end,
            total_ticks,
            total_orders: self.order_count,
            ticks_per_second,
            orders_per_second,
            latency_stats,
            e2e_latency,
            target_achievement,
        }
    }

    /// 计算各阶段延迟统计
    fn calculate_latency_stats(&self) -> Vec<LatencyStats> {
        let mut stats = Vec::new();

        for stage in &[
            LatencyStage::DataProcessing,
            LatencyStage::IndicatorCalc,
            LatencyStage::SignalGen,
            LatencyStage::DecisionMade,
            LatencyStage::OrderSubmitted,
        ] {
            let latencies: Vec<u64> = self.records.iter()
                .filter(|r| r.stage == *stage && r.latency_us > 0)
                .map(|r| r.latency_us)
                .collect();

            if latencies.is_empty() {
                continue;
            }

            let mut sorted = latencies.clone();
            sorted.sort();

            let count = sorted.len();
            let avg = sorted.iter().sum::<u64>() / count as u64;
            let min = sorted[0];
            let max = sorted[count - 1];
            let p50 = sorted[count * 50 / 100];
            let p99 = sorted[count * 99 / 100];
            let p999 = sorted[count * 999 / 1000.min(count - 1)];

            stats.push(LatencyStats {
                stage: *stage,
                count,
                avg_us: avg,
                min_us: min,
                max_us: max,
                p50_us: p50,
                p99_us: p99,
                p999_us: p999,
            });
        }

        stats
    }

    /// 计算指定阶段的延迟统计
    fn calculate_stage_latency(&self, stage: LatencyStage) -> LatencyStats {
        let records: Vec<u64> = self.records.iter()
            .filter(|r| r.stage == stage && r.latency_us > 0)
            .map(|r| r.latency_us)
            .collect();

        if records.is_empty() {
            return LatencyStats {
                stage,
                count: 0,
                avg_us: 0,
                min_us: 0,
                max_us: 0,
                p50_us: 0,
                p99_us: 0,
                p999_us: 0,
            };
        }

        let mut sorted = records.clone();
        sorted.sort();

        let count = sorted.len();
        let avg = sorted.iter().sum::<u64>() / count as u64;

        LatencyStats {
            stage,
            count,
            avg_us: avg,
            min_us: sorted[0],
            max_us: sorted[count - 1],
            p50_us: sorted[count * 50 / 100],
            p99_us: sorted[count * 99 / 100],
            p999_us: sorted[count * 999 / 1000.min(count - 1)],
        }
    }

    /// 获取原始记录
    pub fn get_records(&self) -> Vec<LatencyRecord> {
        self.records.iter().cloned().collect()
    }
}

impl Default for PerformanceBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_tracking() {
        let mut bench = PerformanceBenchmark::new();

        // 模拟一个完整的交易流程
        bench.record_tick_arrival();
        std::thread::sleep(std::time::Duration::from_millis(1));

        bench.record_data_processed();
        std::thread::sleep(std::time::Duration::from_millis(1));

        bench.record_indicator_calculated();
        std::thread::sleep(std::time::Duration::from_millis(1));

        let tick_to_decision = bench.record_decision_made();
        assert!(tick_to_decision > 0);

        let decision_to_order = bench.record_order_submitted();
        assert!(decision_to_order > 0);

        let _ = bench.record_order_ack();

        let report = bench.generate_report();
        assert!(report.total_ticks > 0);
        assert!(report.total_orders > 0);
    }

    #[test]
    fn test_target_achievement() {
        let mut bench = PerformanceBenchmark::new();

        // 模拟低延迟场景
        bench.record_tick_arrival();
        let _ = bench.record_decision_made();
        let _ = bench.record_order_submitted();

        let report = bench.generate_report();

        // 由于实际延迟很小，目标达成率应该很高
        assert!(report.target_achievement.tick_to_decision_target > 0.0);
    }
}