//! h_15m/status.rs
//!
//! PinStatus状态机 - 从pin_main.py (Python) 1:1移植
//!
//! 职责：管理分钟级Pin策略的仓位状态转换

#![forbid(unsafe_code)]

use crate::types::MarketStatus;

/// Pin状态枚举（从pin_main.py移植，Rust命名规范修正）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinStatus {
    /// 初始状态
    Initial,
    /// 进入对冲
    HedgeEnter,
    /// 仓位锁定
    PosLocked,
    /// 多头-初始
    LongInitial,
    /// 多头-第一次开仓
    LongFirstOpen,
    /// 多头-翻倍加仓
    LongDoubleAdd,
    /// 多头-日线开放
    LongDayAllow,
    /// 空头-初始
    ShortInitial,
    /// 空头-第一次开仓
    ShortFirstOpen,
    /// 空头-翻倍加仓
    ShortDoubleAdd,
    /// 空头-日线开放
    ShortDayAllow,
}

impl Default for PinStatus {
    fn default() -> Self {
        PinStatus::Initial
    }
}

impl PinStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PinStatus::Initial => "Initial",
            PinStatus::HedgeEnter => "HedgeEnter",
            PinStatus::PosLocked => "PosLocked",
            PinStatus::LongInitial => "LongInitial",
            PinStatus::LongFirstOpen => "LongFirstOpen",
            PinStatus::LongDoubleAdd => "LongDoubleAdd",
            PinStatus::LongDayAllow => "LongDayAllow",
            PinStatus::ShortInitial => "ShortInitial",
            PinStatus::ShortFirstOpen => "ShortFirstOpen",
            PinStatus::ShortDoubleAdd => "ShortDoubleAdd",
            PinStatus::ShortDayAllow => "ShortDayAllow",
        }
    }
}

/// Pin状态机
///
/// 管理Pin策略的仓位状态转换，参考pin_main.py的singleAssetTrader状态管理
pub struct PinStatusMachine {
    current_status: PinStatus,
}

impl PinStatusMachine {
    pub fn new() -> Self {
        Self {
            current_status: PinStatus::Initial,
        }
    }

    pub fn current_status(&self) -> PinStatus {
        self.current_status
    }

    pub fn set_status(&mut self, status: PinStatus) {
        self.current_status = status;
    }

    /// 判断是否可以开多
    pub fn can_long_open(&self) -> bool {
        matches!(
            self.current_status,
            PinStatus::Initial | PinStatus::LongInitial
        )
    }

    /// 判断是否可以开空
    pub fn can_short_open(&self) -> bool {
        matches!(
            self.current_status,
            PinStatus::Initial | PinStatus::ShortInitial
        )
    }

    /// 判断是否可以加多
    pub fn can_long_add(&self) -> bool {
        matches!(
            self.current_status,
            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd | PinStatus::HedgeEnter
        )
    }

    /// 判断是否可以加空
    pub fn can_short_add(&self) -> bool {
        matches!(
            self.current_status,
            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd | PinStatus::HedgeEnter
        )
    }

    /// 判断是否可以对冲（已有仓位反向开仓）
    pub fn can_hedge(&self) -> bool {
        matches!(self.current_status, PinStatus::LongFirstOpen | PinStatus::ShortFirstOpen)
    }

    /// 判断仓位是否锁定
    pub fn is_locked(&self) -> bool {
        matches!(self.current_status, PinStatus::PosLocked)
    }

    /// 判断是否在日线模式
    pub fn is_day_mode(&self) -> bool {
        matches!(
            self.current_status,
            PinStatus::LongDayAllow | PinStatus::ShortDayAllow
        )
    }

    /// 重置到初始状态
    pub fn reset(&mut self) {
        self.current_status = PinStatus::Initial;
    }

    /// 重置多头侧状态
    pub fn reset_long(&mut self) {
        match self.current_status {
            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd | PinStatus::LongInitial => {
                self.current_status = PinStatus::LongInitial;
            }
            PinStatus::LongDayAllow => {
                self.current_status = PinStatus::ShortDayAllow;
            }
            _ => {}
        }
    }

    /// 重置空头侧状态
    pub fn reset_short(&mut self) {
        match self.current_status {
            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd | PinStatus::ShortInitial => {
                self.current_status = PinStatus::ShortInitial;
            }
            PinStatus::ShortDayAllow => {
                self.current_status = PinStatus::LongDayAllow;
            }
            _ => {}
        }
    }
}

impl Default for PinStatusMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// 市场状态（与MarketStatus枚举对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinMarketStatus {
    Pin,
    Range,
    Trend,
    Invalid,
}

impl From<MarketStatus> for MinMarketStatus {
    fn from(status: MarketStatus) -> Self {
        match status {
            MarketStatus::PIN => MinMarketStatus::Pin,
            MarketStatus::RANGE => MinMarketStatus::Range,
            MarketStatus::TREND => MinMarketStatus::Trend,
            MarketStatus::INVALID => MinMarketStatus::Invalid,
        }
    }
}
