---
title: 量化交易系统架构优化建议
author: 工作流程优化器 (Workflow Optimizer)
created: 2026-03-20
updated: 2026-03-20
role: 软件架构师
status: 已评审
review_date: 2026-03-20
reviewer: 软件架构师
related_docs:
  - docs/2026-03-20-module-deep-analysis.md
---

================================================================================
量化交易系统 Rust 重构 - 架构优化建议
================================================================================

文档目的
================================================================================

本文档识别当前架构的瓶颈和问题，提出系统性的优化方案。
目标：兼顾低延迟 (< 1ms Tick处理) + 高吞吐 (10+ 品种并行)。

现状基线
================================================================================

| 指标 | 当前状态 | 目标状态 |
|------|----------|----------|
| Tick 处理延迟 | ~5-10ms (串行) | < 1ms |
| 品种并行能力 | 单品种 | 10+ 品种 |
| 多线程安全 | AccountPool 无锁 | 线程安全 |
| CPU 利用率 | ~30% (串行) | > 80% |

================================================================================
第一部分：现状问题分析
================================================================================

1.1 Tick 处理瓶颈
--------------------------------------------------------------------------------

### 问题描述

当前 `TradingEngine::on_tick()` 串行执行所有操作：

```rust
pub async fn on_tick(&mut self, tick: &Tick) {
    // 1. 更新 K线 (串行)
    let completed_1m = self.kline_1m.update(tick);
    let completed_1d = self.kline_1d.update(tick);

    // 2. 更新指标 (串行)
    self.update_indicators(tick.price);

    // 3. 风控预检 (串行)
    self.pre_trade_check(tick);

    // 4. K线完成处理 (串行)
    if let Some(kline) = completed_1m {
        self.on_kline_completed(&kline);
    }
    // ...
}
```

### 影响

| 问题 | 后果 |
|------|------|
| K线串行更新 | 2个K线串行，延迟叠加 |
| 指标串行计算 | EMA→RSI 串行，无法利用多核 |
| 风控串行检查 | 阻塞整个流程 |

### 优化方向

- K线并行更新 (利用多核)
- 指标计算流水线化
- 风控异步化

--------------------------------------------------------------------------------

1.2 多品种扩展瓶颈
--------------------------------------------------------------------------------

### 问题描述

当前架构：单 `TradingEngine` 实例处理单品种

```rust
pub struct TradingEngine {
    // 市场数据
    market_stream: Box<dyn MarketStream>,

    // K线合成器 (仅单个品种)
    kline_1m: KLineSynthesizer,
    kline_1d: KLineSynthesizer,

    // 当前交易对
    symbol: String,
}
```

### 影响

| 问题 | 后果 |
|------|------|
| 单品种处理 | N 个品种 = N 倍时间 |
| 共享状态竞争 | 多线程时需要锁 |
| 线性扩展 | 10 品种 = 10x 延迟 |

### 优化方向

- 每品种独立流水线
- 无锁设计 (复制而非共享)
- 消息传递代替共享状态

--------------------------------------------------------------------------------

1.3 资金管理线程安全问题
--------------------------------------------------------------------------------

### 问题描述

`AccountPool` 实现中没有任何同步机制：

```rust
pub fn can_trade(&self, required_margin: Decimal) -> bool {
    if self.account.circuit_state == CircuitBreakerState::Full {
        return false;
    }
    // ...
    self.account.available >= required_margin  // 无锁读取
}

pub fn freeze(&mut self, amount: Decimal) -> Result<(), String> {
    if amount > self.account.available {
        return Err("可用资金不足".to_string());
    }
    self.account.available -= amount;  // 无锁写入
    // ...
}
```

### 影响

| 问题 | 后果 |
|------|------|
| 无锁读写 | 多线程数据竞争 |
| 非原子操作 | 余额可能变为负数 |
| 熔断状态不一致 | 可能超开仓位 |

### 优化方向

- 使用 `parking_lot::RwLock` 保护 AccountPool
- 操作原子化
- 读写分离 (读多写少场景)

--------------------------------------------------------------------------------

1.4 指标计算未充分利用 PipelineForm
--------------------------------------------------------------------------------

### 问题描述

设计文档中的 `PipelineForm` 应贯穿全程，但实际实现直接传参：

```rust
// 设计承诺
fn update_indicators(&mut self, price: Decimal) {
    // PipelineForm 未被使用
    let ema_f = self.ema_fast.calculate(price);
    let ema_s = self.ema_slow.calculate(price);
    let _rsi_value = self.rsi.calculate(ema_f - ema_s);
}
```

### 影响

| 问题 | 后果 |
|------|------|
| 中间结果丢失 | 无法调试/回放 |
| 扩展性差 | 新增指标需要改接口 |
| 可追溯性差 | 无法追踪计算链 |

### 优化方向

- 强制 PipelineForm 贯穿
- 每层产出记录到表单
- 支持回放和调试

================================================================================
第二部分：架构优化方案
================================================================================

2.1 优化方案总览
--------------------------------------------------------------------------------

### 方案选择

**推荐方案：渐进式重构**

| 阶段 | 改动范围 | 风险 | 收益 |
|------|----------|------|------|
| Phase A: 线程安全 | AccountPool + RwLock | 低 | 高 |
| Phase B: 并行化 | 多品种流水线 | 中 | 高 |
| Phase C: PipelineForm | 重构数据流 | 中 | 中 |

--------------------------------------------------------------------------------

2.2 Phase A: 线程安全加固
--------------------------------------------------------------------------------

### 目标

确保多线程环境下资金管理的正确性。

### 改动点

#### 1. AccountPool 添加锁

```rust
use parking_lot::RwLock;

pub struct AccountPool {
    // 使用 RwLock 保护账户数据
    account: RwLock<AccountInfo>,
    // ... 其他字段
}

impl AccountPool {
    pub fn available(&self) -> Decimal {
        self.account.read().available
    }

    pub fn freeze(&self, amount: Decimal) -> Result<(), String> {
        let mut guard = self.account.write();
        if amount > guard.available {
            return Err("可用资金不足".to_string());
        }
        guard.available -= amount;
        guard.frozen += amount;
        Ok(())
    }
}
```

#### 2. StrategyPool 同样添加锁

```rust
pub struct StrategyPool {
    allocations: RwLock<HashMap<String, StrategyAllocation>>,
    // ...
}
```

#### 3. TradingEngine 锁协调

```
风控预检 (读锁) ──────────────────────────────────────┐
                                                        │
                                                        ▼
订单执行 ──> 获取写锁 ──> 扣款 ──> 更新 ──> 释放写锁 ◄──┘
                                ▲
                                │
                              预检通过
```

### 预期效果

| 指标 | 优化前 | 优化后 |
|------|--------|--------|
| 线程安全 | ❌ | ✅ |
| 死锁风险 | 无 | 低 (单锁设计) |
| 性能影响 | - | < 5% |

--------------------------------------------------------------------------------

2.3 Phase B: 多品种并行流水线
--------------------------------------------------------------------------------

### 目标

支持 10+ 品种并行处理，线性扩展。

### 核心设计

#### 0. 共同注册表 (Symbol Registry)

**设计决策**: 各策略/流水线通过注册表声明关注哪些品种，数据层主动推送或策略层主动拉取。

```rust
/// 品种注册表 - 管理所有活跃品种和数据订阅关系
pub struct SymbolRegistry {
    /// 品种元数据: symbol -> SymbolMeta
    symbols: RwLock<HashMap<String, SymbolMeta>>,
    /// 订阅者: symbol -> Vec<PipelineId>
    subscribers: RwLock<HashMap<String, Vec<PipelineId>>>,
}

impl SymbolRegistry {
    /// 策略注册时声明关注品种
    pub fn subscribe(&self, pipeline_id: PipelineId, symbols: Vec<String>) {
        for symbol in symbols {
            self.subscribers.write().entry(symbol)
                .or_default()
                .push(pipeline_id.clone());
        }
    }

    /// 获取某品种的所有订阅者
    pub fn get_subscribers(&self, symbol: &str) -> Vec<PipelineId> {
        self.subscribers.read()
            .get(symbol)
            .cloned()
            .unwrap_or_default()
    }
}
```

**两种数据获取模式**:
1. **推模式 (Push)**: 数据层通过注册表找到订阅者，主动推送 Tick
2. **拉模式 (Pull)**: 流水线通过注册表找到数据层，主动拉取

#### 1. SymbolPipeline (品种流水线)

```rust
/// 品种流水线 - 每个品种独立运行
pub struct SymbolPipeline {
    symbol: String,
    // 品种级状态 (无共享)
    kline_1m: KLineSynthesizer,
    kline_1d: KLineSynthesizer,
    indicators: IndicatorSet,
    // Channel 传递结果
    sender: mpsc::Sender<PipelineForm>,
}

impl SymbolPipeline {
    pub async fn run(&mut self, mut receiver: mpsc::Receiver<Tick>) {
        while let Some(tick) = receiver.recv().await {
            // 1. K线更新
            let form = self.update_market_data(&tick);

            // 2. 指标计算
            let form = self.update_indicators(form);

            // 3. 发送到 Check 表
            let _ = self.sender.send(form).await;
        }
    }
}
```

#### 2. PipelineOrchestrator (编排器)

```rust
/// 流水线编排器 - 管理所有品种流水线
pub struct PipelineOrchestrator {
    pipelines: FnvHashMap<String, SymbolPipeline>,
    check_table: Arc<RwLock<CheckTable>>,
    risk_checker: Arc<RiskPreChecker>,
}

impl PipelineOrchestrator {
    pub fn new(symbols: Vec<String>) -> Self {
        let mut orchestrator = Self {
            pipelines: FnvHashMap::default(),
            check_table: Arc::new(RwLock::new(CheckTable::new())),
            risk_checker: Arc::new(RiskPreChecker::new()),
        };

        // 为每个品种创建流水线
        for symbol in symbols {
            orchestrator.spawn_pipeline(symbol);
        }

        orchestrator
    }

    fn spawn_pipeline(&mut self, symbol: String) {
        let (tx, rx) = mpsc::channel(1000);
        let pipeline = SymbolPipeline::new(symbol.clone(), rx);

        self.pipelines.insert(symbol, pipeline);
    }
}
```

#### 3. 并行执行示意

```
                    ┌─────────────────────────────────────────┐
                    │        PipelineOrchestrator              │
                    │                                          │
                    │  ┌─────────┐ ┌─────────┐ ┌─────────┐   │
Tick ──────────────►│  │ BTC     │ │ ETH     │ │ SOL     │   │
                    │  │Pipeline │ │Pipeline │ │Pipeline │   │
                    │  └────┬────┘ └────┬────┘ └────┬────┘   │
                    └────────┼───────────┼───────────┼────────┘
                             │           │           │
                             ▼           ▼           ▼
                        ┌─────────────────────────────────┐
                        │          Check Table             │
                        │   (Arc<RwLock<CheckTable>>)     │
                        └─────────────────────────────────┘
                             │
                             ▼
                        ┌─────────────────────────────────┐
                        │     Position Decision Layer       │
                        │     (仓位决策 + 风控预检)         │
                        └─────────────────────────────────┘
                             │
                             ▼
                        ┌─────────────────────────────────┐
                        │     Order Execution Layer         │
                        │        (Global Lock)             │
                        └─────────────────────────────────┘
```

### 预期效果

| 指标 | 优化前 | 优化后 |
|------|--------|--------|
| 品种扩展 | O(N) | O(1) (独立流水线) |
| CPU 利用率 | ~30% | > 80% |
| Tick 延迟 | ~10ms | < 1ms (独立) |

--------------------------------------------------------------------------------

2.4 Phase C: PipelineForm 贯穿全程
--------------------------------------------------------------------------------

### 目标

实现设计文档中的承诺：全程表单追踪。

### 设计

#### 1. PipelineForm 结构 (增强)

```rust
/// 全流程表单 - 贯穿所有层级
#[derive(Debug, Clone)]
pub struct PipelineForm {
    // 基础信息
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub round_id: u64,

    // 市场数据层产出
    pub tick_price: Decimal,
    pub kline_1m: Option<KLineData>,
    pub kline_1d: Option<KLineData>,

    // 指标层产出
    pub ema: EmaOutput,
    pub rsi: RsiOutput,
    pub pine_color: PineColorOutput,
    pub tr_ratio: TrRatioOutput,
    pub price_position: PricePositionOutput,

    // 策略层产出
    pub signal: Option<Signal>,
    pub confidence: u8,

    // 风控层
    pub risk_check_passed: bool,
    pub reject_reason: Option<String>,

    // 元数据
    pub stage: PipelineStage,
}

#[derive(Debug, Clone)]
pub enum PipelineStage {
    MarketData,
    Indicators,
    Strategy,
    RiskPreCheck,
    RiskReCheck,
    OrderExecution,
    Completed,
}
```

#### 2. 流水线处理函数签名

```rust
/// 统一接口 - 所有流水线阶段
trait PipelineStageHandler {
    fn process(&self, form: PipelineForm) -> PipelineForm;
}

// 示例
struct IndicatorStage {
    ema_fast: EMA,
    ema_slow: EMA,
    rsi: RSI,
}

impl PipelineStageHandler for IndicatorStage {
    fn process(&self, mut form: PipelineForm) -> PipelineForm {
        // 市场数据 -> 指标
        let price = form.tick_price;
        form.ema = EmaOutput {
            fast: self.ema_fast.calculate(price),
            slow: self.ema_slow.calculate(price),
        };
        form.rsi = RsiOutput {
            value: self.rsi.calculate(form.ema.diff()),
        };
        form.stage = PipelineStage::Indicators;
        form
    }
}
```

#### 3. 日志追踪

```rust
// 每个阶段记录日志
fn process(&self, mut form: PipelineForm) -> PipelineForm {
    let stage_start = Instant::now();
    trace!(
        symbol = %form.symbol,
        stage = ?form.stage,
        round_id = form.round_id,
        "Stage started"
    );

    form = self.handler.process(form);

    trace!(
        symbol = %form.symbol,
        stage = ?form.stage,
        elapsed_ms = stage_start.elapsed().as_millis(),
        "Stage completed"
    );

    form
}
```

### 预期效果

| 指标 | 优化前 | 优化后 |
|------|--------|--------|
| 可追溯性 | 低 | 高 |
| 调试难度 | 高 | 低 |
| 中间结果 | 丢失 | 完整保存 |

================================================================================
第三部分：实施路线图
================================================================================

3.1 优先级矩阵
--------------------------------------------------------------------------------

| 优先级 | 问题 | 风险 | 收益 | 推荐方案 |
|--------|------|------|------|----------|
| P0 | 资金管理线程安全 | 高 | 高 | Phase A |
| P1 | 多品种扩展 | 中 | 高 | Phase B |
| P2 | PipelineForm 贯穿 | 低 | 中 | Phase C |

**推荐顺序**: P0 → P1 → P2

--------------------------------------------------------------------------------

3.2 Phase A 实施计划
--------------------------------------------------------------------------------

### 步骤 1: 添加依赖

```toml
# Cargo.toml
parking_lot = "0.12"
```

### 步骤 2: 修改 AccountPool

1. 添加 `RwLock` 字段
2. 所有读取方法改为 `read()`
3. 所有写入方法改为 `write()`
4. 添加单元测试 (多线程场景)

### 步骤 3: 修改 StrategyPool

同上

### 步骤 4: 修改 TradingEngine

```rust
pub struct TradingEngine {
    // 锁保护的组件
    account_pool: Arc<RwLock<AccountPool>>,
    strategy_pool: Arc<RwLock<StrategyPool>>,
    // ...
}

impl TradingEngine {
    pub async fn execute_order(&self, order: OrderRequest) -> Result<(), EngineError> {
        // 预检 (读锁)
        {
            let pool = self.account_pool.read();
            pool.can_trade(order_value)?;
        }

        // 执行 (写锁)
        {
            let mut pool = self.account_pool.write();
            pool.freeze(order_value)?;
        }

        // ...
    }
}
```

### 验收标准

| 测试项 | 标准 |
|--------|------|
| 单元测试 | 10 并发线程安全 |
| 死锁测试 | 无死锁 |
| 性能测试 | 延迟增加 < 10% |

--------------------------------------------------------------------------------

3.3 Phase B 实施计划
--------------------------------------------------------------------------------

### 步骤 1: 提取 SymbolPipeline

```rust
// 从 TradingEngine 提取品种逻辑
pub struct SymbolPipeline {
    symbol: String,
    kline_1m: KLineSynthesizer,
    kline_1d: KLineSynthesizer,
    indicators: IndicatorSet,
}
```

### 步骤 2: 创建 PipelineOrchestrator

```rust
pub struct PipelineOrchestrator {
    pipelines: FnvHashMap<String, SymbolPipeline>,
    check_table: Arc<RwLock<CheckTable>>,
}
```

### 步骤 3: Tokio Task 并行化

```rust
impl PipelineOrchestrator {
    pub async fn run(&self, ticks: impl Stream<Item = Tick>) {
        let tick_by_symbol = ticks.map(|tick| (tick.symbol.clone(), tick));

        for (_, pipeline) in &self.pipelines {
            let symbol = pipeline.symbol.clone();
            let ticker_stream = tick_by_symbol.clone().filter(|(s, _)| s == &symbol);

            tokio::spawn(async move {
                pipeline.run(ticker_stream).await;
            });
        }
    }
}
```

### 验收标准

| 测试项 | 标准 |
|--------|------|
| 10 品种并行 | 延迟 < 2ms/品种 |
| CPU 利用率 | > 70% |
| 线性扩展 | 品种增加不超线性 |

--------------------------------------------------------------------------------

3.4 Phase C 实施计划
--------------------------------------------------------------------------------

### 步骤 1: 定义 PipelineForm

```rust
// 新建 pipeline_form.rs
#[derive(Debug, Clone)]
pub struct PipelineForm {
    // 见 2.4 节设计
}
```

### 步骤 2: 实现 Stage Handler Trait

```rust
pub trait PipelineStageHandler {
    fn process(&self, form: PipelineForm) -> PipelineForm;
    fn stage_name(&self) -> &'static str;
}
```

### 步骤 3: 重构 PipelineForm 处理链

```rust
impl SymbolPipeline {
    fn process_tick(&self, tick: &Tick) -> PipelineForm {
        let mut form = PipelineForm::new(tick);

        // 流水线处理
        for stage in &self.stages {
            form = stage.process(form);
        }

        form
    }
}
```

### 验收标准

| 测试项 | 标准 |
|--------|------|
| 回放测试 | 可从任意阶段恢复 |
| 日志完整性 | 每阶段有记录 |
| 性能开销 | < 0.1ms |

================================================================================
第四部分：模块问题清单
================================================================================

本文档第三部分的模块深度分析报告发现了以下具体问题，按优先级整理。

4.1 P0 - 线程安全问题 (致命)
--------------------------------------------------------------------------------

| 模块 | 文件 | 问题 | 风险等级 |
|------|------|------|----------|
| AccountPool | engine/src/account_pool.rs | 无锁保护 can_trade/freeze/update_equity | 🔴 致命 |
| StrategyPool | engine/src/strategy_pool.rs | reserve_margin 非原子操作 | 🔴 致命 |
| OrderCheck | engine/src/order_check.rs | reservations HashMap 并发写入 | 🔴 致命 |
| PnlManager | engine/src/pnl_manager.rs | unrealized_pnl HashMap 并发写入 | 🔴 致命 |
| PositionManager | engine/src/position_manager.rs | 多字段复合操作非原子 | 🔴 致命 |
| CheckTable | engine/src/check_table.rs | 多线程写入同一 HashMap | 🟡 中等 |

#### 问题详解

**AccountPool freeze 竞态**:
```
T1: 线程A 调用 freeze(1000)
T2: 线程A 检查 available >= 1000 ✅
T3: 线程B 调用 freeze(2000)
T4: 线程B 检查 available >= 2000 ✅ (基于A的旧值!)
T5: 线程A available -= 1000
T6: 线程B available -= 2000
结果: 总共扣减 3000，但初始值可能只有 2000
```

**StrategyPool reserve_margin 竞态**:
```
当前: get_mut → 检查 → 修改 非原子
风险: 并发调用可能导致超额预占
```

--------------------------------------------------------------------------------

4.2 P1 - 算法正确性问题
--------------------------------------------------------------------------------

| 模块 | 文件 | 问题 | 严重程度 |
|------|------|------|----------|
| RSI | indicator/src/rsi.rs | 非标准 Wilder 平滑算法 | 🔴 高 |
| EMA | indicator/src/ema.rs | 初始值直接赋值，非 SMA | 🟡 中 |
| PineColor | indicator/src/pine_color.rs | 阈值硬编码 70/30 | 🟢 低 |

#### RSI 算法问题详解

**当前实现 (第1次)**:
```rust
if self.avg_loss.is_zero() {
    self.avg_gain = gain;  // ❌ 直接赋值
    self.avg_loss = loss;
}
```

**标准 RSI (Wilder平滑)**:
```rust
// 第1次: 使用 SMA 初始化
avg_gain = sum(gains[0:period]) / period
avg_loss = sum(losses[0:period]) / period

// 第N次:
avg = (avg_prev * (period-1) + current) / period
```

**影响**: 启动阶段 RSI 值不准确，可能导致错误交易信号。

--------------------------------------------------------------------------------

4.3 P2 - 性能问题
--------------------------------------------------------------------------------

| 模块 | 文件 | 问题 | 影响 |
|------|------|------|------|
| KLineSynthesizer | market/src/kline.rs | clone 返回值 | 内存分配开销 |
| PnlManager | engine/src/pnl_manager.rs | Vec.contains O(n) | 品种多时变慢 |
| engine.rs | engine/src/engine.rs | 串行 on_tick | CPU 利用率低 |

#### KLineSynthesizer clone 问题

```rust
// 当前: 每次K线完成都克隆
Some(kline) => {
    let completed = kline.clone();  // 🔴 克隆开销
    self.current = Some(self.new_kline(tick, kline_timestamp));
    Some(completed)
}
```

**优化**: 返回引用 `Option<&KLine>` 或使用 `std::mem::replace`

#### PnlManager Vec 问题

```rust
// 当前: O(n) 遍历
pub fn is_low_volatility(&self, symbol: &str) -> bool {
    self.low_volatility_symbols.contains(&symbol.to_string())  // Vec: O(n)
}

// 优化: O(1)
low_volatility_symbols: HashSet<String>,  // HashSet: O(1)
```

--------------------------------------------------------------------------------

4.4 P3 - 设计实现差异
--------------------------------------------------------------------------------

| 设计承诺 | 实际实现 | 差异 |
|----------|----------|------|
| AccountPool (熔断) | FundPool (无熔断) | types.rs vs account_pool.rs 两套设计 |
| PipelineForm | 直接传参 | 数据流无表单 |
| 双通道架构 | 单通道串行 | 未实现 |
| Lua 脚本预占 | 接口存在但未实现 | order_check.rs |

#### AccountPool vs FundPool

**设计决策**: 合并为一套设计，以 AccountPool 为基础，移除 FundPool。

**types.rs FundPool** (待移除):
```rust
pub struct FundPool {
    pub total_equity: Decimal,
    pub available: Decimal,
    pub positions_value: Decimal,
}
```

**engine/account_pool.rs AccountPool**:
```rust
pub struct AccountPool {
    account: RwLock<AccountInfo>,  // 有熔断状态
    circuit_threshold: Decimal,
    // ...
}
```

**问题**: 两套设计，功能重叠，应统一。

================================================================================
第五部分：风险评估
================================================================================

5.1 风险矩阵
--------------------------------------------------------------------------------

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| Phase A 死锁 | 中 | 高 | 单锁设计，避免嵌套 |
| Phase B 内存爆炸 | 低 | 高 | Channel 背压控制 |
| Phase C 性能开销 | 低 | 低 | 增量实现，按需启用 |
| 修复 RSI 算法 | 低 | 中 | 添加校准期 |

5.2 回滚计划
--------------------------------------------------------------------------------

| Phase | 回滚方案 |
|-------|----------|
| Phase A | 注释掉 RwLock，恢复无锁版本 |
| Phase B | 恢复单品种模式 |
| Phase C | 保持原参数传递 |
| RSI 修复 | 使用特征标记过渡期 |

================================================================================
第六部分：详细改动清单
================================================================================

6.1 Phase A 改动清单
--------------------------------------------------------------------------------

| 文件 | 改动 | 风险 |
|------|------|------|
| engine/src/account_pool.rs | 添加 RwLock<AccountInfo> | 低 |
| engine/src/strategy_pool.rs | 添加 RwLock<allocations> | 低 |
| engine/src/order_check.rs | 添加 RwLock<reservations> | 低 |
| engine/src/pnl_manager.rs | Vec → HashSet，添加 RwLock | 低 |
| engine/src/position_manager.rs | 添加 RwLock | 中 |
| engine/src/check_table.rs | 添加 RwLock | 低 |
| engine/src/engine.rs | 锁协调逻辑 | 中 |

6.2 Phase B 改动清单
--------------------------------------------------------------------------------

| 文件 | 改动 | 风险 |
|------|------|------|
| engine/src/symbol_pipeline.rs | 新建，提取品种逻辑 | 中 |
| engine/src/orchestrator.rs | 新建，多品种编排 | 中 |
| engine/src/engine.rs | 改为使用 Orchestrator | 高 |

6.3 Phase C 改动清单
--------------------------------------------------------------------------------

| 文件 | 改动 | 风险 |
|------|------|------|
| engine/src/pipeline_form.rs | 新建，定义表单结构 | 低 |
| engine/src/stage_handler.rs | 新建，trait 定义 | 低 |
| engine/src/indicator_stage.rs | 新建，指标处理 | 低 |
| engine/src/strategy_stage.rs | 新建，策略处理 | 低 |

================================================================================
架构师评审意见
================================================================================

## 评审结论

**文档状态**: 已评审 ✅
**评审日期**: 2026-03-20
**评审结论**: 建议按 P0 → P1 → P2 顺序实施，Phase A 和 Phase B 可并行准备

--------------------------------------------------------------------------------

## 问题确认

### P0 线程安全问题 - 确认 🔴

| 模块 | 问题 | 架构师确认 |
|------|------|-----------|
| AccountPool | freeze/can_trade 无锁 | ✅ 确认 |
| StrategyPool | reserve_margin 非原子 | ✅ 确认 |
| OrderCheck | reservations HashMap 并发 | ✅ 确认 |
| PnlManager | unrealized_pnl 并发写入 | ✅ 确认 |

**架构建议**:
- 使用 `parking_lot::RwLock` 而非 `std::sync::Mutex`
- 理由: RwLock 在读多写少场景性能更好，trading system 符合此模式
- AccountPool 已有 RwLock 雏形，但 `can_trade` 等方法直接访问 `account` 字段未加锁

### P1 RSI 算法问题 - 需校准 🟡

**问题描述准确**，但影响评估过于严重。

当前实现:
```rust
if self.avg_loss.is_zero() {
    self.avg_gain = gain;  // 第1次直接赋值
    self.avg_loss = loss;
}
```

**实际影响**:
- 仅影响前 `period` 次计算（RSI 初始化阶段）
- 初始化完成后使用 Wilder 平滑，误差会收敛
- 策略层面影响有限，不应列为 P1 致命问题

**架构建议**: 降级为 P2，在 Phase C 中一并处理

### P2 性能问题 - 确认 🟡

| 问题 | 确认 | 建议 |
|------|------|------|
| KLineSynthesizer clone | ✅ | 使用 mem::replace |
| PnlManager Vec → HashSet | ✅ | 低优先级 |
| 串行 on_tick | ✅ | Phase B 中解决 |

--------------------------------------------------------------------------------

## 方案可行性评估

### Phase A: 线程安全 ✅ 可行

**优点**:
- 改动范围明确，影响可控
- parking_lot 已是项目依赖
- 渐进式重构风险低

**风险**:
- 需要确保锁粒度不过细导致死锁
- AccountPool 多个方法需要一起加锁

**架构建议**:
```
AccountPool {
    account: RwLock<AccountInfo>,  // 整个 AccountInfo 加锁
}

impl AccountPool {
    // 所有 public 方法自动获得锁保护
    // can_trade(): read lock
    // freeze(): write lock
    // update_equity(): write lock
}
```

### Phase B: 多品种流水线 ⚠️ 需细化

**问题**:
- 文档中 `PipelineOrchestrator` 设计合理
- 但未说明 Tick 如何分发给各品种流水线
- Channel 背压控制方案缺失

**关键设计决策**:
```
Tick 输入 → 按 symbol 分组 → 发送到对应 pipeline
                    ↑
            需要一个 Router 或 Dispatcher
```

**架构建议**:
1. 保留 `TradingEngine` 作为单品种模式
2. 新增 `MultiStrategyEngine` 包装多个 `TradingEngine`
3. 或使用 `Actor` 模式：每个品种一个 Actor

### Phase C: PipelineForm ⚠️ 建议推迟

**理由**:
- 可追溯性是"锦上添花"，不是核心问题
- 当前 Phase 6 Integration 尚未完成，不宜叠加
- 等系统稳定后再加可观测性更合理

**架构建议**: 降级为"未来考虑项"，不纳入当前优化范围

--------------------------------------------------------------------------------

## 实施建议

### 推荐顺序

| 优先级 | Phase | 理由 |
|--------|-------|------|
| P0 | Phase A: 线程安全 | 致命问题必须先修 |
| P1 | Phase B: 多品种并行 | 性能收益最大 |
| P2 | RSI 校准 | 随 Phase A/B 顺便修复 |
| P3 | PipelineForm | 推迟到 v1.1 |

### 实施约束

1. **编译活动暂停规则不变**
   - 当前 Phase 6 Integration 还在进行
   - 修复 P0 问题不应触发编译验证
   - 等 Integration 完成后统一验证

2. **改动范围控制**
   - Phase A 只改 engine/src/account_pool.rs
   - 不动其他模块
   - 保持向后兼容

3. **文档更新要求**
   - 修复后更新 .planning/STATE.md
   - 记录已知风险和缓解措施

--------------------------------------------------------------------------------

## 需澄清的问题

1. **Tick 分发机制**: Phase B 中 Tick 如何按 symbol 分发？
   > 有个共同注册表，确定谁在进行交易，或在指标层自己找数据层拿数据

2. **资金池统一**: AccountPool vs FundPool 是否需要合并？
   > 是的，需要合并为一套设计

3. ~~PipelineForm 必要性~~: 推迟到 v1.1

================================================================================
总结
================================================================================

本优化建议针对当前架构的三大瓶颈：

1. **线程安全** (P0): 使用 `parking_lot::RwLock` 保护共享状态
2. **扩展性** (P1): 品种级独立流水线，并行处理
3. **可追溯性** (P2): PipelineForm 贯穿全程

建议按 P0 → P1 → P2 顺序实施，每个 Phase 独立验收。

## 关键发现

| 类别 | 问题数 | 致命问题 |
|------|--------|----------|
| P0 线程安全 | 6 | 6 |
| P1 算法正确性 | 3 | 1 |
| P2 性能 | 3 | 0 |
| P3 设计差异 | 4 | 0 |

## 下一步

1. 评审本优化建议文档
2. 确认修复优先级
3. 开始 Phase A: 线程安全修复

================================================================================
文档信息
================================================================================

作者: 工作流程优化器 (Workflow Optimizer)
创建日期: 2026-03-20
更新日期: 2026-03-20
文档状态: 待评审
相关文档: docs/2026-03-20-module-deep-analysis.md
下一步: 提交给开发者执行 Phase A 线程安全修复
