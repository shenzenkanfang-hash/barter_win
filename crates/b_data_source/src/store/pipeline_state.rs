//! PipelineState - 全流程观测表
//!
//! 记录数据从 K线到达 到 订单成交 的完整链路状态
//! 用于时序一致性检查和故障定位

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

// =============================================================================
// 流水线阶段枚举
// =============================================================================

/// 流水线阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineStage {
    /// K线已到达（数据生产者）
    DataReceived,
    /// K线已写入 Store
    DataWritten,
    /// 指标已计算完成
    IndicatorComputed,
    /// 信号已生成
    SignalGenerated,
    /// 策略已决策
    DecisionMade,
    /// 风控已检查
    RiskChecked,
    /// 订单已提交
    OrderSubmitted,
    /// 订单已成交
    OrderFilled,
    /// 持仓已更新
    PositionUpdated,
    /// 错误发生
    ErrorOccurred,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStage::DataReceived => write!(f, "DataReceived"),
            PipelineStage::DataWritten => write!(f, "DataWritten"),
            PipelineStage::IndicatorComputed => write!(f, "IndicatorComputed"),
            PipelineStage::SignalGenerated => write!(f, "SignalGenerated"),
            PipelineStage::DecisionMade => write!(f, "DecisionMade"),
            PipelineStage::RiskChecked => write!(f, "RiskChecked"),
            PipelineStage::OrderSubmitted => write!(f, "OrderSubmitted"),
            PipelineStage::OrderFilled => write!(f, "OrderFilled"),
            PipelineStage::PositionUpdated => write!(f, "PositionUpdated"),
            PipelineStage::ErrorOccurred => write!(f, "ErrorOccurred"),
        }
    }
}

// =============================================================================
// 流水线事件
// =============================================================================

/// 流水线事件日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEvent {
    /// 追踪ID（关联同一数据的全链路事件）
    pub trace_id: u64,
    /// 事件时间戳（毫秒）
    pub timestamp_ms: i64,
    /// 阶段
    pub stage: PipelineStage,
    /// 输入数据哈希（用于回溯）
    pub input_hash: u64,
    /// 输出数据哈希
    pub output_hash: u64,
    /// 本阶段耗时（毫秒），从上一阶段到本阶段的延迟
    pub duration_ms: u64,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl PipelineEvent {
    /// 创建新事件（trace_id 由 PipelineState 自动填充）
    pub fn new(stage: PipelineStage, timestamp_ms: i64, trace_id: u64) -> Self {
        Self {
            trace_id,
            timestamp_ms,
            stage,
            input_hash: 0,
            output_hash: 0,
            duration_ms: 0,
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// 设置耗时
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = ms;
        self
    }
}

// =============================================================================
// 版本号（用于时序一致性）
// =============================================================================

/// 版本号追踪器
pub struct VersionTracker {
    /// K线数据版本
    data_version: AtomicU64,
    /// 指标版本
    indicator_version: AtomicU64,
    /// 信号版本
    signal_version: AtomicU64,
    /// 决策版本
    decision_version: AtomicU64,
}

impl Default for VersionTracker {
    fn default() -> Self {
        Self {
            data_version: AtomicU64::new(0),
            indicator_version: AtomicU64::new(0),
            signal_version: AtomicU64::new(0),
            decision_version: AtomicU64::new(0),
        }
    }
}

impl std::fmt::Debug for VersionTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VersionTracker")
            .field("data_version", &self.data_version.load(Ordering::SeqCst))
            .field("indicator_version", &self.indicator_version.load(Ordering::SeqCst))
            .field("signal_version", &self.signal_version.load(Ordering::SeqCst))
            .field("decision_version", &self.decision_version.load(Ordering::SeqCst))
            .finish()
    }
}

impl VersionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 增加数据版本
    pub fn incr_data_version(&self) -> u64 {
        self.data_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 增加指标版本
    pub fn incr_indicator_version(&self) -> u64 {
        self.indicator_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 增加信号版本
    pub fn incr_signal_version(&self) -> u64 {
        self.signal_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 增加决策版本
    pub fn incr_decision_version(&self) -> u64 {
        self.decision_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 获取当前版本快照
    pub fn snapshot(&self) -> VersionSnapshot {
        VersionSnapshot {
            data_version: self.data_version.load(Ordering::SeqCst),
            indicator_version: self.indicator_version.load(Ordering::SeqCst),
            signal_version: self.signal_version.load(Ordering::SeqCst),
            decision_version: self.decision_version.load(Ordering::SeqCst),
        }
    }
}

/// 版本快照（用于一致性检查）
#[derive(Debug, Clone, Copy, Default)]
pub struct VersionSnapshot {
    pub data_version: u64,
    pub indicator_version: u64,
    pub signal_version: u64,
    pub decision_version: u64,
}

impl VersionSnapshot {
    /// 检查指标是否基于最新 K线数据
    pub fn indicators_current(&self) -> bool {
        self.indicator_version >= self.data_version
    }

    /// 检查信号是否基于最新指标
    pub fn signals_current(&self) -> bool {
        self.signal_version >= self.indicator_version
    }

    /// 检查决策是否基于最新信号
    pub fn decisions_current(&self) -> bool {
        self.decision_version >= self.signal_version
    }

    /// 检查完整链路是否一致
    pub fn is_consistent(&self) -> bool {
        self.indicators_current() && self.signals_current() && self.decisions_current()
    }
}

/// PipelineState 只读快照（用于跨线程/异步检查点传递）
///
/// 与 VersionSnapshot 不同，这里包含完整状态用于诊断输出。
#[derive(Debug, Clone)]
pub struct PipelineStateSnapshot {
    pub symbol: String,
    pub last_update_ms: i64,
    pub versions: VersionSnapshot,
    pub stage_timestamps: HashMap<PipelineStage, i64>,
    pub recent_events: Vec<PipelineEvent>,
}

// =============================================================================
// 流水线状态
// =============================================================================

/// TraceId 生成器（线程安全）
static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(1);

/// 生成新的 TraceId
fn next_trace_id() -> u64 {
    NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed)
}

/// 品种流水线状态
///
/// 注意：不实现 Clone，因为 VersionTracker 包含 AtomicU64（非 Clone）。
/// 对外通过 Arc<RwLock<PipelineState>> 提供共享访问。
pub struct PipelineState {
    /// 品种
    pub symbol: String,
    /// 最后更新时间戳
    pub last_update_ms: i64,
    /// 版本追踪器
    pub versions: VersionTracker,
    /// 各阶段最后执行时间
    pub stage_timestamps: HashMap<PipelineStage, i64>,
    /// 事件日志（最近 N 条）
    recent_events: Vec<PipelineEvent>,
    /// 最大日志条目数
    max_log_size: usize,
}

impl PipelineState {
    /// 创建新品种的流水线状态
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            last_update_ms: 0,
            versions: VersionTracker::new(),
            stage_timestamps: HashMap::new(),
            recent_events: Vec::new(),
            max_log_size: 1000,
        }
    }

    /// 记录阶段完成（自动生成 trace_id，保持向后兼容）
    pub fn record_stage(&mut self, stage: PipelineStage, timestamp_ms: i64) {
        let trace_id = next_trace_id();
        self.record_with_trace(stage, timestamp_ms, trace_id);
    }

    /// 记录阶段完成（带 trace_id，自动计算延迟并输出结构化日志）
    ///
    /// v4.1: 新增 - 支持跨组件 TraceId 关联 + 微秒延迟追踪
    ///
    /// `duration_ms` 从上一阶段的 `stage_timestamps` 自动计算。
    /// 记录完成后自动输出 `tracing::info!` 结构化日志，格式：
    ///   trace_id=X stage=Stage symbol=Y latency_ms=N
    pub fn record_with_trace(
        &mut self,
        stage: PipelineStage,
        timestamp_ms: i64,
        trace_id: u64,
    ) {
        // 1. 计算延迟（从上一阶段的最后时间戳）
        let duration_ms = self
            .last_update_ms
            .checked_sub(timestamp_ms)
            .map(|d| d.unsigned_abs())
            .unwrap_or(0);

        self.last_update_ms = timestamp_ms;
        self.stage_timestamps.insert(stage, timestamp_ms);

        // 2. 增加对应版本号
        match stage {
            PipelineStage::DataReceived | PipelineStage::DataWritten => {
                self.versions.incr_data_version();
            }
            PipelineStage::IndicatorComputed => {
                self.versions.incr_indicator_version();
            }
            PipelineStage::SignalGenerated => {
                self.versions.incr_signal_version();
            }
            PipelineStage::DecisionMade
            | PipelineStage::RiskChecked
            | PipelineStage::OrderSubmitted
            | PipelineStage::OrderFilled
            | PipelineStage::PositionUpdated => {
                self.versions.incr_decision_version();
            }
            PipelineStage::ErrorOccurred => {
                // 错误不增加版本
            }
        }

        // 3. 构建事件
        let event = PipelineEvent::new(stage, timestamp_ms, trace_id);
        let duration_for_log = duration_ms;

        // 4. 添加事件日志
        self.recent_events.push(event);

        // 5. 限制日志大小
        if self.recent_events.len() > self.max_log_size {
            self.recent_events.remove(0);
        }

        // 6. 输出结构化调试日志（AI 可解析）
        tracing::info!(
            target: "pipeline_debug",
            trace_id = trace_id,
            stage = ?stage,
            symbol = %self.symbol,
            latency_ms = duration_for_log,
            version = ?self.versions.snapshot(),
            "[Pipeline] stage completed"
        );
    }

    /// 获取版本快照
    pub fn version_snapshot(&self) -> VersionSnapshot {
        self.versions.snapshot()
    }

    /// 导出只读快照（用于诊断检查点）
    pub fn to_snapshot(&self) -> PipelineStateSnapshot {
        PipelineStateSnapshot {
            symbol: self.symbol.clone(),
            last_update_ms: self.last_update_ms,
            versions: self.versions.snapshot(),
            stage_timestamps: self.stage_timestamps.clone(),
            recent_events: self.recent_events.clone(),
        }
    }

    /// 获取指定阶段的时间戳
    pub fn stage_time(&self, stage: PipelineStage) -> Option<i64> {
        self.stage_timestamps.get(&stage).copied()
    }

    /// 检查指标是否过期（超过指定毫秒未更新）
    pub fn indicator_stale(&self, max_age_ms: i64, now_ms: i64) -> bool {
        match self.stage_time(PipelineStage::IndicatorComputed) {
            Some(ts) => now_ms - ts > max_age_ms,
            None => true,
        }
    }
}

impl Default for PipelineState {
    fn default() -> Self {
        Self::new("UNKNOWN")
    }
}

impl std::fmt::Debug for PipelineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineState")
            .field("symbol", &self.symbol)
            .field("last_update_ms", &self.last_update_ms)
            .field("versions", &self.versions)
            .field("stage_timestamps", &self.stage_timestamps)
            .field("recent_events", &self.recent_events)
            .field("max_log_size", &self.max_log_size)
            .finish()
    }
}

// =============================================================================
// PipelineStore - 流水线状态管理器
// =============================================================================

/// 流水线状态管理器（按品种索引）
pub struct PipelineStore {
    /// 品种 -> 流水线状态（每个状态独立 Arc<RwLock> 支持并发访问）
    states: RwLock<HashMap<String, Arc<RwLock<PipelineState>>>>,
}

impl PipelineStore {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }

    /// 获取品种流水线状态的 Arc 引用（不存在则创建）
    ///
    /// 返回 Arc 以便调用方持有长期引用，同时内部 HashMap 保留所有权。
    pub fn get_or_create(&self, symbol: &str) -> Arc<RwLock<PipelineState>> {
        let mut states = self.states.write();
        states
            .entry(symbol.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(PipelineState::new(symbol))))
            .clone()
    }

    /// 记录阶段（线程安全，自动生成 trace_id，保持向后兼容）
    pub fn record(&self, symbol: &str, stage: PipelineStage, timestamp_ms: i64) {
        let state = self.get_or_create(symbol);
        state.write().record_stage(stage, timestamp_ms);
    }

    /// 记录阶段（线程安全，带 trace_id，关联跨组件事件）
    ///
    /// v4.1: 新增 - 用于手动指定 trace_id 实现全链路关联。
    /// 调用方应从数据源头（如 Kline1mStream）生成 trace_id，
    /// 沿 pipeline 向下传递。
    pub fn record_with_trace(
        &self,
        symbol: &str,
        stage: PipelineStage,
        timestamp_ms: i64,
        trace_id: u64,
    ) {
        let state = self.get_or_create(symbol);
        state.write().record_with_trace(stage, timestamp_ms, trace_id);
    }

    /// 获取品种流水线状态的只读快照
    pub fn get(&self, symbol: &str) -> Option<PipelineStateSnapshot> {
        self.states
            .read()
            .get(symbol)
            .map(|arc| arc.read().to_snapshot())
    }

    /// 获取版本快照
    pub fn version_snapshot(&self, symbol: &str) -> Option<VersionSnapshot> {
        self.states.read().get(symbol).map(|arc| arc.read().version_snapshot())
    }

    /// 检查指标是否过期
    pub fn indicator_stale(&self, symbol: &str, max_age_ms: i64, now_ms: i64) -> bool {
        self.states
            .read()
            .get(symbol)
            .map(|arc| arc.read().indicator_stale(max_age_ms, now_ms))
            .unwrap_or(true)
    }

    /// 获取所有品种
    pub fn symbols(&self) -> Vec<String> {
        self.states.read().keys().cloned().collect()
    }
}

impl Default for PipelineStore {
    fn default() -> Self {
        Self::new()
    }
}
