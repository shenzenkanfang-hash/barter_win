================================================================
架构文档归档目录 (2026-03-25)
================================================================

## 归档说明

本目录包含 barter-rs 量化交易系统 V4.0 (x_data 架构重构) 之前的
所有架构文档、重构设计文档、旧计划文档。

## 归档文件清单 (共30个)

### docs/ 目录归档 (16个)
| 文件名 | 原位置 | 说明 |
|--------|--------|------|
| architecture_v3.0_20260325.md | docs/architecture.md | V3.0 架构文档 |
| x_data-layer-design_20260325.md | docs/superpowers/specs/ | x_data 设计规格 |
| trading_business_flow_v1.4_20260325.md | docs/trading_business_flow.md | V1.4 业务流程 |
| 全项目架构审计报告_2026-03-24.md | docs/ | 架构审计报告 |
| 架构终审合规报告_2026-03-24_V3.md | docs/ | V3.0 合规报告 |
| architecture-module-analysis.md | docs/architecture/ | 模块分析 |
| f_engine_state_design.md | docs/architecture/ | 引擎状态设计 |
| f_engine_engine_state_design.md | docs/architecture/ | 引擎状态设计 |
| 全项目架构优化方案_2026-03-24.md | docs/ | 优化方案 |
| P3_长期演进方案评估.md | docs/architecture/ | 长期演进评估 |
| 架构全面深度检查报告.md | docs/architecture/ | 深度检查 |
| 架构最终合规报告_2026-03-24.md | docs/architecture/ | 合规报告 |
| 架构终审合规报告_2026-03-24_V2.md | docs/architecture/ | V2 合规报告 |
| 豆包评审.md | docs/ | 外部评审 |

### .planning/codebase/ 目录归档 (14个)
| 文件名 | 说明 |
|--------|------|
| ARCHITECTURE.md | 架构分析 |
| ARCHITECTURE_CN.md | 架构分析(中文) |
| CONCERNS.md | 问题追踪 |
| CONCERNS_CN.md | 问题追踪(中文) |
| CONVENTIONS.md | 代码规范 |
| FIX_PLAN.md | 修复计划 |
| FIX_PLAN_CN.md | 修复计划(中文) |
| INTEGRATIONS.md | 集成说明 |
| REMAINING_FIX_PLAN.md | 剩余修复计划 |
| STACK.md | 技术栈 |
| STRUCTURE.md | 项目结构 |
| TESTING.md | 测试规范 |

## 当前版本

| 文件 | 版本 | 说明 |
|------|------|------|
| `docs/architecture.md` | V4.0 | **唯一权威架构文档** |

## V4.0 主要变化

- 新增 x_data 业务数据抽象层
- StateManager trait 统一状态管理
- UnifiedStateView 完整系统快照
- 21 个数据类型统一迁移
- ARCH-001, ARCH-002, ARCH-003 全部修复

================================================================
归档时间: 2026-03-25
================================================================
