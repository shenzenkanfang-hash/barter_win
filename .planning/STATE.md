# State

## Current Position

Phase: 6 (Integration) - COMPLETE
Status: v0.8 Issue Fix Complete

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
- Workspace dependencies updated

## Blockers

(None)

## Next Action

(v0.8 完成 - 问题修复总结归档)

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

新增文档: `docs/2026-03-20-trading-system-rust-design.md` 第十七章 (17.3.7-17.3.8)

整合内容:
- 17.3.7 风控引擎三层架构 (AccountPool/StrategyPool/OrderCheck)
  - AccountPool: 账户保证金池，Redis熔断保护
  - StrategyPool: 策略保证金池，分钟/小时级分配
  - OrderCheck: 订单风控检查器，Lua脚本原子预占
- 17.3.8 盈亏管理模块 (PnlManager)
  - 低波动/高波动品种互斥机制
  - rescue_low_volatility_symbols() 解救机制
  - 浮盈/实盈区分，累计盈利

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
- 实现流水线架构代码
- 实现 Check 表
- 实现一轮编码机制
- 实现 SymbolRules 小模块
