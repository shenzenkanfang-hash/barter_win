================================================================================
v1.1 Phase Context - MockBinanceGateway + Signal Synthesis Layer
================================================================================

## Phase Overview
v1.1 新增功能开发阶段，包含四个主要方向：
- A: MockBinanceGateway - 模拟币安网关
- B: Signal Synthesis Layer - 通道退出逻辑
- C: Complete Test Coverage
- D: Indicator Comparison

## Reference Documents
- docs/mock-binance-gateway-design.md - MockBinanceGateway 设计文档
- docs/system-architecture.md - 系统架构文档
- docs/indicator-logic.md - 指标逻辑文档

## Design Decisions

### A. MockBinanceGateway
1. 模拟账户/持仓/订单/保证金，与币安风控逻辑完全一致
2. 立即成交机制（Market Order）
3. CSV 输出：trades.csv, positions.csv, risk_log.csv, account_snapshot.csv, indicator_comparison.csv

### B. Signal Synthesis Layer
1. 通道切换逻辑：
   - 进入高速：15min>=13% 或 1min>=3% → 马丁策略(分钟级)
   - 退出高速：tr_ratio < 1 → 回到慢速通道
2. 日线趋势平仓：ma5_close位置 + Pine颜色
3. 输出 trigger_log.csv

### C. Test Coverage
完整的单元测试覆盖

### D. Indicator Comparison
Rust vs Python 指标对比输出

## Dependencies
- Phase 7 已完成所有模块
- Phase 6 TradingEngine 已完成
- Phase A.5 线程安全已完成

## Constraints
- 开发阶段禁止编译（由测试工程师执行 verify）
- 功能优先
