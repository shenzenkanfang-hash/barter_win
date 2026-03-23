//! 关仓检查
//!
//! 检查逻辑：检测是否应该关仓（与退出不同，关仓是更激进的平仓）
//!
//! 注意：当前实现为占位符，实际关仓逻辑由引擎层根据风控判定

use crate::types::MinSignalInput;

/// 主检查入口
pub fn check(_input: &MinSignalInput) -> bool {
    // TODO: 实现关仓检查逻辑
    // 关仓条件可能包括：
    // - 极端风控信号
    // - 仓位超过上限
    // - 连续亏损
    false
}
