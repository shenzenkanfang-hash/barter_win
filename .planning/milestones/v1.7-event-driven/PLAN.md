# 交易系统 事件驱动架构改造 PLAN

## 改造目标
移除全链路 `sleep` 轮询，实现：
**Tick 驱动 → 数据更新 → 指标计算 → 策略决策 → 风控下单**
无阻塞、低延迟、可重现、生产级事件驱动架构

---

## 核心设计决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 多策略并行 | 每品种一个事件循环 | 单品种低延迟、多品种互不阻塞 |
| 策略执行频率 | 每 N 个 tick 可配置 | 避免算力浪费、适配不同策略频率 |
| 通道背压 | mpsc::channel(1024) | Tokio 最佳实践，防止溢出 |
| 优雅关闭 | watch::channel | 支持停止信号 |
| 并发安全 | DashMap 互斥锁 | 单品种只运行一个策略任务 |

---

## 阶段一：事件循环核心框架

### 文件
`src/sandbox_main.rs`

### 改动
1. 定义统一事件 `Tick` 作为全链路驱动源
2. 创建 `TradingLoop` 核心结构体
3. 实现单品种独立事件循环（保留并行）
4. 集成优雅关闭 + 策略互斥

### 核心代码
```rust
// 新增 imports
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, DashMap};
use tokio::sync::RwLock as TokioRwLock;

// 事件驱动核心结构
struct TradingLoop {
    symbol: String,
    data_feeder: Arc<DataFeeder>,
    indicator_cache: Arc<IndicatorCache>,
    gateway: Arc<ShadowBinanceGateway>,
    risk_checker: Arc<ShadowRiskChecker>,
    engine: Arc<TradeManagerEngine>,
    strategy_locks: Arc<DashMap<String, ()>>,
    trigger_interval: u64,
    tick_count: AtomicU64,
}

impl TradingLoop {
    async fn run(
        self: Arc<Self>,
        mut tick_rx: mpsc::Receiver<Tick>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                Some(tick) = tick_rx.recv() => {
                    let this = Arc::clone(&self);
                    this.on_tick(tick).await;
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("[TradingLoop] 收到关闭信号，退出循环");
                        break;
                    }
                }
            }
        }
    }

    async fn on_tick(&self, tick: Tick) {
        let symbol = tick.symbol.clone();

        // 1. 写入数据源
        self.data_feeder.push_tick(tick.clone());

        // 2. 增量计算指标
        self.indicator_cache.update(&tick).await;

        // 3. 波动率实时检查（事件驱动）
        self.check_volatility(&tick).await;

        // 4. 可配置间隔执行策略
        let count = self.tick_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count % self.trigger_interval == 0 {
            self.run_strategy(&tick).await;
        }
    }

    async fn check_volatility(&self, tick: &Tick) {
        let volatility = self.indicator_cache.get_volatility(&tick.symbol).await;
        let threshold = dec!(0.02);

        if volatility > threshold && !self.engine.has_task(&tick.symbol).await {
            if self.strategy_locks.contains_key(&tick.symbol) {
                return;
            }
            self.engine.spawn_strategy_task(tick.symbol.clone()).await;
        }
    }

    async fn run_strategy(&self, tick: &Tick) {
        // 策略逻辑保持不变，仅改触发方式
    }
}
```

---

## 阶段二：数据注入层改造

### 文件
`crates/mock 组件/src/historical_replay/`

### 改动
1. 移除所有 `tokio::time::sleep`
2. 改为 `mpsc::channel` 异步推送 Tick
3. 保留历史回放速率控制（使用调度而非 sleep）

### 核心代码
```rust
// 当前问题代码 (L130-171):
tokio::spawn(async move {
    while let Some(tick) = generator.next() {
        data_feeder.push_tick(tick.clone());
        indicator_cache.update(&tick);
        tokio::time::sleep(Duration::from_millis(50)).await; // 删除
    }
});

// 改造后:
let (tick_tx, tick_rx) = mpsc::channel(1024);
let generator = StreamTickGenerator::new(replay_config);

tokio::spawn(async move {
    while let Some(tick) = generator.next() {
        if tick_tx.send(tick).await.is_err() {
            break; // 消费者已关闭
        }
    }
});
```

---

## 阶段三：引擎层改造

### 文件
`src/sandbox_main.rs` / `TradeManagerEngine`

### 改动
1. 移除引擎主循环 `sleep(1s)`
2. 波动率检查移入 `on_tick` 事件
3. 策略任务由事件触发，而非轮询
4. 保留原策略逻辑，无破坏性改动

### 核心代码
```rust
// 当前问题代码 (L196-254):
loop {
    let volatility = engine.get_volatility(&symbol).await;
    if volatility > threshold && !engine.has_task(&symbol).await {
        engine.spawn_strategy_task(symbol.clone(), 50).await;
    }
    sleep(Duration::from_secs(1)).await;
}

// 改造后: 波动率检查集成到 TradingLoop::on_tick
// 无独立循环，无 sleep
```

---

## 阶段四：验证与测试

### 编译检查
```bash
cargo check --bin sandbox
```

### 功能验证清单
- [ ] Tick 注入成功
- [ ] 指标增量计算正常
- [ ] 波动率事件触发
- [ ] 策略任务正常启动
- [ ] 风控下单流程完整
- [ ] 支持优雅关闭

### 性能指标
- 单 Tick 处理耗时 < 10ms
- 无数据竞争、无死锁
- 多品种并行无阻塞

---

## 实施顺序

| 步骤 | 内容 | 文件 |
|------|------|------|
| 1 | 创建全局 mpsc 通道 + 关闭信号 watch | `sandbox_main.rs` |
| 2 | 实现 `TradingLoop` 完整结构 | `sandbox_main.rs` |
| 3 | 改造数据注入层，移除 sleep | `sandbox_main.rs` |
| 4 | 引擎逻辑移入事件循环 | `sandbox_main.rs` |
| 5 | 编译通过 + 修复类型/借用问题 | - |
| 6 | 运行沙盒测试全流程 | - |
| 7 | 性能压测验证延迟达标 | - |

---

## 架构优势总结

✅ 全链路无 `sleep` 轮询
✅ 事件驱动，低延迟
✅ 多品种并行不阻塞
✅ 可配置策略执行频率
✅ 生产级背压 + 优雅关闭
✅ 无数据竞争，可重现
