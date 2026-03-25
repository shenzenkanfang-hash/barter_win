# f_engine 引擎状态管理设计文档

## 1. 概述

f_engine 引擎状态管理模块提供生产级量化交易引擎的核心状态管理能力，采用严格的封装和线程安全设计。

### 1.1 设计目标

- **高封装**: 所有字段私有化，外部无法直接访问数据
- **线程安全**: 使用 `Arc<RwLock<EngineState>>` 保护共享状态
- **高性能**: 高频指标使用原子类型，无锁操作
- **可观测性**: 完整的状态快照和健康检查

### 1.2 模块结构

```
f_engine/src/core/
├── engine_state.rs  # 引擎全局状态
├── state.rs          # 品种级状态
└── mod.rs            # 模块导出
```

---

## 2. 核心类型

### 2.1 EngineStateHandle - 线程安全句柄

**用途**: 提供跨线程共享的引擎状态访问入口

**创建方式**:

```rust
// 基础创建
let state = EngineStateHandle::new(EngineMode::Production);

// 自定义熔断配置
let config = CircuitBreakerConfig::production();
let state = EngineStateHandle::with_circuit_breaker(EngineMode::Production, config);

// 完整配置
let state = EngineStateHandle::with_config(
    EngineMode::Production,
    Environment::Production,
    CircuitBreakerConfig::production(),
);
```

**访问模式**:

```rust
// 原子操作（无锁）
state.read().record_tick();

// 状态查询（读锁）
if state.read().can_trade() { ... }

// 状态修改（写锁）
{
    let mut s = state.write();
    s.start();
    s.register_symbol("BTC-USDT");
}
```

### 2.2 EngineStatus - 引擎状态枚举

```rust
pub enum EngineStatus {
    Initializing,  // 初始化中
    Running,        // 运行中
    Paused,         // 已暂停
    ShuttingDown,   // 优雅关闭中
    Stopped,        // 已停止
    Error,          // 错误状态
}
```

### 2.3 EngineMode - 运行模式

```rust
pub enum EngineMode {
    Backtest,      // 回测模式
    Simulation,    // 模拟交易
    Production,    // 实盘交易
}
```

### 2.4 Environment - 运行环境

```rust
pub enum Environment {
    Development,   // 开发环境
    Test,          // 测试环境
    Staging,       // 预发布环境
    Production,    // 生产环境
}
```

### 2.5 HealthStatus - 健康状态

```rust
pub enum HealthStatus {
    Healthy,    // 健康
    Degraded,   // 降级（部分功能异常）
    Unhealthy,  // 不健康（严重问题）
}
```

---

## 3. 模块间调用规则

### 3.1 严格禁止

⚠️ **以下行为严格禁止**:

1. 直接读写 `EngineState.xxx` 字段
2. 直接读写 `SymbolState.xxx` 字段
3. 直接读写 `TradeLock.xxx` 字段
4. 直接读写 `SymbolMetrics.xxx` 字段
5. 绕过接口方法直接操作内存

### 3.2 正确方式

✅ **所有跨模块访问必须通过方法**:

```rust
// ❌ 错误 - 直接访问字段
state.status = EngineStatus::Running;
state.tick_processed += 1;

// ✅ 正确 - 通过方法访问
state.write().start();
state.read().record_tick();
```

---

## 4. 接口清单

### 4.1 EngineState 主要方法

#### 生命周期管理

| 方法 | 说明 |
|------|------|
| `start()` | 启动引擎 |
| `pause()` | 暂停引擎 |
| `resume()` | 恢复引擎 |
| `start_shutdown()` | 开始优雅关闭 |
| `complete_shutdown()` | 完成关闭 |
| `stop()` | 停止引擎 |
| `set_error(msg)` | 设置错误状态 |
| `clear_error()` | 清除错误状态 |

#### 状态查询

| 方法 | 说明 |
|------|------|
| `engine_id()` | 获取引擎唯一ID |
| `mode()` | 获取运行模式 |
| `environment()` | 获取运行环境 |
| `status()` | 获取当前状态 |
| `health()` | 获取健康状态 |
| `can_trade()` | 是否可以交易 |
| `is_shutting_down()` | 是否正在关闭 |
| `is_paused()` | 是否暂停 |
| `is_stopped()` | 是否已停止 |
| `is_error()` | 是否处于错误状态 |
| `uptime()` | 获取运行时间 |
| `start_time()` | 获取启动时间 |
| `last_active_time()` | 获取最后活跃时间 |
| `error_message()` | 获取错误消息 |

#### 原子指标（无锁操作）

| 方法 | 说明 |
|------|------|
| `record_tick()` | 记录 tick 处理（原子） |
| `record_order_sent()` | 记录订单发送（原子） |
| `record_order_filled()` | 记录订单成交（原子） |
| `record_order_failed()` | 记录订单失败（原子） |
| `record_signal()` | 记录信号生成（原子） |
| `tick_processed()` | 获取 tick 处理数 |
| `order_sent()` | 获取订单发送数 |
| `order_filled()` | 获取订单成交数 |
| `order_failed()` | 获取订单失败数 |
| `fill_rate()` | 获取成交率 |
| `fail_rate()` | 获取失败率 |

#### 熔断管理

| 方法 | 说明 |
|------|------|
| `check_circuit_breaker()` | 检查并执行熔断动作 |
| `trigger_circuit_breaker()` | 触发熔断 |
| `reset_circuit_breaker()` | 重置熔断器 |
| `circuit_breaker()` | 获取熔断器状态（只读） |

#### 品种管理

| 方法 | 说明 |
|------|------|
| `register_symbol(symbol)` | 注册品种 |
| `register_symbols(symbols)` | 批量注册品种 |
| `unregister_symbol(symbol)` | 注销品种 |
| `get_symbol(symbol)` | 获取品种状态（只读） |
| `get_symbol_mut(symbol)` | 获取品种状态（可变） |
| `has_symbol(symbol)` | 检查品种是否注册 |
| `registered_symbols()` | 获取所有已注册品种 |
| `symbol_count()` | 获取品种数量 |

#### 健康检查

| 方法 | 说明 |
|------|------|
| `update_health()` | 更新健康状态 |
| `self_check()` | 执行自检 |

### 4.2 SymbolState 主要方法

| 方法 | 说明 |
|------|------|
| `symbol()` | 获取品种符号 |
| `startup_state()` | 获取启动状态 |
| `trade_lock()` | 获取交易锁（只读） |
| `trade_lock_mut()` | 获取交易锁（可变） |
| `metrics()` | 获取品种指标（只读） |
| `metrics_mut()` | 获取品种指标（可变） |
| `timeout_secs()` | 获取超时阈值 |
| `is_1m_timeout(now)` | 检查分钟级是否超时 |
| `is_daily_timeout(now)` | 检查日线级是否超时 |
| `last_1m_signal()` | 获取分钟级信号 |
| `last_daily_signal()` | 获取日线级信号 |
| `record_1m_request(ts)` | 记录分钟级请求 |
| `record_1m_ok(ts, sig_ts, decision)` | 记录分钟级成功 |
| `record_daily_request(ts)` | 记录日线级请求 |
| `record_daily_ok(ts, decision)` | 记录日线级成功 |

### 4.3 TradeLock 主要方法

| 方法 | 说明 |
|------|------|
| `is_stale(tick_ts)` | 检查 tick 是否过期 |
| `update(tick_ts, qty, price)` | 更新锁状态 |
| `position_value()` | 获取持仓值 |
| `timestamp()` | 获取时间戳 |
| `position_qty()` | 获取持仓数量 |
| `position_price()` | 获取持仓价格 |
| `position_ts()` | 获取持仓更新时间戳 |

### 4.4 SymbolMetrics 主要方法

| 方法 | 说明 |
|------|------|
| `tick_processed()` | tick 处理数 |
| `signal_generated()` | 信号生成数 |
| `order_sent()` | 订单发送数 |
| `order_filled()` | 订单成交数 |
| `order_failed()` | 订单失败数 |
| `fill_rate()` | 成交率 |
| `record_tick()` | 记录 tick |
| `record_signal()` | 记录信号 |
| `record_order_sent()` | 记录订单发送 |
| `record_order_filled()` | 记录订单成交 |
| `record_order_failed()` | 记录订单失败 |

### 4.5 CircuitBreakerConfig 主要方法

| 方法 | 说明 |
|------|------|
| `production()` | 创建生产配置 |
| `backtest()` | 创建回测配置 |
| `max_consecutive_errors()` | 最大连续错误次数 |
| `pause_duration_secs()` | 暂停时长（秒） |
| `auto_resume()` | 是否自动恢复 |

### 4.6 CircuitBreaker 主要方法

| 方法 | 说明 |
|------|------|
| `is_triggered()` | 是否触发熔断 |
| `consecutive_errors()` | 连续错误计数 |
| `config()` | 获取配置 |
| `triggered_at()` | 获取触发时间 |
| `scheduled_resume_at()` | 获取计划恢复时间 |
| `record_error()` | 记录错误 |
| `reset()` | 重置 |
| `check()` | 检查并返回动作 |

---

## 5. 调用关系图

```
┌─────────────────────────────────────────────────────────────┐
│                      TradingEngine                          │
│  ┌─────────────────────────────────────────────────────┐    │
│  │           EngineStateHandle (Arc<RwLock>)           │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      EngineState                            │
│  ├── 标识: engine_id, mode, environment                    │
│  ├── 生命周期: status, health, error_message                │
│  ├── 熔断器: CircuitBreaker                                │
│  ├── 原子指标: tick_processed, order_sent, etc.            │
│  └── 品种管理: symbols: FnvHashMap<SymbolState>            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      SymbolState                            │
│  ├── symbol: String                                        │
│  ├── trade_lock: TradeLock                                │
│  ├── metrics: SymbolMetrics                                │
│  └── 信号缓存: last_1m_signal, last_daily_signal           │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. 线程安全模型

### 6.1 共享状态访问

```
EngineStateHandle
    │
    ├── Arc<RwLock<EngineState>>
    │       │
    │       ├── 多线程共享所有权
    │       │
    │       └── RwLock: 读并发，写独占
    │
    └── parking_lot::RwLock (更高效)
```

### 6.2 原子操作（无锁）

```
高频指标:
  - tick_processed: AtomicU64
  - order_sent: AtomicU64
  - order_filled: AtomicU64
  - order_failed: AtomicU64
  - signal_generated: AtomicU64
  - error_count: AtomicU32

优势:
  - 无需加锁即可读写
  - 适用于高频路径
```

### 6.3 写锁操作

```
需要写锁的场景:
  - 状态变更: start(), pause(), stop()
  - 熔断器: record_error(), reset()
  - 品种注册: register_symbol()
  - 配置更新: update_circuit_breaker_config()

原则:
  - 写锁范围尽量小
  - 避免在持锁时执行耗时操作
```

---

## 7. 熔断机制

### 7.1 CircuitBreakerConfig

```rust
pub struct CircuitBreakerConfig {
    max_consecutive_errors: u32,    // 最大连续错误次数
    pause_duration_secs: u64,       // 暂停时长
    auto_resume: bool,              // 是否自动恢复
}
```

### 7.2 熔断流程

```
正常状态
    │
    ├── 错误发生 → record_error()
    │                 │
    │                 ▼
    │            consecutive_errors++
    │                 │
    │                 ▼
    │        是否 >= max_consecutive_errors?
    │                 │
    │         ┌───────┴───────┐
    │         ▼               ▼
    │        Yes             No
    │         │               │
    │         ▼               ▼
    │    触发熔断         返回 None
    │         │
    │         ▼
    │    is_triggered = true
    │    triggered_at = now
    │         │
    │         ▼
    │    scheduled_resume_at = now + pause_duration
    │         │
    │         ▼
    │    返回 CircuitBreakerAction::Pause
    │
    └── check() 定时检查
              │
              ▼
        是否过了暂停期?
              │
              ├─ Yes + auto_resume → reset()
              └─ Yes + !auto_resume → CircuitBreakerAction::Stop
```

---

## 8. 健康检查

### 8.1 健康状态判断

```rust
fn update_health(&mut self) {
    let fail_rate = self.fail_rate();
    let consecutive = self.consecutive_errors();

    self.health = if consecutive >= 10 || fail_rate > 0.5 {
        HealthStatus::Unhealthy
    } else if consecutive >= 3 || fail_rate > 0.2 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };
}
```

### 8.2 自检项目

```rust
fn self_check(&self) -> Result<()> {
    // 1. 品种是否重复
    // 2. 状态一致性 (is_shutting_down vs status)
    // 3. 计数器是否异常
}
```

---

## 9. 优雅关闭

### 9.1 关闭流程

```
start_shutdown()
    │
    ├── is_shutting_down = true
    ├── shutdown_start_time = Some(now)
    └── status = ShuttingDown
            │
            ├── can_trade() → false
            │
            └── 等待未完成订单...
                    │
                    ▼
            complete_shutdown()
                    │
                    ├── status = Stopped
                    └── is_shutting_down = false
```

### 9.2 can_trade() 判断

```rust
fn can_trade(&self) -> bool {
    self.status == EngineStatus::Running
        && !self.is_shutting_down
        && !self.circuit_breaker.is_triggered
}
```

---

## 10. 使用示例

### 10.1 初始化引擎

```rust
use f_engine::{
    EngineStateHandle, EngineMode, Environment,
    CircuitBreakerConfig,
};

let state = EngineStateHandle::with_config(
    EngineMode::Production,
    Environment::Staging,
    CircuitBreakerConfig::production(),
);

// 启动
{
    let mut s = state.write();
    s.start();
    s.register_symbols(&["BTC-USDT", "ETH-USDT"]);
}
```

### 10.2 处理 Tick

```rust
// 原子操作，无需加锁
state.read().record_tick();

// 查询状态
if !state.read().can_trade() {
    return;
}
```

### 10.3 执行订单

```rust
// 更新指标
state.read().record_order_sent();

// 执行订单...

// 更新结果
{
    let mut s = state.write();
    if success {
        s.record_order_filled();
    } else {
        s.record_order_failed();
        s.record_error();
        // 检查熔断
        if s.check_circuit_breaker() != CircuitBreakerAction::None {
            // 处理熔断
        }
    }
}
```

### 10.4 监控指标

```rust
let snapshot = state.read().metrics_snapshot();
println!("Tick: {}, Order: {}/{} (Fill: {:.2}%)",
    snapshot.tick_processed,
    snapshot.order_filled,
    snapshot.order_sent,
    snapshot.fill_rate * 100.0
);
```

---

## 11. 编译与测试

### 11.1 编译

```bash
cargo check -p f_engine
```

### 11.2 运行测试

```bash
cargo test -p f_engine --lib
```

### 11.3 测试覆盖

- 引擎生命周期
- 原子指标
- 熔断器
- 优雅关闭
- 品种注册
- 健康更新
- 自检

---

## 12. 设计原则总结

### 12.1 封装原则

| 规则 | 说明 |
|------|------|
| 所有字段私有化 | `xxx: T` 而非 `pub xxx: T` |
| 接口隔离 | 外部只能通过方法访问 |
| 不可变配置 | 配置通过构造函数注入 |

### 12.2 线程安全原则

| 场景 | 方案 |
|------|------|
| 共享状态 | `Arc<RwLock<T>>` |
| 高频指标 | `AtomicU64/U32` |
| 低频配置 | `RwLock` + 缓存 |

### 12.3 性能原则

| 原则 | 实现 |
|------|------|
| 读多写少 | `RwLock` 读并发 |
| 无锁高频 | 原子操作 |
| O(1) 查找 | `FnvHashMap` |

---

*文档版本: 1.0.0*
*更新时间: 2026-03-24*
