================================================================================
Pipeline Checkpoint 日志系统设计
================================================================================

## 1. 概述

**目标**：为交易系统添加 Pipeline Checkpoint 日志系统，实时可视化数据流转过程，便于调试。

**核心原理**：任一环节不通过则数据停止传递，日志清晰显示卡在哪里。

## 2. 设计架构

### 2.1 日志分层

```
Tick数据 → [指标计算] → [策略判断] → [风控预检] → [订单执行]
              ↓             ↓            ↓            ↓
           checkpoint    checkpoint   checkpoint   checkpoint
              ↓             ↓            ↓            ↓
           日志输出      日志输出     日志输出     日志输出
```

### 2.2 日志格式

**通过时：**
```
[2026-03-21 20:16:23.024] [BTCUSDT] [CHECKPOINT]
  ├─ [指标] EMA12=70130.1 EMA26=70125.5 RSI=58.3 PineColor=Green ✔
  ├─ [策略] Signal=BUY velocity_tr_ratio=1.23 ✔
  ├─ [风控] margin_ratio=12.5% max_position=95% ✔
  └─ [订单] OPEN_BUY qty=0.001 price=70130.5 PENDING
```

**失败时（任一环节 BLOCKED）：**
```
[2026-03-21 20:16:23.024] [BTCUSDT] [CHECKPOINT]
  ├─ [指标] EMA12=70130.1 EMA26=70125.5 RSI=58.3 PineColor=Green ✔
  ├─ [策略] Signal=BUY velocity_tr_ratio=0.87 ✘
  └─ [BLOCKED] 数据停止传递 - 原因: TR_RATIO < 1
```

### 2.3 颜色方案

| 状态 | 颜色 | 说明 |
|------|------|------|
| ✔ 通过 | Green | 该环节检查通过 |
| ✘ 失败 | Red | 该环节检查失败，Pipeline 停止 |
| ▶ 进行中 | Yellow | 数据正在该环节处理 |

## 3. 模块设计

### 3.1 CheckpointLogger trait

```rust
/// Pipeline 各环节的 checkpoint 日志记录器
pub trait CheckpointLogger: Send + Sync {
    /// 记录环节开始
    fn log_start(&self, stage: &str, symbol: &str);

    /// 记录环节完成
    fn log_complete(&self, stage: &str, symbol: &str, details: &str);

    /// 记录环节失败（Pipeline 停止）
    fn log_blocked(&self, stage: &str, symbol: &str, reason: &str);

    /// 记录完整 checkpoint（所有环节）
    fn log_full_checkpoint(&self, results: &[StageResult]);
}
```

### 3.2 StageResult 结构

```rust
pub struct StageResult {
    pub stage: Stage,        // 环节名称
    pub passed: bool,        // 是否通过
    pub details: String,     // 详细信息
    pub blocked_reason: Option<String>,  // 失败原因（如果有）
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Indicator,   // 指标计算
    Strategy,    // 策略判断
    RiskPre,     // 风控预检
    RiskRe,      // 风控复核
    Order,       // 订单执行
}
```

### 3.3 实现类

| 实现 | 说明 |
|------|------|
| `ConsoleCheckpointLogger` | 终端彩色输出，带时间戳和颜色 |
| `TracingCheckpointLogger` | 基于 tracing crate 的结构化日志 |
| `CompositeCheckpointLogger` | 组合多个 logger，同时输出 |

## 4. 集成设计

### 4.1 Pipeline 集成点

在 `TradingEngine` 或 `PipelineForm` 中集成：

```rust
// 每个环节调用
fn process_tick(&self, tick: &Tick) -> Option<TradingDecision> {
    // 指标计算
    self.checkpoint_logger.log_start("indicator", &self.symbol);
    let indicator_result = self.calculate_indicators(tick)?;
    self.checkpoint_logger.log_complete("indicator", &self.symbol, &format_indicators(&indicator_result));

    // 策略判断
    self.checkpoint_logger.log_start("strategy", &self.symbol);
    let signal = self.strategy.evaluate(&indicator_result)?;
    self.checkpoint_logger.log_complete("strategy", &self.symbol, &format_signal(&signal));

    // 风控预检
    self.checkpoint_logger.log_start("risk_pre", &self.symbol);
    self.risk_checker.pre_check(&signal, &self.account)?;
    self.checkpoint_logger.log_complete("risk_pre", &self.symbol, "OK");

    // ...
}
```

### 4.2 Telegram 集成

`TelegramNotifier` 保持独立，仅在订单执行成功/失败时发送通知。

**Pipeline checkpoint 日志不发送到 Telegram**，仅用于本地调试。

## 5. 文件结构

```
crates/engine/src/
├── core/                  # 核心引擎
│   ├── engine.rs          # TradingEngine 集成 checkpoint
│   └── pipeline_form.rs    # PipelineForm 集成 checkpoint
├── shared/                # 共享模块
│   └── checkpoint.rs      # CheckpointLogger trait + 实现
└── lib.rs                # 模块导出

src/
├── main.rs               # 集成 checkpoint logger
└── telegram.rs           # Telegram 通知（独立）
```

## 6. 依赖

- `tracing` - 已有，结构化日志
- `chrono` - 已有，时间格式化
- `crossterm` - 可选，终端彩色输出

## 7. 实施步骤

1. 定义 `CheckpointLogger` trait + `StageResult` 结构
2. 实现 `ConsoleCheckpointLogger`（终端彩色）
3. 实现 `TracingCheckpointLogger`（结构化日志）
4. 在 `PipelineForm::process()` 中集成 checkpoint 调用
5. 添加测试用例验证日志输出
6. 文档更新

## 8. 验收标准

- [ ] `cargo check --all` 编译通过
- [ ] 运行 `cargo run --release` 时能看到 checkpoint 日志
- [ ] Pipeline 任一环节失败时，日志清晰显示 `✘ [BLOCKED]`
- [ ] 日志格式统一，易于阅读
