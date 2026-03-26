================================================================
品种协程自循环设计方案
================================================================
Author: 软件架构师
Date: 2026-03-27
Status: 待实现
================================================================

一、核心设计
================================================================

一张表贯穿始终 = TradeRecord

存储、传递、检查都用这一张表。

二、架构图
================================================================

┌─────────────────────────────────────────────────────────────┐
│  Engine                                                   │
│  ├── InstanceMap: HashMap<symbol, InstanceHandle>        │
│  │   ├── JoinHandle                                      │
│  │   ├── last_heartbeat                                 │
│  │   └── restart_count                                   │
│  ├── spawn(symbol)                                      │
│  ├── Monitor 协程                                       │
│  └── stop(symbol)                                       │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  Instance.loop()                                          │
│  loop:                                                   │
│    record = TradeRecord.new(symbol)                      │
│    record.market = get_market()                          │
│    record.position = get_position()                       │
│    record.signal = trader.execute()                      │
│    record.check_table = check()                          │
│    if record.check_table.all_passed()                    │
│        record.order = execute()                          │
│    record.saved = save()                                 │
│    record.save() → SQLite                               │
└─────────────────────────────────────────────────────────────┘

三、TradeRecord (唯一的一张表)
================================================================

┌─────────────────────────────────────────────────────────────┐
│  TradeRecord                                             │
├─────────────────────────────────────────────────────────────┤
│  基础信息                                                │
│  ├── symbol: String                                      │
│  ├── timestamp: i64                                      │
│  └── interval_ms: u64                                    │
├─────────────────────────────────────────────────────────────┤
│  行情快照                                                │
│  ├── price: Decimal                                       │
│  ├── volatility: f64                                     │
│  └── market_status                                       │
├─────────────────────────────────────────────────────────────┤
│  持仓快照                                                │
│  ├── long_qty / long_price                              │
│  ├── short_qty / short_price                            │
│  ├── local_orders_history (本地记录)                     │
│  └── last_saved_at                                       │
├─────────────────────────────────────────────────────────────┤
│  策略状态                                                │
│  ├── trader_status                                       │
│  ├── signal                                             │
│  └── confidence                                         │
├─────────────────────────────────────────────────────────────┤
│  账户状态                                                │
│  ├── available                                          │
│  ├── used_margin                                        │
│  └── unrealized_pnl                                     │
├─────────────────────────────────────────────────────────────┤
│  订单执行                                                │
│  ├── order_type                                         │
│  ├── direction                                          │
│  ├── quantity / price                                   │
│  └── result / reason                                    │
├─────────────────────────────────────────────────────────────┤
│  检查表 (CheckTable)                                     │
│  ├── signal_passed: bool      // 有有效信号             │
│  ├── price_check_passed: bool // 价格变化符合条件       │
│  ├── pre_check_passed: bool  // 风控预检通过           │
│  ├── lock_acquired: bool      // 抢锁成功               │
│  ├── risk_check_passed: bool  // 风控二检通过           │
│  ├── order_executed: bool     // 下单成功               │
│  └── saved: bool             // 保存成功               │
└─────────────────────────────────────────────────────────────┘

四、传递流程
================================================================

TradeRecord 贯穿始终，每一步都填入这张表：

loop:
  1. TradeRecord.new() → 创建空表
  2. record.market = get_market()
  3. record.position = get_position()
  4. record.signal = trader.execute()
  5. record.check_signal()
  6. record.check_price()
  7. record.check_pre_risk()
  8. if all passed:
       record.lock()
       record.check_risk()
       record.order_execute()
  9. record.save() → SQLite

五、Engine Monitor
================================================================

Monitor 协程:
  - 检测心跳超时 → 重启 Instance
  - 超过最大重启次数 → 标记 Dead

六、文件结构
================================================================

新建:
  crates/f_engine/src/core/
    └── strategy_loop.rs

修改:
  crates/f_engine/src/core/
    └── mod.rs

================================================================
End of Document
================================================================
