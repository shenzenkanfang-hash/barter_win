# Trading System Architecture Overview

本文档描述 barter-rs 交易系统的完整架构全景图。

---

## 第一层：物理结构图

项目采用 Rust workspace 结构，将系统划分为多个独立的 crate 模块。

根目录下包含：
- `src/main.rs` - 唯一程序入口（沙盒/回测/实盘共用）
- `Cargo.toml` - workspace 配置，定义所有成员 crate
- `Cargo.lock` - 依赖锁定文件
- `data/` - 运行时数据目录（SQLite 记录、K线历史等）
- `docs/` - 架构文档
- `deploy/` - 部署配置
- `testbench/` - 功能测试报告

crates 目录下的七个核心模块：

**a_common（共享工具层）**
- 基础设施模块，提供全系统共享的能力
- 包含：API 客户端、WebSocket 连接器、日志系统、心跳监控、波动率计算、数据备份
- 目录结构：`api/`（交易所 REST API）、`ws/`（WebSocket 连接）、`logs/`（结构化日志）、`heartbeat/`（心跳系统）、`volatility/`（波动率计算）、`backup/`（内存备份）

**b_data_source（实时数据层）**
- 负责从交易所获取真实市场数据
- 包含：K线流管理、订单簿管理、历史数据查询、数据存储
- 目录结构：`ws/`（K线/深度 WebSocket 流）、`store/`（MarketDataStore 统一存储接口）、`history/`（历史数据管理）、`api/`（REST 历史查询）
- 关键组件：Kline1mStream（1分钟K线流）、MarketDataStoreImpl（数据存储实现）

**b_data_mock（模拟数据层）**
- 提供模拟/回测数据源，与 b_data_source 接口对齐
- 包含：模拟 API 网关、模拟账户、模拟 K线流、回放引擎
- 目录结构：`ws/`（模拟K线流）、`api/`（模拟网关）、`models/`（数据模型）、`replay_source/`（回放数据源）
- 关键设计：MockApiGateway（模拟交易所API）、Kline1mStream（模拟K线）

**c_data_process（数据处理层）**
- 将原始市场数据转化为交易信号
- 包含：Pine 指标计算（布林带颜色检测）、信号生成器、策略状态管理
- 目录结构：`pine_indicator_full.rs`（完整 Pine 脚本指标）、`min/`（分钟级处理）、`day/`（日级处理）、`strategy_state/`（策略状态持久化）
- 关键组件：PineColorDetectorV5（颜色检测）、SignalProcessor（信号处理器）

**d_checktable（策略检查层）**
- 核心交易逻辑层，实现 Pin 策略
- 包含：Trader（交易主循环）、Executor（订单执行）、Repository（WAL 日志）
- 目录结构：`h_15m/`（15分钟周期策略）、`h_volatility_trader/`（波动率策略）、`l_1d/`（日线策略）
- 关键组件：Trader（交易器，WAL 模式）、Executor（订单执行器）、MinSignalGenerator（信号生成）

**e_risk_monitor（风险管理层）**
- 订单和持仓的风险控制
- 包含：预风控检查、订单检查、持仓管理、账户池
- 目录结构：`risk/`（风控规则）、`position/`（持仓管理）、`persistence/`（持久化）、`shared/`（共享结构）
- 关键组件：RiskPreChecker（预风控）、OrderCheck（订单检查）、AccountPool（账户池）

**f_engine（引擎协调层）**
- 事件驱动的交易引擎
- 包含：事件总线、策略引擎、订单请求处理
- 目录结构：`event/`（事件引擎）、`core/`（核心策略）、`interfaces/`（接口定义）
- 关键组件：EventEngine（事件引擎）、EventBus（事件总线）、Strategy（策略接口）

**x_data（数据定义层）**
- 全系统共享的数据类型定义
- 包含：持仓、订单、信号、账户等核心数据结构
- 避免循环依赖，所有其他 crate 都可引入

---

## 第二层：六层架构逻辑图

### 层次映射

物理结构对应逻辑分层：

```
┌─────────────────────────────────────────────────────────┐
│                     main.rs (唯一入口)                      │
├─────────────────────────────────────────────────────────┤
│  f_engine (引擎层)    │  事件驱动协调、策略执行            │
├─────────────────────────────────────────────────────────┤
│  d_checktable (策略层)│  Pin策略逻辑、交易决策             │
├─────────────────────────────────────────────────────────┤
│  c_data_process (信号层)│  指标计算、信号生成              │
├─────────────────────────────────────────────────────────┤
│  e_risk_monitor (风控层)│  串行把关、订单校验             │
├─────────────────────────────────────────────────────────┤
│  b_data_source/mock (数据层)│  市场数据、K线流            │
├─────────────────────────────────────────────────────────┤
│  a_common (工具层)    │  共享基础设施                     │
└─────────────────────────────────────────────────────────┘
```

### 层间调用约定

**工具层（a_common）如何被共享**

a_common 不依赖任何其他 crate，是纯粹的共享工具箱。所有其他层都可以引入 a_common 获取通用能力：
- 心跳监控系统（heartbeat）：所有交易组件都报到
- WebSocket 连接器（ws）：b_data_source 依赖它连接交易所
- 日志系统（logs）：所有层使用 tracing 进行结构化日志

**数据层（b_data_*）如何为上层提供数据**

b_data_source 和 b_data_mock 实现统一的 MarketDataStore trait：
- trait 定义在 b_data_source::store::MarketDataStore
- 暴露方法：write_kline()、get_current_kline()、get_history_klines()
- 上层组件（d_checktable、c_data_process）通过 trait object 持有数据存储引用
- 数据层不处理业务逻辑，只负责数据的读写

**信号层（c_data_process）如何转化数据**

c_data_process 订阅数据层的变化，计算技术指标：
- 输入：K线数据
- 输出：MinSignalInput/MinSignalOutput（信号输入输出结构）
- 关键计算：Z-score、布林带颜色、TR比率、RSI等
- 内部状态：策略状态（PositionState、PnLState）持久化到 SQLite

**检查层（d_checktable）如何验证**

d_checktable 包含两套检查机制：
- 并发检查（CheckTable）：使用 CheckChain 并行验证多个条件
- 串行决策（Trader）：按固定顺序决策，不允许并行

**风控层（e_risk_monitor）如何把关**

e_risk_monitor 采用前置检查模式：
- RiskPreChecker：报单前预检（保证金、敞口、频率）
- OrderCheck：订单参数校验（价格、数量、类型）
- 两层检查都是同步串行，不允许跳过

**引擎层（f_engine）如何协调**

f_engine 是事件驱动的协调器：
- EventEngine：接收 Tick 事件，驱动完整处理链
- 流程：Tick → 策略决策 → 风控检查 → 订单提交
- 不直接操作数据，从数据层拉取

**沙盒层（Mock）如何模拟**

b_data_mock 提供与 b_data_source 完全一致的接口：
- MockApiGateway：模拟交易所API（下单、查询）
- MockAccount：模拟账户余额和持仓
- Kline1mStream：模拟K线数据流
- 沙盒不处理业务逻辑，只提供数据注入能力

---

## 第三层：数据生命周期图

### 数据从产生到消费的全过程

一个市场数据点的完整生命周期：

```
交易所 → WebSocket → b_data_source::Kline1mStream → MarketDataStore → c_data_process
                                                            ↓
                                                      d_checktable::Trader
                                                            ↓
                                                      e_risk_monitor
                                                            ↓
                                                      订单执行 → 交易所
```

### 数据的形态变化

**原始报价（Quote）**
- 来源：Binance WebSocket Trade Stream
- 格式：symbol, price, qty, timestamp, sequence_id
- 消费者：b_data_source 内部处理

**合成K线（Kline）**
- 来源：多个 Quote 聚合
- 格式：open, high, low, close, volume, is_closed
- 生命周期：未闭合K线（实时分区）→ 闭合K线（历史分区）
- 消费者：c_data_process 计算指标、d_checktable 读取决策

**计算指标（Indicators）**
- 来源：K线数据
- 内容：Z-score、TR比率、RSI、布林带颜色
- 存储：SQLite（strategy_state）
- 消费者：MinSignalGenerator 生成信号

**交易信号（Signal）**
- 来源：指标计算结果
- 内容：做多信号、做空信号、平仓信号、对冲信号
- 消费者：Trader 决策

**触发订单（Order）**
- 来源：Trader 决策
- 内容：symbol, side, qty, order_type
- 目的地：交易所

### 数据共享方式

所有组件通过共享的 MarketDataStore 访问数据：
- b_data_source 实现 trait，提供真实数据
- b_data_mock 实现 trait，提供模拟数据
- d_checktable 持有 trait object，不关心数据来源
- 存储位置：`data/` 目录下的 SQLite 文件和内存

---

## 第四层：执行时序图

### 组件运行模式

系统采用并行架构，多个组件同时运行：

```
┌──────────────────────────────────────────────────────────────────┐
│                        Tokio 异步运行时                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │ Kline1mStream│     │  Trader      │     │ EventEngine │        │
│  │  (并行)      │     │  (并行)      │     │  (并行)      │        │
│  └──────┬──────┘     └──────┬──────┘     └──────┬──────┘        │
│         │                   │                   │                │
│         ↓                   ↓                   ↓                │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │              MarketDataStore (共享存储)                    │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

### 沙盒数据注入与引擎处理的并行关系

沙盒和引擎通过共享存储并行运行：

```rust
// main.rs 中的并行模式
tokio::select! {
    // 沙盒：定时更新 Store
    _ = sandbox_tick.tick() => {
        let kline = generate_mock_kline();
        store.write_kline(symbol, kline, false);
    }
    
    // 引擎：读取 Store 驱动交易
    _ = engine_tick.tick() => {
        let current = store.get_current_kline(symbol);
        trader.execute_once_wal().await;
    }
}
```

### 时间同步机制

组件之间通过共享存储的时间戳同步：
- 每个 KlineData 包含 kline_start_time 和 kline_close_time
- Trader 通过 store.get_current_kline() 获取最新价格
- 不依赖组件间的时钟同步

### 数据到达与处理速度匹配

使用 tokio::select! 处理多路复用：
- Kline1mStream 可能每秒产生多个 K线更新
- Trader 每 100ms 执行一次
- 如果 K线产生速度 > 处理速度，Store 中保存最新值，Trader 使用最新值

---

## 第五层：接口契约图

### 核心组件的对外接口

**MarketDataStore trait**

```rust
pub trait MarketDataStore: Send + Sync {
    fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool);
    fn get_current_kline(&self, symbol: &str) -> Option<KlineData>;
    fn get_history_klines(&self, symbol: &str) -> Vec<KlineData>;
    fn get_volatility(&self, symbol: &str) -> Option<VolatilityData>;
}
```

**Trader 接口**

```rust
pub struct Trader { ... }
impl Trader {
    pub async fn execute_once_wal(&self) -> Result<ExecutionResult, TraderError>;
    pub async fn run(&self, tick_rx: mpsc::Receiver<Tick>);
    pub fn current_status(&self) -> PinStatus;
    pub fn set_heartbeat_token(&self, token: HeartbeatToken);
}
```

**Executor 接口**

```rust
pub struct Executor { ... }
impl Executor {
    pub async fn execute_order(&self, cmd: &TradeCommand, ...) -> Result<OrderResult, ExecutorError>;
}
```

### 接口稳定性

**稳定接口**
- MarketDataStore trait：实现者必须保持向后兼容
- Trader.execute_once_wal()：核心交易逻辑不常变
- Executor.execute_order()：订单执行接口稳定

**可能变化的接口**
- SignalProcessor：指标计算算法可能优化
- EventEngine：事件处理流程可能调整
- RiskPreChecker：风控规则可能增加

### 调用方式

**拉取模式（Pull）**
- Trader 主动从 Store 拉取数据
- 不等待数据推送

**推送模式（Push）**
- Kline1mStream 通过 channel 推送 Tick 给 Trader
- EventEngine 通过 channel 接收 Tick 事件

**同步等待**
- Executor.execute_order() 等待交易所响应

**异步通知**
- 订单状态通过回调通知

---

## 第六层：状态与存储图

### 状态分布

**集中管理状态**
- MarketDataStore：全局单例（b_data_source::default_store）
- 心跳监控系统：全局单例（a_common::heartbeat::global）
- 日志系统：全局单例（tracing）

**分散在各组件中的状态**
- Trader：持仓快照（position）、状态机（status_machine）
- SignalProcessor：策略状态（strategy_state）
- AccountPool：账户信息

### 状态一致性要求

**强一致性**
- 持仓快照（Trader.position）：交易决策前必须读取最新值
- 订单状态：必须与交易所记录一致

**最终一致性**
- 历史 K线：允许短暂延迟
- 策略状态：WAL 模式保证最终持久化

### 存储的单例模式

```rust
// b_data_source::lib.rs
static DEFAULT_STORE: OnceCell<Arc<MarketDataStoreImpl>> = OnceCell::new();

pub fn default_store() -> &'static Arc<MarketDataStoreImpl> {
    DEFAULT_STORE.get_or_init(|| Arc::new(MarketDataStoreImpl::new()))
}
```

所有组件通过 default_store() 获取同一份数据存储实例。

### 组件间数据交换

通过共享存储实现：
```
Trader ← Store.get_current_kline() ← Kline1mStream.write_kline()
```

不直接传递数据，通过共享存储中转。

---

## 第七层：错误与边界图

### 数据缺失时的行为

**Store 无数据**
- Trader.build_signal_input_fallback() 返回默认值
- 使用保守策略（Low 波动率通道）
- 日志警告：未配置指标计算器

**K线不连续**
- VolatilityManager 检测序列号跳跃
- 触发数据异常告警
- 尝试自动修复（从 API 拉取缺失数据）

### 计算失败时的回退策略

**指标计算失败**
- 降级到 MinSignalInput::default()
- 日志警告后继续执行
- 不阻止交易决策

**风控计算失败**
- 默认拒绝订单
- 不允许降级（安全优先）

### 组件故障时的隔离

**沙盒故障**
- 不影响实盘数据流
- main.rs 中沙盒和实盘路径分离

**Trader 故障**
- 状态保存在 Repository（WAL）
- 重启后从 SQLite 恢复

**网络故障**
- WebSocket 自动重连
- 订单超时处理

### 输入数据假设

**K线格式假设**
- timestamp 必须是 UTC
- open/high/low/close 必须是正数
- is_closed=true 时 kline_close_time 必须已过期

**账户假设**
- available_balance >= 0
- 持仓数量不超过 max_position

当假设不满足时，系统拒绝处理并记录日志。

---

## 第八层：设计哲学与权衡

### 为什么选择并行架构

**优势**
- Kline1mStream 独立运行，不阻塞交易决策
- 多策略可以并行运行
- 易于扩展新的数据源

**权衡**
- 状态同步复杂度增加
- 需要处理数据竞争（通过共享存储和锁）

### 为什么强调共享存储而不是消息传递

**优势**
- 实现简单，不需要消息队列基础设施
- 所有组件可以随时查询最新数据
- 易于调试和监控

**权衡**
- 不适合跨进程部署
- 状态同步延迟高于消息传递

### 为什么沙盒只注入数据而不处理业务逻辑

**优势**
- 沙盒和实盘代码路径完全一致
- 测试覆盖率高
- 沙盒实现简单

**权衡**
- 无法在沙盒中模拟复杂的交易所行为
- 订单拒绝等场景需要在实盘验证

### 关键设计权衡

**WAL vs 直写**
- 选择 WAL（Write-Ahead Log）保证数据一致性
- 牺牲部分性能换取崩溃恢复能力

**parking_lot vs tokio::sync**
- 选择 parking_lot 的 RwLock 用于同步上下文
- 避免 async 上下文中的锁竞争

**Decimal vs f64**
- 选择 rust_decimal::Decimal 处理金融计算
- 避免浮点数精度问题，牺牲部分性能

### 未来可能的调整方向

- 引入消息队列支持跨进程部署
- 增加更多风控规则
- 支持多交易所聚合
- 优化冷启动速度

---

## 总结

 barter-rs 交易系统是一个六层架构的并行量化交易系统：

- **工具层**：共享基础设施（心跳、日志、WebSocket）
- **数据层**：市场数据获取和存储（实盘/模拟）
- **信号层**：技术指标计算
- **策略层**：Pin 策略交易逻辑
- **风控层**：订单和持仓校验
- **引擎层**：事件驱动的协调器

数据通过共享存储在组件间流动，采用 WAL 模式保证一致性，并通过心跳系统实现全链路监控。
