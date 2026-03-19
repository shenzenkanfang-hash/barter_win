# 代码库结构

**分析日期：** 2026-03-20

## 目录布局

```
barter-rs-main/
├── Cargo.toml                     # Workspace 根目录
├── barter/                        # 核心交易引擎
├── barter-data/                   # 市场数据流
├── barter-execution/              # 订单执行
├── barter-integration/            # 底层协议框架
├── barter-instrument/             # 合约/资产数据结构
├── barter-macro/                  # 过程宏
├── .github/workflows/             # CI/CD
├── .planning/codebase/            # 架构文档 (本仓库)
└── release-plz.toml               # 发布配置
```

## Crate 用途

**barter (核心引擎)：**
- 用途：高性能交易引擎和系统编排
- 位置：`barter/src/`
- 关键模块：`engine/`、`strategy/`、`risk/`、`execution/`、`statistic/`、`system/`、`backtest/`

**barter-data (市场数据)：**
- 用途：用于流式获取公共市场数据的 WebSocket 集成
- 位置：`barter-data/src/`
- 关键模块：`exchange/`、`streams/`、`subscription/`、`books/`、`transformer/`

**barter-execution (执行)：**
- 用途：账户数据流和订单执行
- 位置：`barter-execution/src/`
- 关键模块：`client/`、`order/`、`trade/`、`balance/`、`exchange/`

**barter-integration (集成框架)：**
- 用途：底层异步通信原语
- 位置：`barter-integration/src/`
- 关键模块：`protocol/`、`stream/`、`socket/`、`channel/`、`serde/`

**barter-instrument (合约)：**
- 用途：交易所、资产、合约定义和索引
- 位置：`barter-instrument/src/`
- 关键模块：`exchange/`、`asset/`、`instrument/`、`index/`

**barter-macro (宏)：**
- 用途：自定义 serde 派生宏
- 位置：`barter-macro/src/`

## 关键文件位置

**入口点：**
- `barter/src/lib.rs`：主库入口点
- `barter/src/system/mod.rs`：系统编排
- `barter/examples/`：使用示例

**核心逻辑：**
- `barter/src/engine/mod.rs`：Engine 定义和 Processor trait
- `barter/src/engine/state/mod.rs`：EngineState 管理
- `barter/src/engine/state/trading.rs`：TradingState
- `barter/src/engine/state/asset/`：资产状态跟踪
- `barter/src/engine/state/instrument/`：合约状态跟踪
- `barter/src/engine/state/order/`：订单生命周期管理
- `barter/src/engine/state/position.rs`：持仓跟踪
- `barter/src/engine/action/`：引擎动作 (取消、平仓、发送)

**策略：**
- `barter/src/strategy/mod.rs`：策略 traits 和 DefaultStrategy
- `barter/src/strategy/algo.rs`：AlgoStrategy trait
- `barter/src/strategy/close_positions.rs`：ClosePositionsStrategy trait
- `barter/src/strategy/on_disconnect.rs`：OnDisconnectStrategy trait
- `barter/src/strategy/on_trading_disabled.rs`：OnTradingDisabled trait

**风险：**
- `barter/src/risk/mod.rs`：RiskManager trait
- `barter/src/risk/check/`：风险检查工具

**数据层：**
- `barter-data/src/lib.rs`：MarketStream trait
- `barter-data/src/exchange/`：交易所连接器
- `barter-data/src/streams/`：流构建器
- `barter-data/src/subscription/`：订阅类型
- `barter-data/src/books/`：订单簿管理

**执行层：**
- `barter-execution/src/lib.rs`：ExecutionClient trait
- `barter-execution/src/client/`：客户端实现
- `barter-execution/src/order/`：订单类型和生命周期

**合约层：**
- `barter-instrument/src/exchange.rs`：ExchangeId 枚举
- `barter-instrument/src/asset.rs`：资产类型
- `barter-instrument/src/instrument.rs`：合约类型
- `barter-instrument/src/index.rs`：索引集合

**集成层：**
- `barter-integration/src/lib.rs`：核心 traits (Transformer、Validator、Terminal)
- `barter-integration/src/protocol/`：StreamParser 实现
- `barter-integration/src/stream/`：流工具

## 命名规范

**文件：**
- `mod.rs`：模块根 (例如 `engine/mod.rs`)
- `*.rs`：功能文件 (例如 `engine/state/mod.rs`、`engine/action/mod.rs`)
- 所有 Rust 源文件使用 snake_case

**模块：**
- snake_case：`engine_state`、`order_manager`、`close_positions`
- 使用单数名词：`asset` 而不是 `assets`、`instrument` 而不是 `instruments`

**类型：**
- PascalCase：`EngineState`、`TradingState`、`OrderRequestOpen`
- Traits 使用后缀 kind：`AlgoStrategy`、`ClosePositionsStrategy`
- 事件类型：`EngineEvent`、`MarketEvent`、`AccountEvent`

**Traits：**
- 动词-名词或 -er 后缀：`Processor`、`Connector`、`Validator`
- 交易策略使用 `-Strategy` 后缀：`AlgoStrategy`

## 添加新代码的位置

**新增交易所集成 (barter-data)：**
1. 添加交易所模块：`barter-data/src/exchange/<exchange_name>/`
2. 在 `mod.rs` 中实现 `Connector` trait
3. 在 `barter-data/src/subscription/` 中添加订阅种类
4. 如需要，在 `barter-data/src/transformer/` 中添加转换器

**新增策略：**
1. 创建模块：`barter/src/strategy/<strategy_name>.rs`
2. 实现相关 trait(s)：`AlgoStrategy`、`ClosePositionsStrategy` 等
3. 在 `barter/src/strategy/mod.rs` 中注册
4. 在 `barter/examples/` 中添加示例

**新增风险检查：**
1. 添加到 `barter/src/risk/check/`
2. 实现检查逻辑
3. 在 `barter/src/risk/mod.rs` 中注册

**新增订单类型：**
1. 添加到 `barter-execution/src/order/`
2. 如需要，更新 `ExecutionClient` trait
3. 在 `barter-execution/tests/` 中添加测试

**新增市场数据类型：**
1. 添加订阅种类：`barter-data/src/subscription/<kind>.rs`
2. 实现 `SubscriptionKind` trait
3. 添加转换器映射：`barter-data/src/transformer/`

## 特殊目录

**examples/：**
- 用途：引擎/系统使用的工作示例
- 位置：`barter/examples/`
- 注意：包含 `config/`、`data/` 子目录

**benches/：**
- 用途：性能基准测试
- 位置：`barter/benches/`

**tests/：**
- 用途：集成测试
- 位置：`barter/tests/`

## Workspace 配置

**Cargo.toml (workspace 根目录)：**
- 定义所有 workspace 成员
- 共享依赖在 `[workspace.dependencies]` 中

**每个 crate 都有：**
- 带依赖的 `Cargo.toml`
- 以 `src/lib.rs` 为入口点
- `src/error.rs` 用于错误类型

---

*结构分析：2026-03-20*
