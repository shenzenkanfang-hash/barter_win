//! 触发器模块
//!
//! 职责：根据检查链结果生成触发事件
//! 输出给 f_engine 由引擎层统一判定是否执行

use super::check_chain::CheckSignal;

/// 触发事件
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    pub symbol: String,
    pub signal: CheckSignal,
}

impl TriggerEvent {
    pub fn new(symbol: String, signal: CheckSignal) -> Self {
        Self { symbol, signal }
    }
}
