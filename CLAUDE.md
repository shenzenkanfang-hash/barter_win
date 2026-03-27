明白了！以下是给 **Claude 的 `claude.md` 规则文件**，让它严格遵守项目规范：

---

## `claude.md` - AI 助手行为规则

```markdown
# Claude 行为规则（量化交易系统 - Rust）

> 版本: 2026-03-27
> 适用项目: barter-rs-main / 六层架构 Rust 量化交易系统

---

## 核心原则（最高优先级）

### 1. 代码即真理
- 只描述**实际存在的代码**，不描述"应该如此"或"计划实现"
- 文档中的每个文件路径、函数名、配置项必须用 `find crates -name "*.rs"` 验证存在
- 代码示例必须能用 `cargo check` 编译通过

### 2. 诚实暴露问题
- **绝不**在沙盒层构造业务数据（假指标、假K线、默认值降级）
- **绝不**帮系统绕过错误（补全缺失数据、修改订单状态）
- 如果真实系统因数据缺失崩溃，**让它崩溃**，暴露真实 Bug

### 3. 沙盒边界明确
| 沙盒职责 | 系统职责 |
|---------|---------|
| 注入原始 Tick/K线 | 解析、存储、计算指标 |
| 拦截交易所请求 | 管理订单生命周期、状态机 |
| 模拟网络故障 | 处理超时、重试、熔断 |

**禁止**: 沙盒预计算指标、补全数据、修改订单逻辑

---

## 项目结构认知

### 六层架构（必须牢记）
```
a_common → b_data_source → c_data_process → d_checktable → e_risk_monitor → f_engine
                                              ↓
                                         g_test / h_sandbox
```

### 关键文件路径
- 沙盒入口: `crates/h_sandbox/src/main.rs` 或 `full_production_sandbox`
- 共享 Store: `crates/h_sandbox/src/context.rs` (`SandboxContext.store`)
- Trader: `crates/f_engine/src/core/engine.rs` 或 `crates/e_risk_monitor/src/`
- DataFeeder: `crates/b_data_source/src/feeder.rs`
- VolatilityManager: `crates/c_data_process/src/volatility/`

---

## 禁止行为（红色警戒线）

### ❌ 绝对禁止
1. **构造假数据**:
   ```rust
   // 禁止: 在沙盒里计算指标注入
   let fake_indicator = calculate_in_sandbox(kline);
   trader.inject_signal(fake_indicator);
   ```

2. **默认值降级**:
   ```rust
   // 禁止: 用默认值掩盖错误
   let kline = store.get_current_kline().unwrap_or_default();
   ```

3. **绕过真实系统**:
   ```rust
   // 禁止: 沙盒帮 Trader"修复"数据流
   if trader.get_kline().is_none() {
       // 构造一个假的...
   }
   ```

4. **实例隔离不修复**:
   ```rust
   // 禁止: DataFeeder 和 Trader 各自创建 Store
   let data_feeder = DataFeeder::new(default_store());
   let trader = Trader::new(default_store()); // 错误！不同实例
   ```

### ✅ 正确做法
```rust
// 共享 Store 实例
let shared_store = Arc::new(MarketDataStore::new());
let data_feeder = DataFeeder::new(shared_store.clone());
let trader = Trader::new(shared_store);

// 沙盒只注入原始数据
data_feeder.push_tick(tick).await;  // 写入 Store
// Trader 自己读取
let kline = trader.get_current_kline().await?;  // 可能报错，不掩盖
```

---

## 文档编写规则

### 目录结构（必须遵循）
```
docs/
├── README.md
├── 00-meta/
├── 10-overview/
├── 20-crates/          # 与 crates/ 一一对应
│   ├── a_common.md
│   ├── b_data_source.md
│   ├── c_data_process.md
│   ├── d_checktable.md
│   ├── e_risk_monitor.md
│   ├── f_engine.md
│   ├── g_test.md
│   └── h_sandbox.md
├── 30-api/
└── 90-archive/
```

### 文件头部模板
```markdown
---
对应代码: crates/xxx/src/
最后验证: 2026-03-27
状态: 活跃/草稿/归档
---
```

### 内容约束
- 只记录**已实现**的功能
- 必须包含 `limitations.md` 诚实列出限制
- 代码路径用 `` ` `` 包裹，且真实存在
- 不保留 AI 调试过程中的错误思路

### 版本覆盖规则
如果类似文档已经有老版本，**直接替换**旧版本，并在文件头部标注版本变更说明：

```markdown
---
对应代码: crates/xxx/src/
最后验证: 2026-03-27
状态: 活跃
版本变更:
  - v2 (2026-03-27): 重写数据流章节，补充并行执行模型
  - v1 (2026-03-20): 初稿
---
```

**禁止**：
- 创建 `xxx_v1.md`、`xxx_old.md` 等冗余版本
- 在同一目录保留多个功能重复的文档
- 添加"新版本请见 xxx.md"的跳转链接

---

## 交互规则

### 当用户说"测试"时
1. 先问:**"测业务功能还是生产级压力？"**
2. 业务功能: 验证数据流、指标计算、订单成交
3. 生产级: 延迟、故障注入、一致性检查

### 当用户说"修复"时
1. 先问:**"是沙盒问题还是真实系统问题？"**
2. 沙盒问题: 修复数据注入、Store 共享
3. 系统问题: **不绕过**，暴露给系统自己处理

### 当用户说"对比版本"时
1. 使用 `git diff` 对比老版本 vs 新版本
2. 识别数据流变更（Store 共享、指标计算路径）
3. 指出是否引入"实例隔离"或"降级路径"问题

### 当用户说"整理文档"时
1. 审计现有文档与代码一致性
2. 删除/归档过时文档（AI 调试笔记、历史设计稿）
3. 按 `docs/` 架构重建，以当前代码为准

---

## 自检清单（每次回复前）

- [ ] 我是否检查了代码路径真实存在？
- [ ] 我是否在沙盒层构造了业务数据？（禁止）
- [ ] 我是否帮系统掩盖了错误？（禁止）
- [ ] 我是否描述了"计划实现"的功能？（禁止，只描述已实现的）
- [ ] 文档中的代码示例能否编译通过？

---

## 违规处理

如果用户指出我违反了以上规则：
1. 立即停止当前做法
2. 承认具体违规点
3. 按正确规则重新执行
4. 不辩解、不重复错误

---

## 生效声明

本规则覆盖所有历史对话习惯，优先级最高。
如有冲突，以本文件为准。
```

---

## 使用方法

1. **保存位置**: 项目根目录 `claude.md` 或 `.claude.md`
2. **加载方式**: 每次对话开始时引用 `claude.md`
3. **更新方式**: 修改后告知 Claude "规则已更新，重新加载"

---
