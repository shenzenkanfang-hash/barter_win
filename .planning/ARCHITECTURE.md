# 架构

**分析日期：** 2026-03-20

## 模式概述

**总体：** 事件驱动的交易引擎，带可插拔组件

**关键特征：**
- 以 `Engine` 处理器为中心的事件驱动架构
- Strategy 和 RiskManager traits 用于自定义交易逻辑
- 市场数据、执行和交易逻辑之间清晰分离
- 用于 O(1) 查找的索引数据结构
- 支持异步/同步执行模型

## 层级

**核心引擎：**
- 用途：编排交易操作的核心事件处理器
- 位置：`barter/src/engine/`
- 包含：Engine、状态管理、动作处理、审计系统
- 依赖：barter-data、barter-execution、barter-instrument、barter-integration
- 使用者：System、Examples

**策略层：**
- 用途：可插拔的交易逻辑 (算法订单、平仓、断开处理)
- 位置：`barter/src/strategy/`
- 包含：`AlgoStrategy`、`ClosePositionsStrategy`、`OnDisconnectStrategy`、`OnTradingDisabled`
- 依赖：Engine、Execution、Instrument
- 使用者：Engine

**风险管理层：**
- 用途：审查和过滤生成的算法订单
- 位置：`barter/src/risk/`
- 包含：`RiskManager` trait 和检查工具
- 依赖：Engine state
- 使用者：Engine

**市场数据层：**
- 用途：通过 WebSocket 从交易所流式获取公共市场数据
- 位置：`barter-data/src/`
- 包含：交易所连接器、流、订阅系统、订单簿管理
- 依赖：barter-integration、barter-instrument
- 使用者：System、Engine

**执行层：**
- 用途：流式获取账户数据和执行订单 (实盘或模拟)
- 位置：`barter-execution/src/`
- 包含：ExecutionClient trait、MockExchange、AccountEvent 类型
- 依赖：barter-instrument
- 使用者：System、Engine

**合约层：**
- 用途：交易所、资产和合约的核心数据结构
- 位置：`barter-instrument/src/`
- 包含：ExchangeId、Asset、Instrument、Index 类型
- 依赖：无 (基础层)
- 使用者：所有 crates

**集成层：**
- 用途：底层 WebSocket/HTTP 通信框架
- 位置：`barter-integration/src/`
- 包含：Transformer、StreamParser、Socket、Channel traits
- 依赖：无 (基础层)
- 使用者：barter-data、barter-execution

**宏层：**
- 用途：用于 serde Serialize/Deserialize 的过程宏
- 位置：`barter-macro/src/`
- 包含：`DeExchange`、`SerExchange`、`DeSubKind`、`SerSubKind` derives
- 依赖：无
- 使用者：barter-integration、barter-data

## 数据流

**实盘交易流程：**
1. `barter-data` 通过 WebSocket 从交易所流式获取 `MarketEvent`
2. `barter-execution` 流式获取 `AccountEvent` (余额、订单、成交)
3. `System` 通过无界通道将事件转发给 `Engine`
4. `Engine::process()` 处理事件：
   - 更新 `EngineState` (资产、合约、持仓)
   - 触发 `Strategy` 生成订单 (如果交易启用)
   - `RiskManager` 审查并可选地过滤订单
5. 批准的订单发送到 `ExecutionTxs` (路由到交易所)
6. 执行结果 (成交、取消) 作为 `AccountEvent` 流回

**回测流程：**
1. 历史市场数据作为 `MarketEvent` 流输入 `Engine`
2. `MockExecutionClient` 基于市场数据模拟成交
3. `Engine` 与实盘模式相同处理事件
4. `AuditTick` 捕获所有状态变化用于分析

**事件处理序列：**
```
MarketEvent/AccountEvent/Command
         |
         v
   Engine::process()
         |
         +---> update_from_market/account_stream()
         |           |
         |           +---> EngineState update
         |
         +---> generate_algo_orders() [if TradingState::Enabled]
         |           |
         |           +---> Strategy::generate_algo_orders()
         |           +---> RiskManager::check_orders()
         |
         +---> action() [if Command]
         |           |
         |           +---> CancelOrders, ClosePositions, SendRequests
         |
         v
     EngineAudit
```

## 关键抽象

**Engine：**
- 用途：所有交易事件的核心处理器
- 示例：`barter/src/engine/mod.rs`
- 模式：泛型于 Clock、State、ExecutionTxs、Strategy、Risk

**策略 Traits：**
- `AlgoStrategy`：基于状态生成算法订单
- `ClosePositionsStrategy`：生成平仓订单
- `OnDisconnectStrategy`：交易所断开时的自定义逻辑
- `OnTradingDisabled`：交易禁用时的自定义逻辑
- 位置：`barter/src/strategy/`

**RiskManager：**
- 用途：审查并可选地过滤订单请求
- 位置：`barter/src/risk/mod.rs`
- 模式：带 `check_*` 方法的 Trait

**Processor Trait：**
- 用途：处理事件并产生审计
- 位置：`barter/src/engine/mod.rs`
- 模式：`fn process(&mut self, event: Event) -> Self::Audit`

**MarketStream：**
- 用途：规范化市场事件的异步流
- 位置：`barter-data/src/lib.rs`
- 模式：`async_trait` 带 `init()` 工厂方法

**ExecutionClient：**
- 用途：订单执行的统一接口
- 位置：`barter-execution/src/client/mod.rs`
- 模式：带 `submit_*`、`cancel_*` 方法的 Trait

## 入口点

**System (推荐)：**
- 位置：`barter/src/system/mod.rs`
- 触发：`SystemBuilder::build()`
- 职责：组合 Engine 与执行组件，管理生命周期

**Engine Direct：**
- 位置：`barter/src/engine/mod.rs`
- 触发：`sync_run()`、`async_run()` 运行器
- 职责：处理事件、维护状态、生成订单

**示例：**
- 位置：`barter/examples/`
- 关键文件：
  - `engine_sync_with_live_market_data_and_mock_execution_and_audit.rs`
  - `engine_async_with_historic_market_data_and_mock_execution.rs`
  - `engine_sync_with_multiple_strategies.rs`

## 错误处理

**策略：** 使用 thiserror 的自定义错误类型，通过 Result/Option 传播

**模式：**
- `barter-data/src/error.rs` 中的 `DataError`
- `barter-execution/src/error.rs` 中的 `ExecutionError`
- `barter/src/engine/error.rs` 中的 `EngineError`
- `barter-data/src/error.rs` 中的 `DataError`
- `barter-integration` 中的集成错误通过 `SocketError`

**不可恢复错误：**
- `barter-integration` 中的 `Unrecoverable` trait 用于关键故障
- 引擎在不可恢复错误时停止，返回审计

## 横切关注点

**日志：** Tracing 框架 (`tracing`、`tracing-subscriber`)
- 通过 `barter/src/logging.rs` 初始化

**序列化：** 带自定义派生宏的 Serde
- `DeExchange`/`SerExchange` 用于交易所枚举
- `DeSubKind`/`SerSubKind` 用于订阅种类

**并发：** Tokio 运行时
- 市场数据的异步流
- 用于事件路由的无界通道
- `JoinHandle` 管理任务

**时间：** Chrono 配合 DateTime<Utc>
- `EngineClock` trait 用于时间抽象 (支持回测)

---

*架构分析：2026-03-20*
