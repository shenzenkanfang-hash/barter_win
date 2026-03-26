================================================================
品种协程自循环设计方案（详细版）
================================================================
Author: 软件架构师
Date: 2026-03-27
Status: 待实现
Based on: pin_main.py (Python) + h_15m/ (Rust)
================================================================

一、核心设计原则
================================================================

一个品种 = 一个协程 = 一个 Trader
Engine 管 spawn/stop/监控/重启，Trader 自己 loop。

与 pin_main.py (Python) 1:1 对齐：
- Trader.loop() 自循环（对应 Python _run_loop）
- executor.rs 封装下单和存储（对应 Python place_order + _store_data）
- TradeRecord 贯穿全流程（对应 Python Account_Information）

二、架构图
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  Engine                                                          │
│  ├── InstanceMap: HashMap<symbol, TraderHandle>                 │
│  ├── spawn(symbol) → tokio::spawn(trader.start())              │
│  ├── Monitor 协程（心跳检测 + 自动重启）                          │
│  └── stop(symbol)                                               │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  h_15m::Trader.loop()                                       │
│  loop:                                                           │
│    1. record = TradeRecord.new()                                │
│    2. record.market = get_market()                              │
│    3. record.signal = signal_generator.generate()                │
│    4. record.check_*() = check()                               │
│    5. if all passed:                                           │
│         executor.send_order(record)   ← 发送订单                 │
│         executor.save_record(record)  ← 保存记录                 │
│    6. sleep(interval)                                          │
└─────────────────────────────────────────────────────────────────┘

三、h_15m 模块结构
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  h_15m/                                                         │
├─────────────────────────────────────────────────────────────────┤
│  trader.rs         # Trader（自循环）✅ 已有                    │
│  signal.rs         # 信号生成器（7条件Pin）✅ 已有              │
│  status.rs         # 状态机（PinStatus）✅ 已有                │
│  executor.rs       # 下单 + 存储 [新建]                          │
│  mod.rs            # 模块导出                                     │
└─────────────────────────────────────────────────────────────────┘

四、TradeRecord 设计（对照 Python Account_Information）
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  TradeRecord（对应 Python: Account_Information）                │
├─────────────────────────────────────────────────────────────────┤
│  id                  i64 PRIMARY KEY                            │
│  symbol              TEXT NOT NULL                              │
│  timestamp           INTEGER NOT NULL                           │
│  interval_ms          INTEGER                                   │
│  // === 行情快照（对应 Python: self.close） ===                 │
│  price                TEXT                                      │
│  volatility           REAL                                      │
│  market_status        TEXT                                      │
│  // === 持仓快照（对应 Python: pst_manager） ===                │
│  exchange_position    TEXT  -- 交易所持仓 JSON                  │
│  local_position       TEXT  -- 本地持仓记录 JSON                │
│  // === 策略状态 ===                                           │
│  trader_status        TEXT  -- PinStatus                        │
│  signal_json          TEXT  -- MinSignalOutput JSON             │
│  confidence           INTEGER                                   │
│  // === 账户状态（对应 Python: self.trade_information） ===     │
│  realized_pnl         TEXT                                      │
│  unrealized_pnl       TEXT                                      │
│  available_balance    TEXT                                      │
│  used_margin          TEXT                                      │
│  // === 订单执行（对应 Python: place_order 结果） ===           │
│  order_type           INTEGER  -- 0=开仓 1=对冲 2=加仓 3=平仓   │
│  order_direction      TEXT                                      │
│  order_quantity       TEXT                                      │
│  order_result         TEXT                                      │
│  order_timestamp      INTEGER                                   │
│  // === 检查表（对应 CheckTable 两个阶段） ===                   │
│  check_passed         INTEGER  -- 0/1                           │
│  signal_passed        INTEGER  -- 0/1                           │
│  price_check_passed   INTEGER  -- 0/1                           │
│  risk_check_passed    INTEGER  -- 0/1                           │
│  lock_acquired        INTEGER  -- 0/1                           │
│  order_executed       INTEGER  -- 0/1                           │
│  record_saved         INTEGER  -- 0/1                           │
└─────────────────────────────────────────────────────────────────┘

五、executor.rs 详细设计
================================================================

5.1 职责

┌─────────────────────────────────────────────────────────────────┐
│  Executor                                                       │
├─────────────────────────────────────────────────────────────────┤
│  send_order(signal)  → 发送到交易所（对应 Python: place_order）│
│  save_record(record) → 保存到 SQLite                            │
│  load_record(symbol) → 从 SQLite 恢复（对应 Python: _load_data）│
│  calculate_order_qty → 计算下单数量（对应 Python: open_qty 计算）│
│  rate_limit_check()  → 下单频率限制（对应 Python: last_order_   │
│                        timestamp）                               │
└─────────────────────────────────────────────────────────────────┘

5.2 下单类型枚举（对齐 Python place_order order_type）

┌─────────────────────────────────────────────────────────────────┐
│  OrderType（对应 Python: order_type 参数）                      │
├─────────────────────────────────────────────────────────────────┤
│  InitialOpen  = 0   初始开仓（多头/空头第一次开仓）              │
│  HedgeOpen    = 1   对冲开仓（反向开仓锁定）                    │
│  DoubleAdd    = 2   翻倍加仓                                    │
│  DoubleClose  = 3   翻倍平仓                                    │
│  DayHedge     = 4   日线对冲                                    │
│  DayClose     = 5   日线关仓                                    │
└─────────────────────────────────────────────────────────────────┘

5.3 send_order 逻辑（对齐 Python place_order）

```
send_order(signal: &StrategySignal, order_type: OrderType) -> Result<OrderResult, Error>

步骤：
1. rate_limit_check() - 检查下单频率（Python: min_order_interval）
2. calculate_order_qty(order_type) - 计算数量
   - order_type=0: 计算初始开仓数量（基于 risk_engine）
   - order_type=1: 对冲数量 = 反向持仓
   - order_type=2: 加仓数量 = 当前持仓 * 0.5
   - order_type=3/5: 平仓数量 = 同向全部持仓
   - order_type=4: 日线对冲 = 反向持仓
3. 预风控检查 - check_minute_order（对应 Python: order_check）
4. 调用 ExchangeGateway.place_order()
5. 记录 last_order_timestamp
6. 更新本地持仓记录（pst_manager）
```

5.4 calculate_order_qty 逻辑（对齐 Python place_order）

| order_type | 条件 | 计算方式 |
|------------|------|----------|
| 0 InitialOpen | 无持仓时可开 | min(initial_ratio, risk计算量) |
| 1 HedgeOpen | 已有持仓 | 反向持仓数量 |
| 2 DoubleAdd | 已有持仓 | 当前数量 * 0.5 |
| 3 DoubleClose | 已有持仓 | 同向全部数量 |
| 4 DayHedge | 日线模式 | 反向持仓数量 |
| 5 DayClose | 日线模式 | 同向全部数量 |

5.5 save_record 逻辑（对齐 Python _store_data）

```
save_record(record: &TradeRecord)

步骤：
1. 序列化 TradeRecord 为 JSON
2. 写入 SQLite（INSERT OR REPLACE）
3. 记录字段映射到 Python 字段：
   - record.signal_json → Python: signal_output
   - record.local_position → Python: self.pst_info
   - record.trader_status → Python: self.current_pin_status
   - record.realized_pnl → Python: self.trade_information["realized_pnl"]
```

5.6 load_record 逻辑（对齐 Python _load_data）

```
load_record(symbol: &str) -> Option<TradeRecord>

步骤：
1. 从 SQLite 查询该品种最新记录
2. 反序列化为 TradeRecord
3. 恢复字段到各组件：
   - trader.update_status(record.trader_status)
   - trader.update_position(record.local_position)
4. 无记录时返回 None

注意：load_record 对应 Python 启动时的 _load_data()
用于崩溃恢复后还原状态
```

5.7 rate_limit_check（对齐 Python last_order_timestamp）

```rust
fn rate_limit_check(&self, interval_ms: u64) -> bool {
    let now = Instant::now();
    let elapsed = now.duration_since(self.last_order_time);
    let min_interval = Duration::from_millis(interval_ms);
    if elapsed < min_interval {
        tracing::warn!("[{}] 下单频率过高，跳过本次下单", self.symbol);
        return false;
    }
    self.last_order_time = now;
    true
}
```

六、trader.rs 改造（整合 executor）
================================================================

6.1 Trader 新增字段

```rust
pub struct Trader {
    // ... 已有字段 ...
    executor: Executor,           // 新增：下单执行器
    last_order_time: RwLock<Instant>,  // 新增：下单频率控制
}

impl Trader {
    pub fn new(config: TraderConfig) -> Self {
        Self {
            // ... 已有初始化 ...
            executor: Executor::new(&config.symbol),
            last_order_time: RwLock::new(Instant::now()),
        }
    }
}
```

6.2 execute_once 改造

```rust
pub fn execute_once(&self) -> Option<StrategySignal> {
    // 1. 获取数据
    let kline = self.get_current_kline()?;
    let vol_tier = self.volatility_tier();

    // 2. 构建信号输入
    let input = self.build_signal_input()?;

    // 3. 生成信号
    let signal_output = self.signal_generator.generate(&input, &vol_tier, None);

    // 4. 状态机决策
    let status = self.status_machine.read().current_status();
    let price = self.current_price()?;

    // 5. 决策动作
    self.decide_action(&status, &signal_output, price)
}

// decide_action 返回 (signal, order_type) 元组
fn decide_action(&self, ...) -> Option<(StrategySignal, OrderType)> {
    match status {
        PinStatus::Initial | ... => {
            if signal.long_entry {
                return Some((self.build_open_signal(...), OrderType::InitialOpen));
            }
            // ...
        }
    }
}
```

6.3 主循环改造

```rust
pub async fn start(&self) {
    *self.is_running = true;
    tracing::info!("[Trader {}] Started", self.config.symbol);

    // 启动时尝试恢复状态
    if let Some(record) = self.executor.load_record(&self.config.symbol) {
        tracing::info!("[Trader {}] 已恢复状态: {:?}", self.config.symbol, record.trader_status);
    }

    while *self.is_running.read() {
        if let Some((signal, order_type)) = self.execute_once() {
            // 发送订单
            if let Err(e) = self.executor.send_order(&signal, order_type) {
                tracing::error!("[Trader {}] 下单失败: {}", self.config.symbol, e);
            }

            // 构建并保存记录
            let record = self.build_trade_record(&signal, order_type);
            if let Err(e) = self.executor.save_record(&record) {
                tracing::error!("[Trader {}] 保存记录失败: {}", self.config.symbol, e);
            }
        }
        sleep(Duration::from_millis(self.config.interval_ms)).await;
    }

    tracing::info!("[Trader {}] Stopped", self.config.symbol);
}
```

七、Engine 协程管理（心跳 + 自动重启）
================================================================

┌─────────────────────────────────────────────────────────────────┐
│  Engine::spawn(symbol)                                          │
│  1. 创建 Trader 实例                                            │
│  2. tokio::spawn(trader.start())                               │
│  3. 保存 JoinHandle 到 InstanceMap                              │
│  4. 记录启动时间                                                │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Monitor 协程（定期运行）                                        │
│  1. 遍历 InstanceMap                                            │
│  2. 检查每个 Trader 的 last_heartbeat                           │
│  3. 如果超时（> 30s）：                                         │
│     - 停止旧协程                                                │
│     - 重新 spawn 新协程                                         │
│     - 记录重启次数                                              │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Engine::stop(symbol)                                           │
│  1. 设置 Trader.is_running = false                             │
│  2. 等待 JoinHandle 返回                                        │
│  3. 从 InstanceMap 移除                                        │
└─────────────────────────────────────────────────────────────────┘

八、SQLite 表创建
================================================================

CREATE TABLE IF NOT EXISTS trade_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    interval_ms INTEGER DEFAULT 100,
    price TEXT,
    volatility REAL,
    market_status TEXT,
    exchange_position TEXT,
    local_position TEXT,
    trader_status TEXT,
    signal_json TEXT,
    confidence INTEGER,
    realized_pnl TEXT,
    unrealized_pnl TEXT,
    available_balance TEXT,
    used_margin TEXT,
    order_type INTEGER,
    order_direction TEXT,
    order_quantity TEXT,
    order_result TEXT,
    order_timestamp INTEGER,
    check_passed INTEGER DEFAULT 0,
    signal_passed INTEGER DEFAULT 0,
    price_check_passed INTEGER DEFAULT 0,
    risk_check_passed INTEGER DEFAULT 0,
    lock_acquired INTEGER DEFAULT 0,
    order_executed INTEGER DEFAULT 0,
    record_saved INTEGER DEFAULT 0,
    UNIQUE(symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_symbol_timestamp ON trade_records(symbol, timestamp DESC);

九、文件改动清单
================================================================

新建：
  crates/d_checktable/src/h_15m/
    ├── executor.rs     # 下单 + 存储 + 恢复

修改：
  crates/d_checktable/src/h_15m/
    ├── mod.rs          # 导出 Executor
    ├── trader.rs       # 整合 executor + TradeRecord
  crates/f_engine/src/core/
    ├── strategy_loop.rs # Engine spawn/stop/monitor

================================================================
End of Document
================================================================
