# State

## Current Position

Phase: 6 (Integration) - COMPLETE
Status: v0.6 Core Integration Complete

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

(v0.7 进行中 - Binance 实时数据连接完成)

## v0.7 Binance 实时数据连接

新增:
- binance_ws.rs: Binance WebSocket 连接器 (测试网)
- binance_test.rs: 实时数据测试程序
- tokio-tungstenite native-tls 特性支持 TLS 连接

验证:
- 成功从 Binance 测试网接收实时 Tick 数据
- BTCUSDT 价格: ~70485 USDT

## 设计决策记录

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
