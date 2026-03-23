# f_engine 架构文档

**生成时间**: 2026-03-24
**状态**: 建设中
**优先级说明**: P0=必须实现, P1=重要, P2=可选

---

## 1. 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                     数据流方向                               │
└─────────────────────────────────────────────────────────────┘

b_data_source (K线合成)
    │
    ↓ (K线完成事件)
c_data_process (指标计算、信号生成) ──→ 缓存到 DataCache
    │
    ↓ (f_engine 按需拉取)
d_checktable (CheckTable 并行检查)
    │
    ↓
e_risk_monitor (风控复核，串行同步)
    │
    ↓
f_engine (交易执行)
    │
    ↓
交易所 (Binance / MockGateway)
```

---

## 2. 五层职责

| 层级 | 职责 | 状态 |
|------|------|------|
| b_data_source | K线合成、增量更新 | ✅ 完成 |
| c_data_process | 指标计算、信号生成 | ✅ 完成 |
| d_checktable | CheckTable 并行检查 | ✅ 完成 |
| e_risk_monitor | 风控复核、熔断、持仓管理 | ✅ 完成 |
| f_engine | 交易执行、并发控制、状态管理 | 🔨 进行中 |

---

## 3. f_engine 核心流程

### 3.1 on_tick 主入口

```rust
pub async fn on_tick(&mut self, tick: &Tick) {
    let now = now_ts();

    // 1. K线增量更新 (无锁，O(1))
    let completed_1m = self.kline_1m.update(tick);
    let completed_xd = self.kline_xd.update(tick);

    // 2. 实时价格位置 (tick级，简单判断)
    self.update_price_position(tick);

    // 3. 分钟级检查 (K线完成触发，并发)
    if completed_1m.is_some() {
        self.check_minute_strategies_concurrent().await;
    }

    // 4. 日线级检查 (定时批量，1s间隔)
    if now - self.last_daily_ts >= 1000 {
        self.check_daily_strategies_batch();
        self.last_daily_ts = now;
    }
}
```

**优先级**: P0

### 3.2 实时价格位置 (tick级)

```rust
/// 实时价格位置更新 (tick级别)
/// 简单判断：当前价格在近期周期内的位置
fn update_price_position(&mut self, tick: &Tick) {
    // TODO: 实现价格位置判断
    // 简单逻辑：价格在 recent_high 和 recent_low 之间的百分比位置
}
```

**优先级**: P1

---

## 4. 并发控制机制

### 4.1 交易锁 (TradeLock)

**重要性**: P0 - 核心安全机制

```rust
struct TradeLock {
    timestamp: i64,      // 锁持有时间戳
    position: Position,   // 当前持仓
    position_ts: i64,    // 持仓更新时间戳
}

struct SymbolState {
    trade_lock: TradeLock,
    // ...
}
```

### 4.2 时间戳核对

**重要性**: P0 - 防止重复/丢弃tick

```rust
fn check_and_execute(&self, tick: &Tick) -> Result<(), TradeError> {
    let tick_ts = tick.timestamp;

    // 尝试获取锁
    if !self.try_lock() {
        // 获取锁失败，丢弃所有请求（安全）
        return Err(TradeError::LockFailed);
    }

    // 获取锁成功，检查时间戳
    let lock = &self.trade_lock;

    // 如果 tick 时间戳 <= 锁的时间戳，说明已被处理过
    if tick_ts <= lock.timestamp {
        return Err(TradeError::StaleTick);
    }

    // 执行交易
    self.execute(tick)?;

    // 更新锁状态
    self.trade_lock.timestamp = tick_ts;
    self.trade_lock.position = current_position();
    self.trade_lock.position_ts = now_ts();

    Ok(())
}
```

### 4.3 并发保证

| 情况 | 处理 |
|------|------|
| 获取锁失败 | 丢弃请求（安全） |
| tick_ts <= lock_ts | 丢弃（避免重复） |
| 执行成功 | 更新锁的时间戳和仓位 |

---

## 5. 分钟级策略

### 5.1 触发条件

- 1m K线完成
- tick驱动
- 并发检查多品种

### 5.2 检查流程

```rust
async fn check_minute_strategies_concurrent(&mut self) {
    // 并发检查多个品种
    // 有一个成功就执行

    // 1. 获取所有品种的信号（从 c_data_process 缓存）
    let signals = self.get_pending_1m_signals();

    // 2. 并发检查
    for signal in signals {
        tokio::spawn(async move {
            if check_and_execute(signal).await.is_ok() {
                // 有一个成功即可
            }
        });
    }
}
```

### 5.3 超时/延迟检测

**重要性**: P0 - 数据流健康检查

```rust
struct SymbolState {
    last_1m_request_ts: i64,   // 上次请求时间戳
    last_1m_ok_ts: i64,         // 上次成功获取时间戳
    last_1m_signal: Option<TradingDecision>,
}

async fn check_minute_strategies(&mut self, symbol: &str) {
    let state = self.get_symbol_state(symbol);

    // 1. 尝试获取信号
    if let Some(decision) = self.c_data_process.get_1m_signal(symbol) {
        let signal_age = now_ts() - decision.timestamp;

        if signal_age > Duration::minutes(1) {
            // 指标延迟异常 → 停止
            warn!("1m signal delay: age={}s", signal_age.as_secs());
            self.trigger_stop_mechanism();
            return;
        }

        // 正常：记录成功获取
        state.last_1m_ok_ts = now_ts();
        self.execute(decision).await;
    } else {
        // 没拿到 → 检查请求超时
        let request_elapsed = now_ts() - state.last_1m_request_ts;
        if request_elapsed > Duration::minutes(1) {
            // 超时异常 → 停止
            warn!("1m signal timeout");
            self.trigger_stop_mechanism();
        }
    }
}
```

### 5.4 异常处理

| 异常 | 条件 | 结果 |
|------|------|------|
| signal timeout | request > 1min 没拿到 | 停止 |
| signal delay | 拿到但 age > 1min | 停止 |

---

## 6. 日线级策略

### 6.1 触发条件

- Xd K线完成
- 定时批量（每1秒检查一次）
- 1h内完成所有品种

### 6.2 检查流程

```rust
fn check_daily_strategies_batch(&mut self) {
    // 批量检查所有品种
    // 在1h内完成

    for symbol in self.get_active_symbols() {
        // 1. 检查信号
        // 2. 超时/延迟检查（同分钟级逻辑）
        // 3. 执行
    }
}
```

**优先级**: P1

---

## 7. 灾备重启恢复

### 7.1 重启状态

**重要性**: P0 - 安全恢复机制

```rust
enum StartupState {
    Fresh,           // 正常启动
    Recovery(String), // 灾备恢复中，标记恢复ID
}

struct SymbolState {
    startup_state: StartupState,
    last_signal_before_crash: Option<TradingDecision>, // 崩溃前的信号
}
```

### 7.2 重启检查流程

```rust
async fn check_minute_strategies(&mut self, symbol: &str) {
    let state = self.get_symbol_state(symbol);

    // === 灾备重启检查 ===
    if matches!(state.startup_state, StartupState::Recovery(_)) {
        if let Some(decision) = self.c_data_process.get_1m_signal(symbol) {
            let signal_age = now_ts() - decision.timestamp;
            if signal_age < Duration::seconds(30) {
                // 新指标（30秒内生成），确认可以恢复
                info!("Recovery: fresh signal received, resuming trading");
                state.startup_state = StartupState::Fresh;
                self.execute(decision).await;
            }
            // age >= 30s 说明是重启前的旧指标，忽略
        }
        return; // 重启期间不交易
    }

    // === 正常流程 ===
    // ...
}
```

### 7.3 重启安全原则

| 阶段 | 处理 |
|------|------|
| 刚重启 | 不交易，等待新指标 |
| 拿到新指标（<30s） | 确认正常，恢复交易 |
| 拿到旧指标（≥30s） | 忽略，继续等待 |
| 超时/延迟 | 停止 |

---

## 8. 停止机制

### 8.1 触发条件

- 分钟级/日线级信号超时
- 分钟级/日线级信号延迟
- 熔断触发
- 手动停止

### 8.2 停止流程

```rust
fn trigger_stop_mechanism(&mut self) {
    // 1. 切换到维护模式
    self.mode_switcher.set_mode(Mode::Maintenance);

    // 2. 取消所有未完成订单
    self.cancel_pending_orders();

    // 3. 保存当前状态到灾备
    self.save_state_to_disaster_recovery();

    // 4. 通知
    self.notify_stop();
}
```

**优先级**: P2

---

## 9. 交易锁时间序列图

```
Tick A ──────────────────────────────→ 获取锁 ──→ 执行 ──→ 更新锁(t_A) ──→ 释放
     │                                      │                    ↑
     │                                      │                    │
     └──────────────────────────────────────┘                    │
                              获取锁失败 ──→ 丢弃                  │
                                                                    │
Tick B ──────────────────────────────→ 获取锁 ──→ 核对ts ──→ t_B > t_A? ──→ 执行 ──→ 更新锁(t_B)
                                   │              │
                                   │              └── 是（正常）
                                   │
Tick C ──────────────────────────────→ 获取锁 ──→ 核对ts ──→ t_C <= t_A? ──→ 丢弃
                                   │              │
                                   │              └── 是（重复）
```

---

## 10. 实现优先级汇总

| 优先级 | 模块 | 说明 |
|--------|------|------|
| P0 | `SymbolState` + `TradeLock` | 核心数据结构 |
| P0 | 交易锁 + 时间戳核对 | 防止重复/丢弃 |
| P0 | `on_tick` 框架 | 主循环 |
| P0 | 超时/延迟检测 | 数据流健康 |
| P0 | 灾备重启恢复 | 安全恢复 |
| P1 | 分钟级并发检查 | tick驱动 |
| P1 | 日线级批量检查 | 定时批量 |
| P1 | 实时价格位置 | tick级判断 |
| P2 | 停止机制 | 异常处理 |
| P2 | 模拟测试框架 | 验证策略 |

---

## 11. 核心原则

1. **高频路径无锁**: Tick接收、指标更新、策略判断全部无锁
2. **锁仅用于下单和资金更新**
3. **锁外预检所有风控条件**
4. **时间窗口控制代替频繁检查**
5. **指标异步获取，不阻塞交易**
6. **重启后强制等待新指标**

---

## 12. 命名规范

| 原命名 | 改后 | 说明 |
|--------|------|------|
| `on_minute_bar` | `check_minute_strategies` | f_engine 主动检查 |
| `on_daily_bar` | `check_daily_strategies` | f_engine 主动检查 |
| `update_indicators` | 移除 | 改为按需从 c_data_process 获取 |
| `kline_1d` | `kline_xd` | 支持配置周期 |

---

*文档状态: 建设中，待补充具体实现细节*
