//! 统一事件处理器 trait
//!
//! 提供统一的事件处理架构，基于 barter-rs 的 Processor/Auditor 模式。

use crate::ws::mock_ws::SimulatedTick;

/// 统一事件处理器
///
/// 任何需要处理事件的组件都可以实现此 trait
pub trait Processor<Event> {
    /// 审计输出类型
    type Audit;

    /// 处理事件
    fn process(&mut self, event: Event) -> Self::Audit;
}

/// Tick 处理器接口
pub trait TickProcessor: Processor<SimulatedTick> {}

impl<T: Processor<SimulatedTick>> TickProcessor for T {}
