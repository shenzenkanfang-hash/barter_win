================================================================
品种协程自循环设计方案
================================================================
Author: 软件架构师
Date: 2026-03-27
Status: 待实现
================================================================

一、核心设计
================================================================

一个品种 = 一个协程 = 一个 Instance

Engine 管 spawn/stop/监控/重启，品种自己 loop。

二、架构图
================================================================

┌─────────────────────────────────────────────────────────────┐
│  Engine                                                   │
│  ├── InstanceMap: HashMap<symbol, InstanceHandle>        │
│  │   ├── JoinHandle         // 重启用                      │
│  │   ├── last_heartbeat     // 最后心跳时间               │
│  │   └── restart_count      // 重启次数                   │
│  │                                                       │
│  ├── spawn(symbol) → 创建 Instance + 注册                │
│  ├── Monitor 协程 → 检测心跳，超时自动重启                │
│  └── stop(symbol) → 停止 + 移除注册                      │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  Instance                                                  │
│  ├── symbol: String                                      │
│  ├── running: bool                                       │
│  ├── interval_ms: u64                                    │
│  ├── last_heartbeat: AtomicI64                           │
│  ├── trader: Trader                                      │
│  │                                                       │
│  ├── loop()                                              │
│  │   ├── 获取行情                                        │
│  │   ├── 获取持仓                                        │
│  │   ├── 计算信号                                        │
│  │   ├── CheckTable 检查                                │
│  │   ├── 下单                                            │
│  │   ├── 保存                                            │
│  │   ├── 更新心跳                                        │
│  │   └── 等 interval                                    │
│  │                                                       │
│  └── 退出条件: running = false                           │
└─────────────────────────────────────────────────────────────┘

三、传递流程
================================================================

┌─────────────────────────────────────────────────────────────┐
│  TradeRecord (完整交易记录)                              │
├─────────────────────────────────────────────────────────────┤
│  基础信息                                                  │
│  ├── symbol: String          // 交易对                   │
│  ├── timestamp: i64          // 时间戳                    │
│  └── interval_ms: u64        // 循环间隔                 │
├─────────────────────────────────────────────────────────────┤
│  行情快照                                                  │
│  ├── price: Decimal          // 当前价格                 │
│  ├── volatility: f64          // 波动率                   │
│  └── market_status           // 市场状态 (Pin/Trend/Range)│
├─────────────────────────────────────────────────────────────┤
│  持仓快照                                                  │
│  ├── 交易所持仓                                            │
│  │   ├── long_qty                                    │
│  │   ├── long_price                                  │
│  │   ├── short_qty                                   │
│  │   └── short_price                                  │
│  └── 本地持仓记录 (SQLite)                               │
│      ├── local_open_price                              │
│      ├── local_open_qty                                 │
│      ├── local_orders_history                           │
│      └── last_saved_at                                  │
├─────────────────────────────────────────────────────────────┤
│  策略状态                                                  │
│  ├── trader_status           // (Initial/LongFirstOpen...) │
│  ├── signal                 // 信号                      │
│  └── confidence: u8          // 信心度                   │
├─────────────────────────────────────────────────────────────┤
│  账户状态                                                  │
│  ├── available: Decimal      // 可用资金                 │
│  ├── used_margin: Decimal    // 已用保证金               │
│  └── unrealized_pnl         // 未实现盈亏               │
├─────────────────────────────────────────────────────────────┤
│  订单执行                                                  │
│  ├── order_type             // (Open/Add/Close)         │
│  ├── direction              // (Long/Short)              │
│  ├── quantity: Decimal      // 数量                     │
│  ├── price: Decimal         // 执行价格                 │
│  ├── result                // (Success/Failed/Rejected)│
│  └── reason: String        // 失败原因                 │
├─────────────────────────────────────────────────────────────┤
│  风控结果                                                  │
│  ├── risk_passed: bool     // 是否通过                  │
│  └── risk_reason: String   // 拒绝原因                 │
└─────────────────────────────────────────────────────────────┘

四、CheckTable (检查表)
================================================================

┌─────────────────────────────────────────────────────────────┐
│  CheckTable (检查表) - 每步 yes/no                       │
├─────────────────────────────────────────────────────────────┤
│  第一步: 预检 (Lock 外)                                │
│  ├── signal_passed: bool         // 有有效信号             │
│  ├── price_check_passed: bool   // 价格变化符合条件       │
│  └── pre_check_passed: bool     // 风控预检通过           │
├─────────────────────────────────────────────────────────────┤
│  第二步: 执行 (Lock 内)                                 │
│  ├── lock_acquired: bool        // 抢锁成功               │
│  ├── risk_check_passed: bool   // 风控二检通过           │
│  ├── order_executed: bool       // 下单成功               │
│  └── saved: bool               // 保存成功                │
└─────────────────────────────────────────────────────────────┘

五、Instance.loop() 流程
================================================================

loop:
  1. 检查 running = true？
     - false → 退出 loop
  2. 获取行情 → 填入 TradeRecord.market
  3. 获取持仓 → 填入 TradeRecord.position
  4. Trader.execute() → 填入 TradeRecord.strategy
  5. CheckTable 第一步 (预检):
     - signal_check
     - price_check
     - pre_risk_check
     - 任一失败 → 保存 CheckTable → 等 interval
  6. CheckTable 第二步 (执行):
     - lock_acquire
     - risk_check
     - order_execute
     - save
  7. 更新心跳 (last_heartbeat = now)
  8. 等 interval → 回第1步

六、Engine Monitor 机制
================================================================

Monitor 协程 (每秒执行):
  1. 遍历所有 InstanceHandle
  2. 检查 now - last_heartbeat > 超时阈值？
     - 是 → 重启 Instance
  3. 检查 restart_count >= 最大次数？
     - 是 → 标记为 Dead，不再重启

重启流程:
  1. abort(JoinHandle)
  2. spawn 新的协程
  3. 更新 JoinHandle + 重置 heartbeat
  4. restart_count++

七、文件结构
================================================================

新建:
  crates/f_engine/src/core/
    └── strategy_loop.rs      # Instance + Engine + Monitor

修改:
  crates/f_engine/src/core/
    └── mod.rs              # 导出

================================================================
End of Document
================================================================
