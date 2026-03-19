# 编码规范

**分析日期：** 2026-03-20

## 语言

**主要：**
- Rust (Edition 2024) - 所有 crates 使用 edition = "2024"

## 格式化

**工具：**
- rustfmt - 自动化代码格式化
- 配置文件：项目根目录的 `rustfmt.toml`
- 设置：
  ```toml
  edition = "2024"
  imports_granularity = "crate"
  ```

**执行：**
- CI 在每次推送/PR 时运行 `cargo fmt --all -- --check`
- 格式化是强制性的 (continue-on-error: false)

## 代码检查

**工具：**
- clippy - 带严格规则的代码检查

**全局属性 (barter/src/lib.rs)：**
```rust
#![forbid(unsafe_code)]
#![warn(
    unused,
    clippy::cognitive_complexity,
    unused_crate_dependencies,
    unused_extern_crates,
    clippy::unused_self,
    clippy::useless_let_if_seq,
    missing_debug_implementations,
    rust_2018_idioms,
    rust_2024_compatibility
)]
#![allow(clippy::type_complexity, clippy::too_many_arguments, type_alias_bounds)]
```

**执行：**
- CI 在每次推送/PR 时运行 `cargo clippy -- -D warnings`
- 不允许使用不安全代码 (通过 #![forbid(unsafe_code)] 禁止)

## 命名规范

**文件：**
- 使用 snake_case：`engine_state.rs`、`mod.rs`
- 每个模块一个文件优先，或多个相关项放一个文件

**目录：**
- 使用 snake_case：`engine/state/`、`statistic/metric/`

**类型/结构体/枚举：**
- PascalCase：`EngineState`、`TradingState`、`EngineEvent`
- 枚举变体：`TradingState::Enabled`、`EngineEvent::Market`

**Traits：**
- PascalCase：`AlgoStrategy`、`Processor`、`ExecutionClient`

**函数/方法：**
- snake_case：`update_from_account`、`process_with_audit`、`generate_algo_orders`

**变量：**
- snake_case：`time_exchange`、`balance_total`、`sequence`
- 布尔变量可使用 `is_`、`has_`、`can_` 前缀

**常量：**
- SCREAMING_SNAKE_CASE：`STARTING_BALANCE_USDT`、`QUOTE_FEES_PERCENT`

**类型参数：**
- 单个字母或 PascalCase：`T`、`State`、`Clock`

## 导入组织

**顺序 (rustfmt 通过 imports_granularity = "crate" 执行)：**
1. 标准库导入
2. 外部 crate 导入 (按字母顺序)
3. 内部 crate 导入

**来自 `barter/src/engine/mod.rs` 的示例：**
```rust
use crate::{
    EngineEvent, Sequence,
    engine::{
        action::{...},
        audit::{...},
        clock::EngineClock,
        ...
    },
    ...
};
use barter_data::{...};
use barter_execution::{...};
use chrono::{DateTime, Utc};
```

## 错误处理

**模式：**
- 使用 `thiserror` crate 进行错误派生
- 带 `#[derive(Error)]` 的自定义错误枚举

**来自 `barter/src/error.rs` 的示例：**
```rust
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize, Error)]
pub enum BarterError {
    #[error("IndexError: {0}")]
    IndexError(#[from] IndexError),

    #[error("ExecutionBuilder: {0}")]
    ExecutionBuilder(String),

    #[error("market data: {0}")]
    MarketData(#[from] DataError),
}
```

**原则：**
- 使用 `#[from]` 进行自动转换
- 在错误消息中包含上下文
- 错误实现 Debug、Clone、Deserialize、Serialize

## 模块设计

**模块结构：**
- 每个模块有 `mod.rs` 重新导出公共项
- 复杂模块的子模块在单独文件中
- 清晰分离：`engine/action/`、`engine/state/`、`engine/audit/`

**库导出 (barter/src/lib.rs)：**
```rust
pub mod engine;
pub mod error;
pub mod execution;
pub mod logging;
pub mod risk;
pub mod statistic;
pub mod strategy;
pub mod system;
pub mod backtest;
pub mod shutdown;
```

## 文档

**Crate 级文档：**
```rust
//! # Barter
//! Barter core is a Rust framework for building high-performance live-trading...
```

**模块级文档：**
```rust
/// Defines how the [`Engine`] actions a [`Command`], and the associated outputs.
pub mod action;
```

**公共 API 的文档注释：**
```rust
/// Algorithmic trading `Engine`.
///
/// The `Engine`:
/// * Processes input [`EngineEvent`] (or custom events if implemented).
/// * Maintains the internal [`EngineState`].
```
- 使用 `[]` 进行交叉引用：`[`Engine`]` 或 `[`EngineState`]`
- 使用反引号表示代码元素

## 派生模式

**常用派生 (按顺序)：**
```rust
#[derive(
    Debug,           // 公共类型始终派生
    Clone,           // 用于所有权上下文
    Eq,              // 用于有 PartialEq 的类型
    PartialEq,       // 相等比较
    Ord,             // 用于有 PartialOrd 的类型
    PartialOrd,      // 排序
    Hash,            // 用于集合
    Deserialize,     // Serde
    Serialize,       // Serde
    Error,           // thiserror
    Constructor,     // derive_more - builder 模式
    From,            // derive_more - 转换
    Display,         // derive_more - 格式化
)]
```

**impl 块中的 trait bounds：**
```rust
impl<Clock, GlobalData, InstrumentData, ExecutionTxs, Strategy, Risk>
    Processor<EngineEvent<InstrumentData::MarketEventKind>>
    for Engine<...>
where
    Clock: EngineClock + for<'a> Processor<&'a EngineEvent<...>>,
    ...
{
    type Audit = ...;
    fn process(&mut self, event: EngineEvent<...>) -> Self::Audit { ... }
}
```

## 日志

**框架：**
- `tracing` crate 用于结构化日志
- 使用 `info!`、`error!`、`warn!` 宏，`?` 用于调试

**示例：**
```rust
info!(
    ?requests,
    "Engine actioning user Command::SendCancelRequests"
);
```

## 代码风格指南

**Match 表达式：**
```rust
match &event {
    EngineEvent::Shutdown(_) => ...,
    EngineEvent::Command(command) => ...,
    ...
}
```

**Builder 模式：**
```rust
EngineState::builder(&instruments, DefaultGlobalData::default(), |_| {
    DefaultInstrumentMarketData::default()
})
.time_engine_start(STARTING_TIMESTAMP)
.trading_state(trading_state)
.balances([...])
.build()
```

**类型别名以提高清晰度：**
```rust
pub type AccountStreamEvent<...> = reconnect::Event<ExchangeId, AccountEvent<...>>;
```

---

*规范分析：2026-03-20*
