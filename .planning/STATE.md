# State

## Current Position

Milestone: v1.1 - MockBinanceGateway + SignalSynthesisLayer
Status: Phase 08 In Progress
Current: SQLite + CSV 模块实现完成，测试通过

## Completed Milestones

- v0.1-v0.10: All development phases complete
- v1.0: Integration & Testing - SHIPPED 2026-03-20
  - All modules implemented
  - Compilation verified (cargo check --all)
  - Git tag v1.0.0 created

## Completed

- Phase 1: Foundation - TradingError, Order, Position, FundPool
- Phase 2: Market Data - Tick, KLine, KLineSynthesizer, MarketConnector, MarketStream, MockMarketStream
- Phase 3: Indicator - EMA, RSI, PineColor, PricePosition
- Phase 4: Strategy - Strategy trait, Signal, TradingMode, OrderRequest
- Phase 5: Engine - RiskPreChecker, OrderExecutor, ModeSwitcher
- Phase 6: Integration
  - types.rs 类型转换模块 (Side, OrderType)
  - engine.rs TradingEngine 主引擎
  - websocket.rs MockMarketStream/MockMarketConnector
  - main.rs 程序入口 (10秒模拟运行)
- Phase 7: Enhancement (v0.9)
  - RiskReChecker: 风控锁内复核
  - PnlManager: 盈亏管理模块
  - MarketStatusDetector: 市场状态检测
  - PositionExclusionChecker: 仓位互斥检查
  - ThresholdConstants: 阈值常量集中管理
  - OrderCheck: 订单风控检查器
  - 日线指标支持 (channel.rs)
- Phase 8: v1.1 (进行中)
  - MockBinanceGateway: 模拟账户/持仓/订单/保证金/风控
  - SignalSynthesisLayer: 通道退出逻辑 (tr_ratio<1/日线趋势平仓)
  - 单元测试: 账户创建/持仓盈亏/通道切换/频率限制

## Blockers

(None)

## v1.1 完成进度 (更新: 2026-03-20)

### 已完成 ✓
- [x] SQLite 持久化模块 (sqlite_persistence.rs)
  - SqliteRecordService: 6张表
  - EventRecorder trait + NoOpEventRecorder + SqliteEventRecorder
- [x] CSV 输出 (IndicatorCsvWriter)
- [x] MockBinanceGateway 集成 EventRecorder
- [x] 编译通过 (cargo check --all)
- [x] 测试通过 (mock_binance_gateway 4/4)
- [x] symbol_rules/thresholds 测试修复 (13 tests pass)

### 待完成
- [x] 指标对比验证 (Rust vs Python)
  - indicator_compare.rs: 从币安获取1000根日线
  - BigCycleCalculator getter 方法
  - 输出 indicator_comparison_btcusdt.csv (1001行)

## Next Action

继续完成剩余测试用例 + 指标对比验证

## v0.10 Enhancement 完成

新增模块:
- AccountPool (account_pool.rs): 账户保证金池，熔断保护
  - CircuitBreakerState (Normal/Partial/Full)
  - AccountInfo, AccountPool
- PersistenceService (persistence.rs): 持久化服务
  - TradeRecord, PositionSnapshot, KLineCache
  - PersistenceService
- StrategyPool (strategy_pool.rs): 策略资金池，支持再平衡
  - StrategyAllocation, StrategyPool
- VelocityPercentile (velocity.rs): 速度百分位指标
  - VelocityPercentile, PriceDeviation, Momentum
- BigCycleCalculator (big_cycle.rs): 大周期指标计算器
  - TR Ratio (tr_ratio_5d_20d, tr_ratio_20d_60d)
  - 区间位置 (pos_norm_20, ma5_in_20d_ma5_pos, ma20_in_60d_ma20_pos)
  - PineColor (三种参数组合: 100/200, 20/50, 12/26)

## v0.9 Enhancement 完成

新增模块:
- RiskReChecker (risk_rechecker.rs): 风控锁内复核
- PnlManager (pnl_manager.rs): 盈亏管理模块
- MarketStatusDetector (market_status.rs): 市场状态检测
- PositionExclusionChecker (position_exclusion.rs): 仓位互斥检查
- ThresholdConstants (thresholds.rs): 阈值常量集中管理
- OrderCheck (order_check.rs): 订单风控检查器
- 日线指标 (channel.rs): kline_1d, ema_100/200, rsi_daily
- ZScore (z_score.rs): Z-Score 指标框架
- TRRatio (tr_ratio.rs): TR-Ratio 指标框架
- LocalPositionManager (position_manager.rs): 持仓管理器
- TrendStrategy (trend_strategy.rs): 趋势策略状态机
- PinStrategy (pin_strategy.rs): 马丁/插针策略状态机

待调整:
- 指标计算逻辑需根据 indicator_1m/indicator_calc.py 调整
- 指标计算逻辑需根据 indicator_1d/pine_scripts.py 调整

## v0.8 编译修复 (补充)

修复内容:
- KLineSynthesizer 添加 current_kline() accessor
- PricePosition::new(14) 正确参数
- 清理未使用导入 (chrono::Utc, OrderType, Side)
- symbol_rules.rs step_size() 循环实现

编译验证: cargo check 通过，无警告

## v0.8 问题修复

修复内容:
- PineColor 判断逻辑统一 - 按设计文档修正为先判断 RSI 极值
- RiskPreChecker 完善 - 添加品种注册、波动率模式检查
- 消除编译警告 - completed_1d, strategy_id, period, unused import
- account/src/error.rs 派生宏补全 - 添加 Clone, Eq, PartialEq
- 添加文档注释 - OrderExecutor, ModeSwitcher, KLineSynthesizer

## v0.7 Binance 实时数据连接

新增:
- binance_ws.rs: Binance WebSocket 连接器 (测试网)
- binance_test.rs: 实时数据测试程序
- tokio-tungstenite native-tls 特性支持 TLS 连接

验证:
- 成功从 Binance 测试网接收实时 Tick 数据
- BTCUSDT 价格: ~70485 USDT

## 设计决策记录

### v0.8 设计文档整合: 旧代码逻辑归档

新增文档: `docs/2026-03-20-trading-system-rust-design.md` 第十七章 (17.3.7-17.3.9)

整合内容:
- 17.3.7 风控引擎三层架构 (AccountPool/StrategyPool/OrderCheck)
  - AccountPool: 账户保证金池，Redis熔断保护
  - StrategyPool: 策略保证金池，分钟/小时级分配
  - OrderCheck: 订单风控检查器，Lua脚本原子预占
- 17.3.8 盈亏管理模块 (PnlManager)
  - 低波动/高波动品种互斥机制
  - rescue_low_volatility_symbols() 解救机制
  - 浮盈/实盈区分，累计盈利
- 17.3.9 交易对规则模块 (SymbolRules)
  - SymbolRules: 交易对规则数据模型
  - effective_min_qty: 实际有效最小开仓数量
  - calculate_open_qty(): 基于名义价值计算合规数量

设计决策:
- 品种规则需要专门的 SymbolRules 小模块处理
- 持仓管理简化: 只需记录价格/数量/多空，无需复杂状态机
- 全流程表单 PipelineForm 贯穿所有层级

### v0.7 架构升级: 流水线并行架构

新增文档: `docs/2026-03-20-trading-system-rust-design.md` 第十六章

核心设计:
- 品种级流水线并行 (每品种独立，互不阻塞)
- Check 表统一记录各层结果
- 双通道: 慢速(时间驱动) + 高速(波动率触发)
- 一轮编码机制确保计算一致性
- 策略 Rust 模块配置驱动
- 风控两层: 锁外预检 + 锁内复核
- 仓位互斥判断

待办:
- 实现流水线架构代码 ✅ 全部完成
  - ✅ Check 表 (CheckTable)
  - ✅ 一轮编码机制 (RoundGuard)
  - ✅ PipelineForm 全流程表单
  - ✅ SymbolRules 模块
  - ✅ VolatilityChannel (高速/慢速通道)
