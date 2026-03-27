# 沙盒测试指南（当前实现）

> 最后验证: 2026-03-27
> 状态: 活跃

---

## 一、快速开始

### 1.1 运行命令

```bash
cargo run --bin full_production_sandbox -- \
  --symbol HOTUSDT \
  --start 2025-10-09T00:00:00Z \
  --end 2025-10-11T23:59:59Z
```

### 1.2 参数说明

| 参数 | 说明 | 示例 |
|------|------|------|
| --symbol | 交易对 | HOTUSDT |
| --start | 开始时间 | 2025-10-09T00:00:00Z |
| --end | 结束时间 | 2025-10-11T23:59:59Z |

---

## 二、核心机制（当前代码行为）

### 2.1 数据注入

每个 Tick 同时写入：
- DataFeeder（API 查询用）
- MarketDataStore（Trader 读取用）

代码位置: `crates/b_data_source/src/api/data_feeder.rs`

### 2.2 Store 共享

SandboxContext.store 与 Trader.store 是同一 Arc 实例。

代码位置:
- `crates/h_sandbox/src/simulator/mod.rs` - SandboxContext
- `crates/f_engine/src/core/engine.rs` - TradingEngine

### 2.3 指标计算

VolatilityManager 在 Store 更新时自动触发。

代码位置: `crates/b_data_source/src/store/volatility.rs`

### 2.4 错误处理

数据缺失时 Trader 报错，不返回默认值。

---

## 三、核心组件

### 3.1 历史回放控制器

代码: `crates/h_sandbox/src/historical_replay/replay_controller.rs`

职责:
- 控制回放速度
- 管理数据分片
- 生成 Tick 流

### 3.2 账户模拟器

代码: `crates/h_sandbox/src/simulator/account.rs`

职责:
- 开仓/平仓逻辑
- 资金管理
- PnL 计算

### 3.3 订单模拟器

代码: `crates/h_sandbox/src/simulator/order.rs`

职责:
- 订单撮合
- 成交记录
- 持仓更新

### 3.4 风控检查器

代码: `crates/h_sandbox/src/simulator/risk_checker.rs`

职责:
- 交易前风控检查
- 持仓限额
- 止损检查

---

## 四、沙盒配置

### 4.1 配置文件

代码位置: `crates/h_sandbox/src/config.rs`

```rust
pub struct SandboxConfig {
    pub symbol: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub initial_balance: Decimal,
    // ...
}
```

---

## 五、验证方法

### 5.1 检查 Store 数据

```rust
// 检查 Store 是否有数据
let kline = ctx.store.get_current_kline("HOTUSDT");
assert!(kline.is_some());

// 检查波动率是否计算
let vol = ctx.store.get_volatility("HOTUSDT");
assert!(vol > Decimal::ZERO);
```

### 5.2 检查账户状态

```rust
// 检查账户余额
let balance = account.get_balance();
assert!(balance > Decimal::ZERO);

// 检查持仓
let position = account.get_position("HOTUSDT");
```

---

## 六、已知问题

| 问题 | 描述 | 状态 |
|------|------|------|
| 仅支持单品种 | 每次只能测试一个交易对 | 已知限制 |
| 内存存储 | 重启后数据丢失 | 需持久化 |
| 简化撮合 | 无盘口深度模拟 | 已知限制 |

---

## 七、测试示例

### 7.1 单元测试

```bash
cargo test -p h_sandbox
```

### 7.2 集成测试

```bash
cargo test -p g_test
```