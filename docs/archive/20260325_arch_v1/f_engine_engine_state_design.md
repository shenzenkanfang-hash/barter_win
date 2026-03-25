# f_engine 引擎状态设计方案（生产级优化版）

## 一、现状分析

### 现有 state.rs 内容

| 结构 | 说明 |
|------|------|
| `TradeLock` | 单品种交易锁，防止并发重复执行 |
| `SymbolState` | 单品种状态（信号缓存、时间戳） |
| `CheckConfig` | 检查配置（时间窗口） |

---

## 二、优化版设计方案

### 核心原则

| 原则 | 说明 |
|------|------|
| **线程安全** | 所有状态访问通过锁保护 |
| **接口隔离** | 字段全部 private，只暴露方法 |
| **原子指标** | 高频指标使用 AtomicU64 |
| **熔断风控** | 连续错误自动触发熔断 |
| **优雅关闭** | 支持 graceful shutdown |
| **健康检查** | 支持监控面板/API查询 |

---

## 三、最终架构图

```
┌─────────────────────────────────────────────────────────────────────┐
│                      EngineStateHandle                              │
│                  (Arc<RwLock<EngineState>>)                        │
├─────────────────────────────────────────────────────────────────────┤
│                      EngineState                                    │
├─────────────────────────────────────────────────────────────────────┤
│  生命周期                                                          │
│    ├─ start_time: DateTime<Utc>                                    │
│    ├─ last_active_time: DateTime<Utc>                              │
│    ├─ restart_count: u32                                            │
│    ├─ status: EngineStatus                                         │
│    ├─ mode: EngineMode                                             │
│    └─ health: HealthStatus                                         │
├─────────────────────────────────────────────────────────────────────┤
│  风控熔断                                                          │
│    ├─ circuit_breaker: CircuitBreaker                              │
│    ├─ is_shutting_down: bool                                       │
│    └─ shutdown_start_time: Option<DateTime<Utc>>                    │
├─────────────────────────────────────────────────────────────────────┤
│  原子指标（高性能无锁）                                             │
│    ├─ tick_processed: AtomicU64                                    │
│    ├─ order_sent: AtomicU64                                        │
│    ├─ order_filled: AtomicU64                                      │
│    ├─ order_failed: AtomicU64                                       │
│    ├─ signal_generated: AtomicU64                                   │
│    └─ error_count: AtomicU32                                        │
├─────────────────────────────────────────────────────────────────────┤
│  配置管理                                                          │
│    ├─ config_version: u64                                         │
│    └─ config_updated_at: DateTime<Utc>                             │
├─────────────────────────────────────────────────────────────────────┤
│  品种管理                                                          │
│    └─ symbols: HashMap<String, SymbolState>                        │
│         ├─ trade_lock: TradeLock                                   │
│         ├─ signals: SignalCache                                    │
│         ├─ time_windows: TimeWindows                               │
│         └─ metrics: SymbolMetrics                                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 四、新增结构（完整定义）

### 1. HealthStatus - 健康状态

```rust
/// 健康检查状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// 健康
    Healthy,
    /// 降级（部分功能异常）
    Degraded,
    /// 不健康（严重问题）
    Unhealthy,
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Healthy
    }
}
```

### 2. CircuitBreaker - 熔断配置

```rust
/// 熔断配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 最大连续错误次数
    pub max_consecutive_errors: u32,
    /// 暂停时长
    pub pause_duration_secs: u64,
    /// 是否自动恢复
    pub auto_resume: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_consecutive_errors: 5,
            pause_duration_secs: 60,
            auto_resume: true,
        }
    }
}

/// 熔断器状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreaker {
    /// 配置
    pub config: CircuitBreakerConfig,
    /// 当前连续错误计数
    pub consecutive_errors: u32,
    /// 是否触发熔断
    pub is_triggered: bool,
    /// 触发时间
    pub triggered_at: Option<DateTime<Utc>>,
    /// 自动恢复定时器
    pub auto_resume_at: Option<DateTime<Utc>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self;
    pub fn record_error(&mut self);
    pub fn reset(&mut self);
    pub fn should_pause(&self) -> bool;
    pub fn should_auto_resume(&self) -> bool;
}
```

### 3. SymbolMetrics - 品种指标（合并入 SymbolState）

```rust
/// 品种级运行指标
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolMetrics {
    /// 累计处理 tick 数
    pub tick_processed: u64,
    /// 累计生成信号数
    pub signal_generated: u64,
    /// 累计下单数
    pub order_sent: u64,
    /// 累计成交数
    pub order_filled: u64,
    /// 最后信号时间
    pub last_signal_time: Option<DateTime<Utc>>,
    /// 最后下单时间
    pub last_order_time: Option<DateTime<Utc>>,
}
```

### 4. EngineState - 引擎全局状态（最终版）

```rust
/// 引擎全局状态（私有字段，接口化设计）
///
/// 所有字段为 private，通过方法访问
pub struct EngineState {
    // ─────────────────────────────────────────────────────────
    // 生命周期（private）
    // ─────────────────────────────────────────────────────────
    start_time: DateTime<Utc>,
    last_active_time: DateTime<Utc>,
    restart_count: u32,
    status: EngineStatus,
    mode: EngineMode,
    health: HealthStatus,

    // ─────────────────────────────────────────────────────────
    // 风控熔断（private）
    // ─────────────────────────────────────────────────────────
    circuit_breaker: CircuitBreaker,
    is_shutting_down: bool,
    shutdown_start_time: Option<DateTime<Utc>>,

    // ─────────────────────────────────────────────────────────
    // 原子指标（private，通过方法访问）
    // ─────────────────────────────────────────────────────────
    tick_processed: AtomicU64,
    order_sent: AtomicU64,
    order_filled: AtomicU64,
    order_failed: AtomicU64,
    signal_generated: AtomicU64,
    error_count: AtomicU32,

    // ─────────────────────────────────────────────────────────
    // 配置热更新（private）
    // ─────────────────────────────────────────────────────────
    config_version: u64,
    config_updated_at: Option<DateTime<Utc>>,

    // ─────────────────────────────────────────────────────────
    // 品种管理（private，通过方法访问）
    // ─────────────────────────────────────────────────────────
    symbols: FnvHashMap<String, SymbolState>,
}

impl EngineState {
    // ═══════════════════════════════════════════════════════════════
    // 构造函数
    // ═══════════════════════════════════════════════════════════════

    pub fn new(mode: EngineMode) -> Self;
    pub fn with_circuit_breaker(mode: EngineMode, config: CircuitBreakerConfig) -> Self;

    // ═══════════════════════════════════════════════════════════════
    // 线程安全句柄
    // ═══════════════════════════════════════════════════════════════

    pub fn handle(&self) -> EngineStateHandle;
}

/// 类型别名：线程安全的引擎状态句柄
pub type EngineStateHandle = Arc<RwLock<EngineState>>;

impl EngineStateHandle {
    pub fn new(mode: EngineMode) -> Self;
    pub fn with_circuit_breaker(mode: EngineMode, config: CircuitBreakerConfig) -> Self;
}
```

### 5. EngineState 接口方法（完全接口化）

```rust
impl EngineState {
    // ─────────────────────────────────────────────────────────
    // 生命周期管理
    // ─────────────────────────────────────────────────────────

    /// 启动引擎
    pub fn start(&mut self);

    /// 优雅关闭
    pub fn start_shutdown(&mut self);

    /// 完成关闭
    pub fn complete_shutdown(&mut self);

    /// 暂停引擎
    pub fn pause(&mut self);

    /// 恢复引擎
    pub fn resume(&mut self);

    /// 停止引擎
    pub fn stop(&mut self);

    /// 设置错误状态
    pub fn set_error(&mut self, msg: String);

    /// 清除错误状态
    pub fn clear_error(&mut self);

    // ─────────────────────────────────────────────────────────
    // 状态查询（只读方法）
    // ─────────────────────────────────────────────────────────

    /// 检查是否可以交易
    pub fn can_trade(&self) -> bool;

    /// 获取当前状态
    pub fn status(&self) -> EngineStatus;

    /// 获取运行模式
    pub fn mode(&self) -> EngineMode;

    /// 获取健康状态
    pub fn health(&self) -> HealthStatus;

    /// 是否正在关闭
    pub fn is_shutting_down(&self) -> bool;

    /// 是否暂停
    pub fn is_paused(&self) -> bool;

    /// 获取运行时间
    pub fn uptime(&self) -> Duration;

    // ─────────────────────────────────────────────────────────
    // 指标查询（无锁读取）
    // ─────────────────────────────────────────────────────────

    pub fn tick_processed(&self) -> u64;
    pub fn order_sent(&self) -> u64;
    pub fn order_filled(&self) -> u64;
    pub fn order_failed(&self) -> u64;
    pub fn signal_generated(&self) -> u64;
    pub fn error_count(&self) -> u64;
    pub fn consecutive_errors(&self) -> u32;

    pub fn fill_rate(&self) -> f64;
    pub fn fail_rate(&self) -> f64;

    // ─────────────────────────────────────────────────────────
    // 指标更新（原子操作）
    // ─────────────────────────────────────────────────────────

    pub fn record_tick(&self);
    pub fn record_order_sent(&self);
    pub fn record_order_filled(&self);
    pub fn record_order_failed(&self);
    pub fn record_signal(&self);
    pub fn record_error(&self);
    pub fn reset_consecutive_errors(&self);

    // ─────────────────────────────────────────────────────────
    // 健康检查
    // ─────────────────────────────────────────────────────────

    pub fn update_health(&mut self);
    pub fn self_check(&self) -> Result<(), EngineError>;

    // ─────────────────────────────────────────────────────────
    // 熔断检查
    // ─────────────────────────────────────────────────────────

    pub fn check_circuit_breaker(&mut self) -> CircuitBreakerAction;
    pub fn trigger_circuit_breaker(&mut self);
    pub fn reset_circuit_breaker(&mut self);

    // ─────────────────────────────────────────────────────────
    // 品种管理
    // ─────────────────────────────────────────────────────────

    pub fn register_symbol(&mut self, symbol: &str) -> &mut SymbolState;
    pub fn unregister_symbol(&mut self, symbol: &str);
    pub fn get_symbol(&self, symbol: &str) -> Option<&SymbolState>;
    pub fn get_symbol_mut(&mut self, symbol: &str) -> Option<&mut SymbolState>;
    pub fn registered_symbols(&self) -> Vec<String>;

    // ─────────────────────────────────────────────────────────
    // 配置热更新
    // ─────────────────────────────────────────────────────────

    pub fn update_config(&mut self, config: CircuitBreakerConfig);
    pub fn config_version(&self) -> u64;
}
```

---

## 五、状态流转图（增强版）

```
                         ┌─────────────────┐
                         │  Initializing   │
                         └────────┬────────┘
                                  │ start()
                                  ▼
                         ┌─────────────────┐
            ┌────────────│    Running      │────────────┐
            │            └────────┬────────┘            │
            │                     │                     │
      pause()               is_shutting_down()    error()
            │                     │                     │
            ▼                     ▼                     ▼
    ┌───────────────┐    ┌─────────────────┐    ┌───────────────┐
    │    Paused    │    │   ShuttingDown  │    │   Unhealthy   │
    └───────┬───────┘    └────────┬────────┘    └───────┬───────┘
            │                     │                     │
       resume()             complete_shutdown()    clear_error()
            │                     │                     │
            └─────────────────────┼─────────────────────┘
                                  ▼
                         ┌─────────────────┐
                         │    Stopped      │
                         └────────┬────────┘
                                  │
                            restart()
                                  │
                                  ▼
                         ┌─────────────────┐
                         │  Initializing   │
                         └─────────────────┘

    ┌─────────────────────────────────────────────────────────────┐
    │                   Circuit Breaker                           │
    │                                                              │
    │   consecutive_errors >= max  ──► is_triggered = true        │
    │   │                                    │                    │
    │   │                              pause()                   │
    │   │                                    │                    │
    │   │                         auto_resume_at reached         │
    │   │                                    │                    │
    │   ◄────────────────────────────────────┘                    │
    │   (if auto_resume = true)                                   │
    └─────────────────────────────────────────────────────────────┘
```

---

## 六、SQLite 持久化

### 表结构

```sql
CREATE TABLE engine_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    start_time TEXT NOT NULL,
    last_active_time TEXT NOT NULL,
    restart_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    mode TEXT NOT NULL,
    health TEXT NOT NULL DEFAULT 'Healthy',
    tick_processed INTEGER NOT NULL DEFAULT 0,
    order_sent INTEGER NOT NULL DEFAULT 0,
    order_filled INTEGER NOT NULL DEFAULT 0,
    order_failed INTEGER NOT NULL DEFAULT 0,
    signal_generated INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    consecutive_errors INTEGER NOT NULL DEFAULT 0,
    circuit_breaker_triggered INTEGER NOT NULL DEFAULT 0,
    config_version INTEGER NOT NULL DEFAULT 1,
    config_updated_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE symbol_state (
    symbol TEXT PRIMARY KEY,
    tick_processed INTEGER NOT NULL DEFAULT 0,
    signal_generated INTEGER NOT NULL DEFAULT 0,
    order_sent INTEGER NOT NULL DEFAULT 0,
    order_filled INTEGER NOT NULL DEFAULT 0,
    last_signal_time TEXT,
    last_order_time TEXT,
    last_1m_ok_ts INTEGER NOT NULL DEFAULT 0,
    last_daily_ok_ts INTEGER NOT NULL DEFAULT 0,
    trade_lock_ts INTEGER NOT NULL DEFAULT 0,
    trade_lock_qty TEXT NOT NULL DEFAULT '0',
    trade_lock_price TEXT NOT NULL DEFAULT '0',
    updated_at TEXT NOT NULL
);
```

---

## 七、使用示例

### 基础使用

```rust
use f_engine::core::engine_state::{EngineStateHandle, EngineMode, CircuitBreakerConfig};

// 创建线程安全的引擎状态
let state = EngineStateHandle::new(EngineMode::Production);

// 启动
{
    let mut s = state.write();
    s.start();
}

// 记录处理（无锁原子操作）
state.read().record_tick();
state.read().record_signal();
state.read().record_order_sent();

// 批量更新
{
    let s = state.read();
    if s.can_trade() {
        println!("可以交易");
    }
}

// 暂停/恢复
{
    let mut s = state.write();
    if s.consecutive_errors() >= 5 {
        s.pause();
        s.trigger_circuit_breaker();
    }
}

// 优雅关闭
{
    let mut s = state.write();
    s.start_shutdown();
    // 此时 can_trade() 返回 false，停止接单
}
// 等待未完成订单...
{
    let mut s = state.write();
    s.complete_shutdown();
}
```

### 与 TradingEngine 集成

```rust
impl TradingEngine {
    pub fn new(mode: EngineMode) -> Self {
        Self {
            state: EngineStateHandle::new(mode),
            // ...
        }
    }

    async fn process_tick(&mut self, tick: Tick) {
        // 原子更新指标（无锁）
        self.state.read().record_tick();

        // 检查是否可交易
        {
            let s = self.state.read();
            if !s.can_trade() || s.is_shutting_down() {
                return;
            }
        }

        // 正常处理...
        let decision = self.generate_signal(&tick).await;

        if decision.action != TradingAction::Hold {
            self.state.read().record_signal();
            match self.execute_decision(decision).await {
                Ok(_) => {
                    self.state.read().record_order_sent();
                }
                Err(e) => {
                    self.handle_error(e);
                }
            }
        }
    }

    fn handle_error(&mut self, error: Error) {
        let mut s = self.state.write();
        s.record_error();

        // 检查熔断
        match s.check_circuit_breaker() {
            CircuitBreakerAction::Pause => {
                s.pause();
                error!("熔断触发，暂停交易");
            }
            CircuitBreakerAction::Stop => {
                s.stop();
                error!("连续错误达到上限，停止引擎");
            }
            CircuitBreakerAction::None => {}
        }
    }

    fn health_check(&self) -> HealthStatus {
        self.state.read().health()
    }
}
```

---

## 八、测试计划

| 测试项 | 说明 |
|--------|------|
| `test_engine_state_lifecycle` | 测试启动→运行→暂停→恢复→停止流程 |
| `test_circuit_breaker_trigger` | 测试连续错误触发熔断 |
| `test_circuit_breaker_auto_resume` | 测试自动恢复 |
| `test_graceful_shutdown` | 测试优雅关闭流程 |
| `test_atomic_metrics` | 测试原子指标无锁更新 |
| `test_can_trade_logic` | 测试 can_trade() 逻辑 |
| `test_health_update` | 测试健康状态更新 |
| `test_self_check` | 测试自检功能 |
| `test_symbol_registration` | 测试品种注册/注销 |
| `test_config_hot_update` | 测试配置热更新 |
| `test_concurrent_access` | 测试并发访问安全性 |

---

## 九、实现清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `f_engine/src/core/engine_state.rs` | 新建 | EngineState（最终版） |
| `f_engine/src/core/state.rs` | 更新 | SymbolState 合并 SymbolMetrics |
| `f_engine/src/core/mod.rs` | 更新 | 导出新模块 |
| `f_engine/src/lib.rs` | 更新 | 添加 re-export |

### 新增类型清单

| 类型 | 文件 | 说明 |
|------|------|------|
| `HealthStatus` | engine_state.rs | 健康检查状态 |
| `CircuitBreakerConfig` | engine_state.rs | 熔断配置 |
| `CircuitBreaker` | engine_state.rs | 熔断器状态 |
| `SymbolMetrics` | state.rs | 品种指标 |
| `EngineState` | engine_state.rs | 引擎状态（主结构） |
| `EngineStateHandle` | engine_state.rs | 线程安全句柄 |

---

## 十、关键优化点总结

| 优化项 | 解决的问题 | 实现方式 |
|--------|-----------|----------|
| **线程安全** | 多线程并发访问 | `Arc<RwLock<EngineState>>` |
| **原子指标** | 高并发锁竞争 | `AtomicU64/U32` |
| **熔断器** | 连续错误自动保护 | `CircuitBreaker` 结构 |
| **优雅关闭** | 安全退出不丢单 | `is_shutting_down` 标记 |
| **健康检查** | 监控面板状态 | `HealthStatus` 枚举 |
| **接口化** | 模块间隔离 | private 字段 + 方法 |
| **热更新** | 动态配置 | `config_version` 追踪 |

---

## 十一、与 c_data_process 分工

| 模块 | 职责 | 数据类型 |
|------|------|----------|
| **f_engine::EngineState** | 引擎运行状态 | tick数、订单数、错误数、熔断 |
| **c_data_process::StrategyState** | 策略交易状态 | 持仓、盈亏、信号、参数 |

```
f_engine (引擎层)
  └─ EngineState.handle() → 获取引擎指标
       │
       ▼
c_data_process (策略层)
  └─ StrategyState.manager() → 获取策略状态
       │
       ▼
e_risk_monitor (风控层)
  └─ RiskChecker → 风控规则检查
```
