# Milestones

## v0.1: Foundation

**Goal**: Project scaffold and core infrastructure

- Workspace structure
- Error type definitions
- Core data structures (Order, Position, Fund)
- Logging setup

**Status**: COMPLETE

---

## v0.2: Market Data Layer

**Goal**: WebSocket connection and K-line synthesis

- Exchange WebSocket connector (trait)
- K-line incremental synthesis
- Market data trait abstraction
- MockMarketStream for testing

**Status**: COMPLETE

---

## v0.3: Indicator Layer

**Goal**: Core indicators with O(1) incremental calculation

- EMA incremental calculation
- Pine color (MACD + EMA + RSI)
- TR and price position
- RSI relative strength index

**Status**: COMPLETE

---

## v0.4: Strategy Layer

**Goal**: Strategy trait and three strategy types

- Strategy trait definition
- Signal, TradingMode, OrderRequest types
- Order side abstraction

**Status**: COMPLETE

---

## v0.5: Engine Layer

**Goal**: Core engine with risk check and order execution

- Engine core (TradingEngine)
- Risk pre-check (lock-free)
- Order execution with global lock
- Position management (types conversion)
- ModeSwitcher for volatility detection

**Status**: COMPLETE

---

## v0.6: Integration

**Goal**: Full trading flow integration

- main.rs entry point
- Component wiring
- Mock data flow
- End-to-end compilation

**Status**: COMPLETE (代码实现完成,待编译验证)

---

## v0.7: Pipeline Architecture

**Goal**: Species-level pipeline parallel architecture

- Check table (CheckTable)
- Round guard (one-round encoding)
- PipelineForm (full flow form)
- SymbolRules (trading pair rules)
- VolatilityChannel (slow/fast channel)
- Position mutex check

**Status**: COMPLETE

---

## v0.8: Risk Control Enhancement

**Goal**: Three-layer risk architecture

- AccountPool: Account margin pool with circuit breaker
- StrategyPool: Strategy margin pool with rebalancing
- OrderCheck: Order risk checker with Lua script
- PnlManager: Profit/loss management
- RiskReChecker: Lock-in risk re-check

**Status**: COMPLETE

---

## v0.9: Strategy Enhancement

**Goal**: Strategy state machine

- TrendStrategy: Trend strategy state machine
- PinStrategy: Martin/pin strategy state machine
- ZScore indicator framework
- TRRatio indicator framework
- MarketStatusDetector: Market status detection

**Status**: COMPLETE

---

## v0.10: Persistence & Indicators

**Goal**: Persistence service and advanced indicators

- PersistenceService: Trade record, position snapshot
- AccountPool: Account margin pool with circuit breaker
- StrategyPool: Strategy fund pool with rebalancing
- VelocityPercentile: Velocity percentile indicator
- PriceDeviation: Price deviation indicator
- Momentum: Momentum indicator
- BigCycleCalculator: Daily cycle indicators (TR Ratio, position, PineColor)

**Status**: COMPLETE

---

## v1.0: Integration & Testing

**Goal**: Full system integration and compilation verification

- Compile and verify all modules
- Adjust indicator calculations based on Python code
- Integration testing with mock data

**Status**: ✅ COMPLETE (2026-03-20)

**Delivered:**
- All modules implemented and compilation verified
- cargo check --all passes
- Git tag v1.0.0 created

---

## v1.1: MockBinanceGateway + Signal Synthesis Layer

**Goal**: 实现模拟币安网关和信号综合层

**Deliverables:**
- MockBinanceGateway: 模拟账户/持仓/订单/保证金，与币安风控一致
- Signal Synthesis Layer: 通道退出逻辑 (tr_ratio < 1, ma5_close + PineColor)
- SQLite 持久化 (SqliteRecordService: 6张表)
- CSV 输出 (IndicatorCsvWriter)
- Complete test coverage for all modules

**Status**: ✅ COMPLETE (2026-03-21)

**Phase Directory:** `.planning/phases/08-v1.1-mock-binance/`

---

## v1.2: Market Data Layer Enhancement

**Goal**: 市场数据层增强 - 数据分发、持久化、订单簿

**Deliverables:**
- DataFeeder: 核心数据分发器，统一分发 KLine/Tick/OrderBook 数据
- KlinePersistence: K线 Redis 持久化，支持断线恢复
- SymbolRegistry: 品种注册中心，统一管理交易对信息
- VolatilityDetector: 波动率检测器，实时计算波动率指标
- OrderBook: 订单簿模块，支持 L1/L2 行情
- WebSocket 重连指数退避机制
- Pine v5 完整指标模块
- Complete unit tests

**Status**: ✅ COMPLETE (2026-03-21)

**Phase Directory:** `.planning/phases/09-v1.2-market-data/`

---

## v1.3: Indicator Module Cleanup

**Goal**: 简化指标库，删除零散指标，保留核心模块

**Deliverables:**
- 保留 `pine_indicator_full.rs` - Pine v5 完整指标 (PineColorDetector, EMA, RMA, DominantCycleRSI)
- 保留 `min_cycle.rs` - 大周期指标 (BigCycleCalculator)
- 新增 `indicator_1m.rs` - 1分钟指标（非 Pine 逻辑）
- 删除零散指标: ema.rs, rsi.rs, pine_color.rs, price_position.rs, tr_ratio.rs, velocity.rs, z_score.rs, big_cycle.rs, error.rs

**Status**: ✅ COMPLETE (2026-03-21)

**Commit**: `73d8dd0` - indicator 模块重构完成

---

## v1.4: Indicator System Enhancement

**Goal**: 完善指标系统，100% 对齐 Python v2.6

**Deliverables:**
- 重构 `indicator_1m.rs` - 100% 对齐 Python v2.6
  - 百分位计算修正 (percentileofscore kind="weak")
  - 滚动窗口工具 (RollingMean, RollingStd, RollingPercentile, RollingMax, RollingMin)
  - TR 指标完善 (tr_ratio, tr_ratio_zscore)
  - Z-Score 计算 (zscore_1h_1m, zscore_14_1m)
  - 高阶动能指标 (Jerk, Norm jerk, Market force, Acc efficiency)

**Status**: ✅ COMPLETE (2026-03-21)

**Commit**: `d59a611` - 重构 indicator_1m.rs - 100% 对齐 Python v2.6

**待继续**: 指标系统持续完善中

---

## v1.5: Python-Rust 功能对齐

**Goal**: 根据Python-Rust功能验证结果，实现核心缺失功能

**Deliverables:**
- TrendStatusDetector: Pine颜色分组校验 ✅ (signal_generator.rs)
- LocalPositionManager: 统一接口 + 品种锁 ✅ (已确认 RwLock 保护)
- sync_account/position_data: 交易所状态同步 ✅ (symbol_rules_fetcher.rs)
- TR比率排名: 20日百分比排名 🔄 进行中
- SymbolRuleParser: 实时规则同步 ✅ (symbol_rules_fetcher.rs)

**Status**: IN PROGRESS (2026-03-21)
- ✅ fetch_account_info - 账户信息 API
- ✅ fetch_position_risk - 持仓风险 API
- 🔄 TR比率排名 - indicator 模块

**Phase Directory:** `.planning/phases/10-v1.5-py-rust-align/`
