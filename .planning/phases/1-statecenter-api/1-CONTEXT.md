# Phase 1: StateCenter API 标准化与完善 - Context

**Gathered:** 2026-03-30
**Status:** Ready for planning

<domain>
## Phase Boundary

将 `x_data/src/state/` 中的 `StateCenterTrait` API 命名与设计规格对齐，补全缺失方法，确保 trait 与设计规格 `docs/superpowers/specs/2026-03-30-event-driven-architecture-design.md` 第三节完全一致。
</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion
纯基础设施重构阶段，所有实现选择由 Claude 自行判断：
- 保持同步方法（`parking_lot::RwLock` 无需 async）
- 保留旧方法名作为 type alias 避免破坏性变更
- 新方法按设计规格签名实现
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/x_data/src/state/component.rs`: ComponentState + ComponentStatus（已完善，无需修改）
- `crates/x_data/src/state/mod.rs`: 模块导出结构（保持不变）
- `#[async_trait]` 用于 trait 定义

### Established Patterns
- Trait 使用 `#[async_trait::async_trait]` 宏
- 实现使用 `parking_lot::RwLock`（同步高性能锁）
- 测试在 `#[cfg(test)]` 模块中，使用 `super::*` 引用

### Integration Points
- 尚无生产代码调用 `StateCenterTrait`（trait 未被主程序集成）
- 未来 `EngineManager`（Phase 2）将依赖此 trait
</code_context>

<specifics>
## Specific Ideas

设计规格 API（第三节 3.3）：
- `report_alive(&self, component_id: &str) -> Result<(), StateCenterError>`
- `report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError>`
- `get(&self, component_id: &str) -> Option<ComponentState>`
- `get_all(&self) -> Vec<ComponentState>`
- `get_alive(&self, timeout_secs: i64) -> Vec<ComponentState>`
- `get_stale(&self, threshold_secs: i64) -> Vec<ComponentState>`
</specifics>

<deferred>
## Deferred Ideas

无 — 阶段范围清晰
</deferred>
