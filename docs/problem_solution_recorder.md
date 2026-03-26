# 项目问题与解决方案全记录

---

## 一、文档说明

### 1.1 文档用途
- 记录所有发现的架构/代码/逻辑问题
- 记录问题原因和解决方案
- 跟踪修复状态
- 记录修改文件和验证结果

### 1.2 问题记录格式

```
==================================================
问题ID：PROB-YYYYMMDD-XXX
问题类型：架构 / 逻辑 / 代码 / 数据流 / 命名
问题描述：（清晰描述问题现象、影响）
确定解决方案：
  1. 具体操作
  2. 关键实现
  3. 涉及文件/函数
修改文件/模块：（列表）
执行优先级：高 / 中 / 低
当前状态：待执行 / 执行中 / 已完成 / 已验证
验证标准：（具体可核对的验证条件）
==================================================
```

---

## 二、待处理问题总览

| ID | 问题类型 | 问题简述 | 优先级 | 状态 |
|----|---------|---------|-------|------|
| PROB-20260326-001 | 架构 | 引擎架构重构（异步任务模式） | 高 | 待执行 |

---

## 三、架构设计（最终版）

### 3.1 引擎架构（TradeManager 模式）

**参考实现：**
Python TradeManager (D:\个人量化策略\wintrade\module\tradeManager.py)

**核心概念：**
- 异步任务（Async Task）：不是子线程，是 tokio::spawn 的协程
- 双重状态核对：引擎层 + 品种层
- 心跳机制：品种定时更新 last_beat，引擎定期检查

---

### 3.2 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│  引擎层 (Engine)                                            │
│  ├─ 任务注册表: HashMap<Symbol, SymbolState>              │
│  ├─ 触发器 → 启动任务                                     │
│  ├─ 定期检查心跳                                           │
│  └─ 发现 Ended → 移除任务                                 │
└─────────────────────────────────────────────────────────────┘
                              ↑
┌─────────────────────────────────────────────────────────────┐
│  品种层 (SymbolState) - 每个品种独立                        │
│  ├─ 自己的循环间隔 (50ms / 1s)                            │
│  ├─ 自己的运行状态                                         │
│  ├─ 自己的最后运行时间 (last_beat)                        │
│  └─ 自己执行策略 + 风控 + 下单                             │
└─────────────────────────────────────────────────────────────┘
```

---

### 3.3 品种层 SymbolState

```rust
pub struct SymbolState {
    // 基础信息
    symbol: String,
    
    // 运行状态
    status: RunningStatus,  // Running / Stopped / Ended
    
    // 心跳
    last_beat: i64,  // 最后更新时间
    
    // 持仓信息
    position_qty: Decimal,
    position_price: Decimal,
    
    // 禁止信息
    forbid_until: Option<i64>,
    forbid_reason: Option<String>,
    
    // 统计
    trade_count: u32,
    done_reason: Option<String>,
}

pub enum RunningStatus {
    Running,   // 运行中
    Stopped,   // 已停止
    Ended,     // 已结束
}
```

---

### 3.4 任务执行流程

```
触发器触发
    ↓
引擎 spawn(任务协程)
    ↓
┌─────────────────────────────────────────────────────────────┐
│  任务协程（自己循环）                                      │
│                                                             │
│  loop {                                                   │
│      // 1. 检查禁止                                       │
│      if state.forbid_until > now {                       │
│          sleep(interval);                                │
│          continue;                                        │
│      }                                                   │
│                                                             │
│      // 2. 获取锁                                         │
│      let _lock = global_lock.acquire().await;            │
│                                                             │
│      // 3. 策略计算（无锁，可并行）                        │
│      let signal = strategy().await?;                      │
│                                                             │
│      // 4. 风控 + 下单                                     │
│      risk_check(&signal)?;                               │
│      order = place_order(&signal)?;                       │
│                                                             │
│      // 5. 更新状态（加锁）                                │
│      state.update_position(order);                        │
│      state.heartbeat();                                   │
│                                                             │
│      // 6. 释放锁                                          │
│      drop(_lock);                                         │
│                                                             │
│      // 7. 检查是否结束                                    │
│      if is_trade_complete(&state) {                      │
│          state.end(reason);                               │
│          break;  // 退出循环                               │
│      }                                                   │
│                                                             │
│      // 8. 等待下一个周期                                  │
│      sleep(Duration::from_millis(self.interval)).await;  │
│  }                                                       │
└─────────────────────────────────────────────────────────────┘
```

---

### 3.5 引擎层职责

```rust
pub struct Engine {
    // 任务注册表
    tasks: FnvHashMap<String, Arc<RwLock<SymbolState>>>,
    
    // 数据库
    db: Database,
    
    // 配置
    heartbeat_timeout: i64,
}

impl Engine {
    // 引擎主循环
    async fn run(&self) {
        loop {
            // 1. 检查所有任务
            self.check_tasks();
            
            // 2. 检查心跳
            self.check_heartbeat();
            
            // 3. 持久化变化
            self.flush_changes();
            
            sleep(1000).await;
        }
    }
    
    fn check_tasks(&self) {
        for (symbol, state_arc) in &self.tasks {
            let s = state_arc.read();
            
            match s.status {
                RunningStatus::Ended => {
                    // 发现已结束，移除 + 持久化
                    self.tasks.remove(symbol);
                    self.db.persist_end(&s);
                }
                _ => {}
            }
        }
    }
    
    fn check_heartbeat(&self) {
        let now = Utc::now().timestamp();
        
        for (symbol, state_arc) in &self.tasks {
            let s = state_arc.read();
            
            if s.last_beat < now - self.heartbeat_timeout {
                // 超时，任务可能挂了
                self.handle_timeout(symbol);
            }
        }
    }
}
```

---

### 3.6 持久化策略

**只在任务变化时持久化：**

```rust
pub enum PersistEvent {
    TaskCreated { symbol, channel },
    TaskRemoved { symbol },
    TaskEnded { symbol, reason, forbid_until },
    TaskHeartbeat { symbol, last_beat },  // 不持久化
}
```

**不持久化：**
- 心跳更新（频繁）
- 普通执行状态

**持久化：**
- 创建任务
- 移除任务
- 任务结束

---

### 3.7 与 Python TradeManager 对应

| Python | Rust |
|--------|------|
| `instances` | `tasks` |
| `start_instance()` | `tokio::spawn(任务)` |
| `run_once()` | 任务自己的循环 |
| `monitor()` | `check_heartbeat()` |
| `heartbeat()` | `last_beat` |
| `scan_volatile()` | 触发器 |

---

## 四、问题详细记录

### 4.1 PROB-20260326-001：引擎架构重构

**问题类型：** 架构

**问题描述：** 
当前引擎设计不符合 TradeManager 模式：
1. 没有任务自主循环
2. 引擎主循环没有实现
3. 没有心跳机制
4. 触发器与执行层未分离

**确定解决方案：**

#### 4.1.1 重构 SymbolState

```rust
// f_engine/src/core/state.rs

pub enum RunningStatus {
    Running,
    Stopped,
    Ended,
}

pub struct SymbolState {
    symbol: String,
    status: RunningStatus,
    last_beat: i64,
    position_qty: Decimal,
    position_price: Decimal,
    forbid_until: Option<i64>,
    forbid_reason: Option<String>,
    trade_count: u32,
    done_reason: Option<String>,
}

impl SymbolState {
    pub fn heartbeat(&mut self) {
        self.last_beat = Utc::now().timestamp();
    }
    
    pub fn end(&mut self, reason: String) {
        self.status = RunningStatus::Ended;
        self.done_reason = Some(reason);
        // 禁止到下个日线周期
        self.forbid_until = Some(next_day_start());
        self.heartbeat();
    }
    
    pub fn is_forbidden(&self) -> bool {
        if let Some(until) = self.forbid_until {
            return Utc::now().timestamp() < until;
        }
        false
    }
}
```

#### 4.1.2 重构 Engine

```rust
// f_engine/src/core/engine.rs

pub struct Engine {
    tasks: FnvHashMap<String, Arc<RwLock<SymbolState>>>,
    db: Database,
    heartbeat_timeout: i64,
    global_lock: Arc<Mutex<()>>,
}

impl Engine {
    // 启动任务
    pub fn spawn_task(&self, symbol: String, interval_ms: u64) {
        let state = Arc::new(RwLock::new(SymbolState::new(symbol.clone())));
        let state_clone = state.clone();
        
        // 注册到任务表
        self.tasks.insert(symbol.clone(), state);
        
        // spawn 异步任务
        tokio::spawn(async move {
            Self::task_loop(symbol, state_clone, interval_ms).await;
        });
    }
    
    // 任务循环
    async fn task_loop(symbol: String, state: Arc<RwLock<SymbolState>>, interval_ms: u64) {
        loop {
            {
                let s = state.read();
                if s.is_forbidden() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
            
            // 执行交易
            match execute_once(&symbol).await {
                Ok(TradingComplete) => {
                    state.write().end("TradeComplete".to_string());
                    break;
                }
                Err(e) if e.should_stop() => {
                    state.write().end(format!("{:?}", e));
                    break;
                }
                _ => {}
            }
            
            state.write().heartbeat();
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }
    
    // 引擎主循环
    pub async fn run(&self) {
        loop {
            self.check_tasks();
            self.check_heartbeat();
            self.flush_changes();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
```

#### 4.1.3 实现触发器

```rust
impl Engine {
    // 触发器检查
    pub async fn check_triggers(&self) {
        // 日线触发器
        for symbol in self.scan_daily_trigger() {
            self.spawn_task(symbol, 1000);  // 慢速
        }
        
        // 分钟触发器
        for (symbol, volatility) in self.scan_minute_trigger() {
            let interval = if volatility > 0.05 { 50 } else { 1000 };
            self.spawn_task(symbol, interval);
        }
    }
}
```

**涉及文件/函数：**
- `f_engine/src/core/state.rs` - SymbolState 重构
- `f_engine/src/core/engine.rs` - Engine 重构
- `f_engine/src/core/triggers.rs` - 触发器重构

**修改文件/模块：** f_engine/src/core/

**执行优先级：** 高

**当前状态：** 待执行

**验证标准：**
- 任务启动后自主循环
- 心跳正常更新
- 任务结束自动移除
- 持久化正常工作

---

## 五、已完成问题

（修复后移动到这里）

---

## 六、完整架构图（从 architecture_full.md）

================================================================
barter-rs 高频金融量化交易系统 - 项目全景架构图
================================================================

### 一、八层架构全景

┌─────────────────────────────────────────────────────────────────────┐
│ L8: h_sandbox (回测沙盒层)                                          │
│ L7: g_test (测试验证层)                                             │
│ L6: f_engine (交易引擎运行时层)                                      │
│ L5: e_risk_monitor (风险监控层)                                     │
│ L4: d_checktable (检查层)                                           │
│ L3: c_data_process (数据处理层)                                     │
│ L2: b_data_source (数据源层)                                        │
│ L1: a_common + x_data (基础设施层 + 业务数据抽象层)                  │
└─────────────────────────────────────────────────────────────────────┘

### 二、核心概念（TradeManager 模式）

```
┌─────────────────────────────────────────────────────────────────────┐
│  引擎层 (Engine)                                                    │
│  ├─ 任务注册表: HashMap<Symbol, TaskState>                         │
│  ├─ 触发器 → 启动任务                                             │
│  ├─ 定期检查心跳                                               │
│  └─ 发现 Ended → 移除任务                                         │
└─────────────────────────────────────────────────────────────────────┘
                              ↑
┌─────────────────────────────────────────────────────────────────────┐
│  品种层 (TaskState) - 每个品种独立                                    │
│  ├─ 自己的循环间隔 (50ms / 1s)                                    │
│  ├─ 自己的运行状态                                               │
│  ├─ 自己的最后运行时间 (last_beat)                              │
│  └─ 自己执行策略 + 风控 + 下单                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 三、f_engine 结构

```
f_engine/src/
├─ core/
│  ├─ engine_v2.rs    - TradingEngineV2 主循环
│  ├─ engine.rs       - Engine (TradeManager 模式)
│  ├─ engine_state.rs - EngineState 状态管理
│  └─ mod.rs
├─ order/
│  ├─ order.rs       - OrderExecutor 订单执行器
│  └─ gateway.rs     - ExchangeGateway trait
├─ channel/
│  └─ mode_switcher.rs - ModeSwitcher 交易模式切换
└─ interfaces/
   ├─ market_data.rs  - MarketDataProvider
   └─ execution.rs    - ExchangeGateway
```

### 四、完整架构（分层自运行）

#### 4.0 架构选型：为什么不用微服务？

交易系统追求 **微秒级低延迟**，微服务通过网络通信（HTTP/gRPC）引入数十毫秒延迟，无法满足需求。
采用 **共享内存的数据服务模式**，同进程内数据交互零网络开销。

**延迟对比：**
| 架构 | 数据访问延迟 | 适用场景 |
|-----|-------------|---------|
| 微服务 | 10-50ms (网络开销) | 高吞吐、低实时要求 |
| 共享内存 | <1μs (内存访问) | 量化交易、高频交易 |

#### 4.1 数据服务模式

```
┌─────────────────────────────────────────────────────────────────────┐
│  数据服务层（共享内存，同进程）                                     │
├─────────────────────────────────────────────────────────────────────┤
│  DataFeeder (Tick/K线数据服务)                                      │
│  └─ 持续接收数据，按需提供 ws_get_1m(symbol)                        │
├─────────────────────────────────────────────────────────────────────┤
│  IndicatorCache (指标数据服务)                                      │
│  └─ 持续计算指标，按需提供 get(symbol)                              │
└─────────────────────────────────────────────────────────────────────┘
                              ↑ 提供数据
┌─────────────────────────────────────────────────────────────────────┐
│  消费者（引擎层/策略层）                                            │
│  └─ 按需调用 → 共享内存访问 → 微秒级响应                           │
└─────────────────────────────────────────────────────────────────────┘
```

#### 4.2 四层架构

```
┌─────────────────────────────────────────────────────────────────────┐
│  数据源层（后台自运行）                                            │
│  StreamTickGenerator → push_tick → DataFeeder                     │
│  作用：持续接收数据，存入内存，供其他组件拉取                       │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  指标层（后台自运行）                                              │
│  从 DataFeeder 获取 K 线 → 计算指标 → IndicatorCache               │
│  作用：持续计算指标（RSI/EMA/波动率），供其他组件查询              │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  引擎层（监控波动率触发任务）                                       │
│  监控波动率 → 波动率 > 阈值 → spawn_task                         │
│  职责：只监控波动率，不监控价格                                    │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  策略层（每个任务独立）                                            │
│  从 DataFeeder 获取价格                                            │
│  从 IndicatorCache 获取指标                                        │
│  策略计算 → 风控 → 下单                                            │
│  平仓完成 → 设置 status=Ended → 自己退出                           │
└─────────────────────────────────────────────────────────────────────┘
```

#### 4.2 各层职责

| 层级 | 职责 | 数据来源 |
|-----|------|---------|
| 数据源层 | 接收数据存入内存 | StreamTickGenerator / WebSocket |
| 指标层 | 计算指标存入缓存 | DataFeeder |
| 引擎层 | 监控波动率触发任务 | IndicatorCache |
| 策略层 | 策略/风控/下单 | DataFeeder + IndicatorCache |

#### 4.3 引擎层职责

```
Engine
├── 任务注册表 (tasks)
├── 监控波动率 (check_triggers) - 波动率 > 阈值 → spawn_task
├── 心跳检查 (check_heartbeat)
├── 任务移除 (check_tasks) - 发现 Ended 则移除
└── 持久化 (EngineDb) - 只在任务变化时持久化
```

#### 4.4 策略层职责

```
每个任务独立
├── 自己的循环 (50ms / 1s)
├── 从 DataFeeder 获取价格
├── 从 IndicatorCache 获取指标
├── 策略计算
├── 风控检查
├── 下单执行
└── 平仓完成 → 设置 status=Ended → 自己退出
```

#### 4.5 引擎层 vs 策略层职责划分

| 功能 | 引擎层 | 策略层 |
|-----|-------|--------|
| 波动率监控 | ✅ | ❌ |
| 触发任务 | ✅ | ❌ |
| 任务注册/移除 | ✅ | ❌ |
| 心跳检查 | ✅ | ❌ |
| 持久化 | ✅ | ❌ |
| 获取价格 | ❌ | ✅ 从 DataFeeder |
| 获取指标 | ❌ | ✅ 从 IndicatorCache |
| 策略计算 | ❌ | ✅ |
| 风控检查 | ❌ | ✅ |
| 下单执行 | ❌ | ✅ |

#### 4.6 TaskState 结构

```rust
pub struct TaskState {
    symbol: String,
    status: RunningStatus,  // Running/Stopped/Ended
    last_beat: i64,
    position_qty: Decimal,
    position_price: Decimal,
    forbid_until: Option<i64>,
    trade_count: u32,
    done_reason: Option<String>,
    interval_ms: u64,
}
```

#### 4.7 异步任务模式

- 不是子线程，是 `tokio::spawn` 的协程
- 引擎 spawn 任务，任务自己循环
- 任务结束自己退出，引擎发现后移除

#### 4.8 与 Python TradeManager 对应

| Python TradeManager | Rust Engine |
|--------------------|-------------|
| `instances` | `tasks` |
| `start_instance()` | `spawn_task()` |
| `run_once()` | 策略层自己的循环 |
| `monitor()` | `check_heartbeat()` |
| `scan_volatile()` | 监控波动率 |

#### 4.9 实现文件

- `crates/f_engine/src/core/engine.rs` - Engine, TaskState, RunningStatus
- `src/sandbox_main.rs` - IndicatorCache, 完整沙盒实现

### 五、技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| 运行时 | Tokio | 异步IO、多线程任务调度 |
| 状态管理 | FnvHashMap | O(1)查找，高频路径无锁 |
| 同步原语 | parking_lot | RwLock比std更高效 |
| 数值计算 | rust_decimal | 金融计算高精度 |
| 时间处理 | chrono | DateTime<Utc> |
| 错误处理 | thiserror | 禁用panic! |

### 六、关键优化点

- ✅ 高频路径无锁（Tick接收、指标更新、策略判断）
- ✅ O(1) 增量计算（EMA/SMA/RSI/MACD）
- ✅ 锁仅用于下单和资金更新

================================================================

## 七、文档更新记录

| 日期 | 更新内容 | 更新人 |
|-----|---------|-------|
| 2026-03-26 | 初始版本 | Droid |
| 2026-03-26 | 更新为最终架构（异步任务模式） | Droid |
| 2026-03-26 | 添加完整架构图（从 architecture_full.md） | Droid |
| 2026-03-26 | 添加 TradeManager 架构（引擎/策略层职责划分） | Droid |
