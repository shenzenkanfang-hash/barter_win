# H_Sandbox 沙盒测试框架

## 设计目的

**沙盒是全项目集成测试环境**，用于验证整个交易系统在真实行情数据下的完整闭环运行。

## 核心原则

> **沙盒 = 外部世界模拟器，不是系统的保姆。**
> 如果真实系统崩溃，那就让它崩溃，这样你才能发现真正的 Bug。

---

## 架构原则

### 沙盒职责（仅这些）

| 组件 | 职责 | 说明 |
|------|------|------|
| `StreamTickGenerator` | 生成 Tick 数据 | 仅数据生成，不处理业务逻辑 |
| `ShadowBinanceGateway` | 拦截并模拟交易所响应 | 模拟账户/持仓/成交，不干预策略 |
| `TickToWsConverter` | 转换为 WS 格式 | 仅格式转换 |

### 沙盒禁止事项

- ❌ 禁止预计算指标（让真实系统自己算）
- ❌ 禁止补全缺失数据（让真实系统自己处理空值）
- ❌ 禁止修改订单价格/状态（让真实系统自己管理）
- ❌ 禁止同步状态（让不同组件独立运作）

### 真实业务层（使用这些）

| 组件 | 路径 | 说明 |
|------|------|------|
| `Trader` | `d_checktable::h_15m::Trader` | 真实交易器 |
| `Executor` | `d_checktable::h_15m::Executor` | 真实订单执行器 |
| `Repository` | `d_checktable::h_15m::Repository` | WAL 持久化 |
| `DataFeeder` | `b_data_source` | 真实数据存储 |

---

## 事件驱动架构 (v3.0)

### 数据流

```
┌─────────────────────────────────────────────────────────────────────┐
│  沙盒层 (h_sandbox)                                                │
│                                                                     │
│  StreamTickGenerator ──→ Tick ──→ ShadowBinanceGateway              │
│         │                                  │                         │
│         │  (仅数据)                       │  (仅拦截)               │
│         └──────────────────────────────────┘                         │
└─────────────────────────────────────────────────────────────────────┘
                                ↓
┌─────────────────────────────────────────────────────────────────────┐
│  真实业务层                                                        │
│                                                                     │
│  c_data_process ──→ d_checktable ──→ e_risk_monitor ──→ f_engine │
│                                                                     │
│  Trader::run(tick_rx) ← 事件驱动，无轮询                            │
│                                                                     │
│  真实系统边界 ──────────────────────────────────────────────────────  │
└─────────────────────────────────────────────────────────────────────┘
```

### Channel 驱动

```rust
// 沙盒生成 Tick，发送到 channel
let tick = generator.next().await;
tick_tx.send(tick).await?;

// 真实 Trader 从 channel 接收
while let Some(tick) = tick_rx.recv().await {
    trader.execute_once_wal(tick).await;
}
```

### 关键约束

| 约束 | 说明 |
|------|------|
| 零轮询 | `recv().await` 阻塞等待，无 `tokio::time::sleep` |
| 零 spawn | `Trader::run()` 直接 await，不 spawn 后台任务 |
| 单事件流 | 一个 Tick 驱动完整处理链 |

---

## 入口程序

| 程序 | 说明 | 使用场景 |
|------|------|----------|
| `sandbox_pure.rs` | **推荐** 纯沙盒，仅数据注入，使用真实业务层 | 集成测试 |
| `sandbox_full_production.rs` | 完整生产级（已废弃） | ❌ 不要使用 |

### 运行命令

```bash
# 推荐：纯沙盒模式
cargo run --bin sandbox_pure -- --symbol HOTUSDT --fund 10000

# 指定数据文件
cargo run --bin sandbox_pure -- --symbol HOTUSDT --data data/HOTUSDT_1m.csv
```

---

## 两端拦截模式

| 方向 | 组件 | 职责 |
|-----|------|------|
| **上行数据** | `StreamTickGenerator` | 生成模拟 Tick，注入 DataFeeder |
| **下行请求** | `ShadowBinanceGateway` | 拦截 place_order/get_account/get_position |

```
真实系统                    沙盒
    │                        │
    ├─→ get_account() ──────→│ 返回模拟账户
    │                        │
    ├─→ place_order() ───────→│ 模拟成交
    │                        │
    │←──────────────────────┤
    │                        │
    │←──────────────────────┤
    │  push_tick() ←─────────┤
```

---

## 沙盒与正常模式对比

| 层级 | 正常模式 | 沙盒模式 |
|-----|---------|---------|
| WS 数据源 | Binance WS | StreamTickGenerator |
| 订单执行 | Binance API | ShadowBinanceGateway |
| 账户查询 | Binance API | ShadowBinanceGateway |
| 持仓查询 | Binance API | ShadowBinanceGateway |
| 业务逻辑 | c/d/e/f_engine | c/d/e/f_engine (不变) |

---

## 关键文件

| 文件 | 说明 |
|------|------|
| `src/historical_replay/tick_generator.rs` | StreamTickGenerator 实现 |
| `src/gateway/interceptor.rs` | ShadowBinanceGateway 实现 |
| `src/config.rs` | 沙盒配置 |
| `src/simulator/` | 模拟器组件 |

---

## 测试检查清单

### 1. 资金一致性
```rust
assert_eq!(account.balance + unrealized_pnl, initial_fund);
```

### 2. 持仓一致性
```rust
assert_eq!(opened_qty - closed_qty, current_position);
```

### 3. 订单闭环
```rust
for order in orders {
    assert!(matches!(order.status, Filled | Cancelled));
}
```

### 4. 性能检查
```rust
assert!(processing_time < Duration::from_millis(1));
```

---

## 开发约束

| 层级 | 允许修改 | 说明 |
|------|---------|------|
| `b_data_source` | ✅ | 数据存储（沙盒专用） |
| `c_data_process` | ❌ | 指标计算（使用真实逻辑） |
| `d_checktable` | ❌ | 检查表（使用真实规则） |
| `e_risk_monitor` | ❌ | 风控检查（使用真实风控） |
| `f_engine` | ❌ | 订单执行（使用真实引擎） |
