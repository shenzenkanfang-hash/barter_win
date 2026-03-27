明白了！以下是结合你项目实际情况的**完整 `claude.md` 规则文件**：

---

## `claude.md` - AI 助手行为规则

```markdown
# Claude 行为规则（量化交易系统 - Rust 六层架构）

> 版本: 2026-03-27
> 项目: barter-rs-main / 六层架构 Rust 量化交易系统
> 路径: D:/Rust项目/barter-rs-main

---

## 核心原则（最高优先级）

### 1. 代码即真理
- 只描述**实际存在的代码**，不描述"应该如此"或"计划实现"
- 文档中的每个文件路径必须用 `find crates -name "*.rs"` 验证存在
- 代码示例必须能用 `cargo check` 编译通过

### 2. 诚实暴露问题
- **绝不**在沙盒层构造业务数据（假指标、假K线、默认值降级）
- **绝不**帮系统绕过错误（补全缺失数据、修改订单状态）
- 如果真实系统因数据缺失崩溃，**让它崩溃**，暴露真实 Bug

### 3. 六层架构边界（强制）

```
a_common → b_data_source → c_data_process → d_checktable → e_risk_monitor → f_engine
                                              ↓
                                         g_test / h_sandbox
```

| 层级 | 职责 | 禁止 |
|------|------|------|
| a_common | 工具、配置、错误类型 | 业务逻辑 |
| b_data_source | DataFeeder、Tick、K线合成 | 指标计算 |
| c_data_process | EMA/RSI/Pine指标、信号生成 | 订单执行 |
| d_checktable | 异步并发检查 | 状态修改 |
| e_risk_monitor | 风控、仓位管理 | 策略决策 |
| f_engine | TradingEngine执行闭环 | 数据获取 |
| g_test | 集成测试 | 生产代码 |
| h_sandbox | **数据注入、订单拦截** | **业务逻辑** |

---

## 项目结构认知（必须牢记）

### 关键文件路径
| 组件 | 路径 | 说明 |
|------|------|------|
| 沙盒入口 | `crates/h_sandbox/src/main.rs` | 实验性代码，数据注入 |
| 共享Store | `crates/h_sandbox/src/context.rs` | `SandboxContext.store` |
| TradingEngine | `crates/f_engine/src/core/engine.rs` | 主循环 |
| DataFeeder | `crates/b_data_source/src/feeder.rs` | 数据注入 |
| 指标计算 | `crates/c_data_process/src/` | EMA/RSI/Pine |
| 风控 | `crates/e_risk_monitor/src/` | 仓位、合规 |
| 订单执行 | `crates/f_engine/src/order/` | OrderExecutor |

### f_engine 子模块结构（强制）
```
f_engine/src/
├── core/           # engine.rs, pipeline.rs, state.rs, strategy_pool.rs
├── order/          # order.rs, gateway.rs, mock_binance_gateway.rs
├── channel/        # mode_switcher.rs
├── types.rs
└── lib.rs          # 必须包含 #![forbid(unsafe_code)]
```

---

## 沙盒设计原则（红色警戒线）

### 正确数据流（当前实现）
```
Tick → DataFeeder.push_tick()
          ↓
    MarketDataStore.update_with_tick()  ← 共享Store（Arc）
          ↓
    VolatilityManager.calculate()
          ↓
    Trader.get_current_kline()  ← 读取真实数据
```

### ❌ 绝对禁止（AI历史错误）
```rust
// 禁止1: 沙盒构造假指标
let fake_indicator = calculate_in_sandbox(kline);
trader.inject_signal(fake_indicator);

// 禁止2: 默认值降级
let kline = store.get_current_kline().unwrap_or_default();

// 禁止3: 实例隔离不修复
let data_feeder = DataFeeder::new(default_store());
let trader = Trader::new(default_store()); // 错误！不同实例

// 禁止4: 帮系统绕过错误
if trader.get_kline().is_none() {
    // 构造一个假的...
}
```

### ✅ 正确做法（当前代码）
```rust
// 共享 Store 实例
let shared_store = Arc::new(MarketDataStore::new());
let data_feeder = DataFeeder::new(shared_store.clone());
let trader = Trader::new(shared_store);

// 沙盒只注入原始数据
data_feeder.push_tick(tick).await;

// Trader 自己读取，可能报错
let kline = trader.get_current_kline().await?;  // 暴露真实问题
```

---

## 代码规范（强制）

### 1. lib.rs 顶部
```rust
#![forbid(unsafe_code)]
```

### 2. 派生宏顺序
```rust
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
```

### 3. 错误类型（thiserror）
```rust
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum MyError {
    #[error("描述: {0}")]
    MyVariant(String),
}
```

### 4. 高频路径（禁止加锁）
- Tick接收、指标更新、策略判断：**无锁**
- 锁仅用于：下单、资金更新
- 锁外预检所有风控条件

---

## 编译与开发规则

| 阶段 | 规则 | 说明 |
|------|------|------|
| 开发 | **禁止编译** | 不执行 cargo build/check/test |
| 功能 | 先完成再优化 | 先有再改 |
| 验证 | 测试工程师执行 | verify 阶段才编译 |
| 提交 | 自动 git commit | 每次修改后提交 |

---

## 文档目录架构（必须遵循）

```
docs/
├── README.md                 # 文档入口
├── 00-meta/
│   ├── conventions.md        # 文档规范
│   └── glossary.md           # 术语表（策略/引擎/信号）
├── 10-overview/
│   ├── index.md              # 六层架构简介
│   ├── quickstart.md         # 5分钟上手
│   ├── architecture.md       # 数据流（以当前代码为准）
│   └── limitations.md        # 当前限制（诚实清单）
├── 20-crates/                # 与 crates/ 一一对应
│   ├── a_common.md
│   ├── b_data_source.md
│   ├── c_data_process.md
│   ├── d_checktable.md
│   ├── e_risk_monitor.md
│   ├── f_engine.md
│   ├── g_test.md
│   └── h_sandbox.md          # 沙盒数据注入、订单拦截
├── 30-api/
│   ├── public-interfaces.md  # StateManager/OrderExecutor
│   ├── configuration.md      # 配置项清单
│   └── examples/
│       ├── basic-engine.rs
│       └── sandbox-test.rs
└── 90-archive/
    ├── README.md
    └── 2025-03-go-version/   # 迁移前文档（如有）
```

### 文件头部模板
```markdown
---
对应代码: crates/xxx/src/
最后验证: 2026-03-27
状态: 活跃/草稿/归档
---
```

---

## 交互规则（话术标准）

### 当用户说"测试"
1. 先问：**"测业务功能还是生产级压力？"**
2. 业务功能：验证数据流、指标计算、订单成交
3. 生产级：延迟、故障注入、一致性检查

### 当用户说"修复"
1. 先问：**"是沙盒问题还是真实系统问题？"**
2. 沙盒问题：修复数据注入、Store共享
3. 系统问题：**不绕过**，暴露给系统自己处理

### 当用户说"对比版本"
1. 使用 `git diff` 对比老版本 vs 新版本
2. 识别数据流变更（Store共享、指标计算路径）
3. 指出是否引入"实例隔离"或"降级路径"问题

### 当用户说"整理文档"
1. 审计现有文档与代码一致性
2. 删除/归档过时文档（AI调试笔记、历史设计稿）
3. 按 `docs/` 架构重建，以当前代码为准

### 当用户说"快速实现"
1. 提醒：**"先有再改，功能优先"**
2. 不提前优化，不追求编译通过
3. 完成后再由测试工程师验证

---

## 自检清单（每次回复前）

- [ ] 我是否检查了代码路径真实存在？（`find crates -name "*.rs"`）
- [ ] 我是否在高频路径加了锁？（禁止）
- [ ] 我是否在沙盒层构造了业务数据？（禁止）
- [ ] 我是否帮系统掩盖了错误？（禁止）
- [ ] 我是否描述了"计划实现"的功能？（禁止，只描述已实现的）
- [ ] 我是否使用了 `panic!()`？（禁止，用 Result）
- [ ] 文档中的代码示例能否编译通过？（`cargo check`）

---

## 违规处理

如果用户指出我违反了以上规则：
1. 立即停止当前做法
2. 承认具体违规点（引用规则编号）
3. 按正确规则重新执行
4. 不辩解、不重复错误

---

## 生效声明

本规则覆盖所有历史对话习惯，优先级最高。
如有冲突，以本文件为准。

最后更新: 2026-03-27
项目路径: D:/Rust项目/barter-rs-main
```

---

## 使用方式

1. **保存位置**: `D:/Rust项目/barter-rs-main/claude.md`
2. **加载指令**: 每次对话开始时告知 "遵循 claude.md 规则"
3. **更新方式**: 修改后告知 "规则已更新，重新加载 claude.md"

需要我补充**特定场景的应对话术**（如用户要求"绕过错误快速演示"时的标准回复）吗？
