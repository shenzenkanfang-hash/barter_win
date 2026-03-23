//! 检查链入口
//!
//! 提供统一的检查链执行入口
//! 从 SignalProcessor 获取指标数据，执行各检查

use crate::h_15m::check::{a_exit, b_close, d_add, e_open};
use crate::h_15m::check::trigger::TriggerEvent;
use crate::types::MinSignalInput;

/// 检查信号枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSignal {
    Exit,   // 退出信号
    Close,  // 关仓信号
    Add,    // 加仓信号
    Open,   // 开仓信号
}

/// 检查链结果
#[derive(Debug, Clone, Default)]
pub struct CheckChainResult {
    pub signals: Vec<CheckSignal>,
}

impl CheckChainResult {
    pub fn new() -> Self {
        Self { signals: Vec::new() }
    }

    /// 添加信号
    pub fn add_signal(&mut self, signal: CheckSignal) {
        self.signals.push(signal);
    }

    /// 检查是否有特定信号
    pub fn has(&self, signal: CheckSignal) -> bool {
        self.signals.contains(&signal)
    }

    /// 是否有任何信号
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

/// 执行完整检查链（接收指标输入）
pub fn run_check_chain(symbol: &str, input: &MinSignalInput) -> Option<TriggerEvent> {
    // 各检查函数接收 MinSignalInput
    let exit_result = a_exit::check(input);
    let close_result = b_close::check(input);
    let add_result = d_add::check(input);
    let open_result = e_open::check(input);

    // 汇总信号
    let mut signals = Vec::new();
    if exit_result { signals.push(CheckSignal::Exit); }
    if close_result { signals.push(CheckSignal::Close); }
    if add_result { signals.push(CheckSignal::Add); }
    if open_result { signals.push(CheckSignal::Open); }

    if signals.is_empty() {
        return None;
    }

    // 返回第一个信号作为触发事件
    signals.first().map(|signal| {
        TriggerEvent::new(symbol.to_uppercase(), *signal)
    })
}
