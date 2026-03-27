# 系统架构（当前实现）

> 最后验证: 2026-03-27
> 状态: 活跃

---

## 一、八层架构全景

```
┌─────────────────────────────────────────────────────────────────────┐
│ L8: h_sandbox (回测沙盒层)                                          │
│   职责: 历史数据回放、压力测试、策略验证                              │
├─────────────────────────────────────────────────────────────────────┤
│ L7: g_test (测试验证层)                                              │
│   职责: 集成测试、单元测试、策略黑盒测试                              │
├─────────────────────────────────────────────────────────────────────┤
│ L6: f_engine (交易引擎运行时层)                                      │
│   职责: 订单执行、策略调度、资金池管理、状态协调                        │
├─────────────────────────────────────────────────────────────────────┤
│ L5: e_risk_monitor (风险监控层)                                      │
│   职责: 风控检查、持仓管理、账户池、合规约束                          │
├─────────────────────────────────────────────────────────────────────┤
│ L4: d_checktable (检查层)                                            │
│   职责: 15分钟检查、日线检查、交易规则验证                            │
├─────────────────────────────────────────────────────────────────────┤
│ L3: c_data_process (数据处理层)                                      │
│   职责: 指标计算、信号生成、Pine语言指标、策略状态                     │
├─────────────────────────────────────────────────────────────────────┤
│ L2: b_data_source (数据源层)                                         │
│   职责: WebSocket行情、REST API、K线合成、波动率检测                  │
├─────────────────────────────────────────────────────────────────────┤
│ L1: a_common + x_data (基础设施层 + 业务数据抽象层)                   │
│   职责: 错误类型、配置管理、通用类型、状态管理trait                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 二、模块依赖关系

### 2.1 依赖顺序（从底向上）

```
x_data (数据定义)
    ↓
a_common (基础设施)
    ↓
b_data_source (数据源)
    ↓
c_data_process (数据处理)
    ↓
d_checktable (检查层)
    ↓
e_risk_monitor (风控)
    ↓
f_engine (引擎)
    ↓
h_sandbox (沙盒顶层)
```

### 2.2 关键模块说明

| 模块 | 路径 | 职责 |
|------|------|------|
| a_common/api | `crates/a_common/src/api/` | Binance API/WS 网关，只返回原始消息类型 |
| a_common/ws | `crates/a_common/src/ws/` | WebSocket 连接器 |
| b_data_source | `crates/b_data_source/src/api/data_feeder.rs` | 数据注入、K线合成 |
| b_data_source/store | `crates/b_data_source/src/store/` | 市场数据存储、波动率 |
| c_data_process | `crates/c_data_process/src/` | 指标计算、信号生成 |
| d_checktable | `crates/d_checktable/src/` | 检查层 |
| e_risk_monitor | `crates/e_risk_monitor/src/` | 风控、持仓管理 |
| f_engine/core | `crates/f_engine/src/core/engine.rs` | 交易引擎核心 |

---

## 三、数据流（以代码为准）

### 3.1 主数据流

```
Tick 注入 → DataFeeder.push_tick()
                ↓
        MemoryStore.update_with_tick()
                ↓
        VolatilityManager.calculate()
                ↓
        Trader.get_current_kline() / get_volatility()
```

### 3.2 关键代码位置

**Store 共享**（沙盒层）：
- `crates/h_sandbox/src/simulator/mod.rs` - SandboxContext.store
- `crates/b_data_source/src/store/memory_store.rs` - MarketDataStore

**数据注入**：
- `crates/b_data_source/src/api/data_feeder.rs` - DataFeeder.push_tick()

**指标计算**：
- `crates/b_data_source/src/store/volatility.rs` - VolatilityManager

---

## 四、当前限制

### 4.1 已知的系统限制

| 限制项 | 描述 | 代码位置 |
|--------|------|----------|
| 仅支持单品种测试 | 硬编码交易对 | `crates/h_sandbox/src/config.rs` |
| 波动率计算 | 简单移动平均 | `crates/b_data_source/src/store/volatility.rs` |
| 沙盒撮合 | 即时成交，无盘口深度 | `crates/h_sandbox/src/simulator/mod.rs` |
| 内存存储 | 重启后数据丢失 | `crates/b_data_source/src/store/memory_store.rs` |

---

## 五、技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| Runtime | Tokio | 异步 IO，多线程任务调度 |
| 状态管理 | FnvHashMap | O(1) 查找 |
| 同步原语 | parking_lot | 比 std RwLock 更高效 |
| 数值计算 | rust_decimal | 金融计算避免浮点精度问题 |
| 时间处理 | chrono | DateTime<Utc> |
| 错误处理 | thiserror | 清晰的错误类型层次 |
| 日志 | tracing | 结构化日志 |
| 序列化 | serde | Serialize/Deserialize |
| 数据库 | rusqlite 0.32 | SQLite 持久化 |

---

## 六、存储方案

### 6.1 平台自动检测

`Platform::detect()` 自动选择 Windows/Linux 路径：
- Windows: E盘高速内存盘
- Linux: /dev/shm/

代码位置: `crates/a_common/src/config/platform.rs`

### 6.2 存储层级

| 层级 | 路径 | 用途 |
|------|------|------|
| SQLite 持久化 | `E:/backup/trading_events.db` | 交易事件持久化 |
| 内存备份 | `E:/shm/backup/` | 高速内存盘备份 |

---

## 七、构建与运行

### 7.1 编译

```bash
cargo check --all
```

### 7.2 运行沙盒

```bash
cargo run --bin full_production_sandbox -- \
  --symbol HOTUSDT \
  --start 2025-10-09T00:00:00Z \
  --end 2025-10-11T23:59:59Z
```

---

## 八、验证方法

```rust
// 检查 Store 是否有数据
let kline = ctx.store.get_current_kline("HOTUSDT");
assert!(kline.is_some());

// 检查波动率是否计算
let vol = ctx.store.get_volatility("HOTUSDT");
assert!(vol > Decimal::ZERO);
```