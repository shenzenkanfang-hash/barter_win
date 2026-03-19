================================================================================
Phase 6: Integration 实施计划
================================================================================

## 任务列表

### T1: 类型统一 - Side转换
**文件**: crates/engine/src/types.rs (新建)
**内容**: 添加 Side 转换函数
```
strategy_side_to_account(strategy::Side) -> account::Side
```

### T2: WebSocket Stub 实现
**文件**: crates/market/src/websocket.rs
**内容**: 添加 MockMarketStream 实现
- 模拟生成 Tick 数据
- 实现 next_tick() 方法

### T3: 类型转换模块
**文件**: crates/engine/src/types.rs (新建)
**内容**: OrderRequest → Order 转换

### T4: TradingEngine 主引擎
**文件**: crates/engine/src/engine.rs (新建)
**内容**:
- TradingEngine 结构体
- run() 方法: 串联所有层
- tick_to_strategy() 数据流

### T5: main.rs 入口
**文件**: src/main.rs (新建)
**内容**:
- tracing 初始化
- 组件创建
- 启动 TradingEngine

## 验收标准

1. cargo build --release 能编译通过
2. main.rs 能启动（即使无真实数据）
3. 类型转换无歧义

## 执行顺序

T1 → T2 → T3 → T4 → T5
