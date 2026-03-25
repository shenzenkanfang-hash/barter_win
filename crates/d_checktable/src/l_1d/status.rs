//! l_1d/status.rs
//!
//! TrendStatus状态机 - 从trend_main.py (Python) 1:1移植
//!
//! 职责：管理日线级Trend策略的仓位状态转换

#![forbid(unsafe_code)]

/// Trend状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendStatus {
    /// 初始状态
    Initial,
    /// 多头入场
    LongEnter,
    /// 多头持仓
    LongHold,
    /// 空头入场
    ShortEnter,
    /// 空头持仓
    ShortHold,
    /// 多头日线对冲
    LongDayHedge,
    /// 空头日线对冲
    ShortDayHedge,
}

impl Default for TrendStatus {
    fn default() -> Self {
        TrendStatus::Initial
    }
}

impl TrendStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrendStatus::Initial => "Initial",
            TrendStatus::LongEnter => "LongEnter",
            TrendStatus::LongHold => "LongHold",
            TrendStatus::ShortEnter => "ShortEnter",
            TrendStatus::ShortHold => "ShortHold",
            TrendStatus::LongDayHedge => "LongDayHedge",
            TrendStatus::ShortDayHedge => "ShortDayHedge",
        }
    }
}

/// Trend状态机
///
/// 管理日线级趋势策略的仓位状态转换
pub struct TrendStatusMachine {
    current_status: TrendStatus,
}

impl TrendStatusMachine {
    pub fn new() -> Self {
        Self {
            current_status: TrendStatus::Initial,
        }
    }

    pub fn current_status(&self) -> TrendStatus {
        self.current_status
    }

    pub fn set_status(&mut self, status: TrendStatus) {
        self.current_status = status;
    }

    /// 判断是否有多头持仓
    pub fn has_long_position(&self) -> bool {
        matches!(
            self.current_status,
            TrendStatus::LongEnter | TrendStatus::LongHold | TrendStatus::LongDayHedge
        )
    }

    /// 判断是否有空头持仓
    pub fn has_short_position(&self) -> bool {
        matches!(
            self.current_status,
            TrendStatus::ShortEnter | TrendStatus::ShortHold | TrendStatus::ShortDayHedge
        )
    }

    /// 判断是否有任何持仓
    pub fn has_any_position(&self) -> bool {
        self.has_long_position() || self.has_short_position()
    }

    /// 判断是否可以开多
    pub fn can_long_open(&self) -> bool {
        matches!(self.current_status, TrendStatus::Initial)
    }

    /// 判断是否可以开空
    pub fn can_short_open(&self) -> bool {
        matches!(self.current_status, TrendStatus::Initial)
    }

    /// 重置到初始状态
    pub fn reset(&mut self) {
        self.current_status = TrendStatus::Initial;
    }
}

impl Default for TrendStatusMachine {
    fn default() -> Self {
        Self::new()
    }
}
