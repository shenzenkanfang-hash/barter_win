# 量化交易系统 - Rust 重构项目

## 项目目标
核心是先有再改，先实现在优化。基于 Go 量化交易系统迁移到 Rust，采用 Barter-rs 风格架构的高性能高可用系统。

## 编译器配置
- cargo.exe: `C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe`
- rustc.exe: `C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe`
- 构建前需设置环境变量: `set RUSTC=C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe`

---

## 核心概念

**策略 = 设计图/蓝图（名词）**，**引擎 = 执行运行时（动词）**

类比游戏开发：
- 游戏引擎（Unity/Unreal）≠ 具体的游戏
- 引擎提供运行时能力
- 游戏设计师画蓝图（策略图纸）
- 引擎执行蓝图

交易系统也一样：
- `c_data_process`: 设计各种策略蓝图（通道、策略类型、信号生成）
- `d_checktable`: 引擎的硬性约束（交易所规则检查）
- `f_engine`: 真正的执行引擎（运行时）

---

## 六层架构

```
┌─────────────────────────────────────────────────────────┐
│                      a_common                          │
│         工具层: API/WS通用组件、错误类型、配置          │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                    b_data_source                         │
│        数据/网关层: 纯粹调用，无任何业务逻辑            │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                   c_data_process                         │
│           信号生成层: 指标计算、信号生成                │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                   d_checktable                           │
│           检查层: CheckTable汇总（异步并发）            │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                   e_risk_monitor                         │
│              合规约束层: 交易所硬性规则                  │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                      f_engine                            │
│              引擎运行时层: 协调执行                      │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                       g_test                             │
│                    测试层: 集成测试                      │
└─────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────┐
│                      h_sandbox                           │
│                   沙盒层: 实验性代码                     │
└─────────────────────────────────────────────────────────┘
```

---

## 执行流程

```
市场数据 (b_data_source)
    │
    ▼
指标计算 (c_data_process) → 产生交易信号
    │
    ▼
d_checktable 检查层（异步并发）
    │
    ▼
e_risk_monitor 风控层（串行同步）
    │
    ▼
f_engine 引擎执行闭环
状态更新 + 数据存储 (f_engine + e_risk_monitor)
```

---

## crates/ 目录结构

```
crates/
├── a_common/           # 工具层: API/WS网关、配置、通用类型
│
├── b_data_source/      # 数据层: DataFeeder、K线合成、Tick
│
├── c_data_process/     # 信号生成层: 指标计算、信号生成
│
├── d_checktable/       # 检查层: CheckTable汇总（异步并发）
│
├── e_risk_monitor/     # 合规约束层: 风控检查、仓位管理
│
├── f_engine/           # 引擎运行时层: 核心执行
│
├── g_test/             # 测试层: 集成测试
│
└── h_sandbox/          # 沙盒层: 实验性代码
```

---

## f_engine/src/ 子模块结构（强制约束）

**禁止在子模块外新增文件，所有新功能必须放入对应子模块。**

```
f_engine/src/
├── core/               # 核心引擎
│   ├── engine.rs       # TradingEngine 主循环
│   ├── pipeline.rs     # 交易管道
│   ├── state.rs        # 引擎状态
│   ├── strategy_pool.rs# 策略池
│   └── mod.rs
│
├── order/              # 订单模块
│   ├── order.rs        # OrderExecutor
│   ├── gateway.rs      # ExchangeGateway trait
│   ├── mock_binance_gateway.rs
│   └── mod.rs
│
├── channel/            # 通道模块
│   ├── mode_switcher.rs# 交易模式切换
│   └── mod.rs
│
├── types.rs            # 共享类型
└── lib.rs              # 库入口
```

---

## 技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| Runtime | Tokio | 异步 IO，多线程任务调度 |
| 状态管理 | FnvHashMap | O(1) 查找 |
| 同步原语 | parking_lot | 比 std RwLock 更高效 |
| 数值计算 | rust_decimal | 金融计算避免浮点精度问题 |
| 时间处理 | chrono | DateTime<Utc> |
| 错误处理 | thiserror | 清晰的错误类型层次 |
| 日志 | tracing | 结构化日志 info!/warn!/error! |
| 序列化 | serde | Serialize/Deserialize |

---

## 架构原则（强制）

### 1. 高频路径无锁
- Tick接收、指标更新、策略判断全部无锁
- 锁仅用于下单和资金更新
- 锁外预检所有风控条件

### 2. 增量计算 O(1)
- EMA、SMA、MACD 等指标必须增量计算
- K线增量更新当前K线

### 3. 三层指标体系
- TR (True Range): 波动率突破判断
- Pine颜色: 趋势信号 (MACD + EMA10/20 + RSI)
- 价格位置: 周期极值判断

### 4. 混合持仓模式
- 资金池 RwLock 保护（低频）
- 策略持仓独立计算（无锁）

---

## 代码规范（强制）

### 1. 所有 lib.rs 顶部必须添加:
```rust
#![forbid(unsafe_code)]
```

### 2. 派生宏顺序:
```rust
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
```

### 3. 错误类型模式 (使用 thiserror):
```rust
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum MyError {
    #[error("描述: {0}")]
    MyVariant(String),
}
```

### 4. 避免的问题:
- 禁止使用 `panic!()`，全部返回 Result
- 禁止在高频路径加锁
- 禁止过多 `clone()`，优先使用引用

---

## 编译活动规则

- 开发阶段禁止编译: 不执行 cargo build/check/test
- 功能优先: 先完成所有功能代码实现
- 编译归属测试工程师: verify 阶段由测试工程师执行编译验证
- 自动提交: 每次修改或创建文件后自动 git commit

---

## 当前进度

| Phase | 状态 | 说明 |
|-------|------|------|
| Phase 1: Foundation | 完成 | TradingError, Order, Position, FundPool |
| Phase 2: Market Data | 完成 | Tick, KLine, KLineSynthesizer |
| Phase 3: Indicator | 完成 | EMA, RSI, PineColor, PricePosition |
| Phase 4: Strategy | 完成 | Signal, TradingDecision |
| Phase 5: Engine | 完成 | RiskPreChecker, OrderExecutor, ModeSwitcher |
| Phase 6: Integration | 完成 | TradingEngine, c层整理 |
| Phase 8: StateManager Trait | 完成 | StateViewer + StateManager 定义 |
| Phase 9: PositionManager | 完成 | LocalPositionManager impl StateManager |
| Phase 10: UnifiedStateView | 完成 | SystemSnapshot 完整实现 |
| V4.0 Architecture | 完成 | x_data 重构 + 终极验收通过 |
