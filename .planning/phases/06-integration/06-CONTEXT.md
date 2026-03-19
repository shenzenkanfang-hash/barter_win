================================================================================
Phase 6: Integration 上下文
================================================================================

## 阶段目标

完成从市场数据输入到订单执行输出的全链路集成。

## 现有状态

### 已完成模块
- account: Order, Position, FundPool, TradingError
- market: Tick, KLine, KLineSynthesizer, MarketConnector, MarketStream
- indicator: EMA, RSI, PineColor, PricePosition
- strategy: Strategy trait, Signal, TradingMode, OrderRequest
- engine: RiskPreChecker, OrderExecutor, ModeSwitcher

### 存在Gap
1. 类型不一致: strategy::Side (Long/Short) vs account::Side (Buy/Sell)
2. WebSocket stub为空: MarketStream trait未实现
3. 调用链未串联: 各层独立，无数据流
4. 无程序入口: main.rs不存在

## 集成路径

market::Tick → indicator计算 → strategy判断 → engine风控 → account执行

## 类型转换规则

strategy::Side::Long → account::Side::Buy
strategy::Side::Short → account::Side::Sell

strategy::OrderRequest → account::Order (通过 OrderExecutor)

## 依赖关系

market (数据源)
    ↓
indicator (计算)
    ↓
strategy (决策)
    ↓
engine (风控+执行)
    ↓
account (持仓+资金)
