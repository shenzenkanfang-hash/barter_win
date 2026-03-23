//! 检查链入口
//!
//! 提供统一的检查链执行入口

use crate::h_1m::检查::{a_exit, b_close, c_risk, d_add, e_open};

/// 检查信号枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSignal {
    Exit,   // 退出信号
    Close,  // 关仓信号
    Risk,   // 风控信号
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
}

/// 执行完整检查链
pub fn run_check_chain() -> CheckChainResult {
    let mut result = CheckChainResult::new();

    // a_exit 检查
    if a_exit::check() {
        result.add_signal(CheckSignal::Exit);
    }

    // b_close 检查
    if b_close::check() {
        result.add_signal(CheckSignal::Close);
    }

    // c_risk 检查
    if c_risk::check() {
        result.add_signal(CheckSignal::Risk);
    }

    // d_add 检查
    if d_add::check() {
        result.add_signal(CheckSignal::Add);
    }

    // e_open 检查
    if e_open::check() {
        result.add_signal(CheckSignal::Open);
    }

    result
}
