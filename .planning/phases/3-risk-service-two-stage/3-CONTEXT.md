# Phase 3: 风控服务两阶段抽取 - Context

**Gathered:** 2026-03-30
**Status:** Ready for execution

<domain>
## Phase Boundary

将 `RiskService` 和 `RiskReChecker` 的 `re_check()` 方法重命名为 `final_check()`，对齐设计规格第五节两阶段风控命名。同时补全 backward compatibility。
</domain>

<decisions>
## Implementation Decisions

### API Naming Alignment
- `re_check()` → `final_check()` — 对齐设计规格 Stage 2 命名
- `ReCheckRequest` → `FinalCheckRequest` — 保持 deprecated type alias 兼容
- `ReCheckResult` → `FinalCheckResult` — 保持 deprecated type alias 兼容
- `RiskReChecker::re_check()` → `final_check()` — 内部实现同步更新

### Compatibility Strategy
- 所有 deprecated 名称保留为 type alias 或 trait method
- 使用 `#[allow(deprecated)]` 避免链式调用告警
</decisions>

<reusable>
## Reusable Assets
- `crates/e_risk_monitor/src/risk_service.rs`: RiskService trait
- `crates/e_risk_monitor/src/risk/common/risk_rechecker.rs`: RiskReChecker
- `crates/e_risk_monitor/tests/test_trade_lock_risk_service.rs`: 集成测试
</reusable>
