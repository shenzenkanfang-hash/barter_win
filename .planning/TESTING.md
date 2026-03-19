# 测试模式

**分析日期：** 2026-03-20

## 测试框架

**测试运行器：**
- Rust 内置测试框架，带 `#[test]` 属性
- `tokio-test` 用于异步测试 (workspace 依赖)
- `criterion` 用于基准测试 (workspace 依赖)

**开发依赖 (barter/Cargo.toml)：**
```toml
[dev-dependencies]
rust_decimal_macros = { workspace = true }
serde_json = { workspace = true }
spin_sleep = { workspace = true }
tokio = { workspace = true, features = ["fs"]}
criterion = { workspace = true }
```

**运行命令：**
```bash
cargo test              # 运行所有测试
cargo test <name>       # 运行特定测试
cargo bench             # 运行基准测试
```

## 测试组织

**三个测试位置：**

1. **内联单元测试** - 源文件内的 `#[cfg(test)]` 模块
   - 位置：在 `src/*.rs` 文件内
   - 模式：`#[cfg(test)] mod tests { ... }`
   - 数量：workspace 中 66 个带内联测试的文件

2. **集成测试** - `tests/` 目录
   - 位置：`barter/tests/*.rs`
   - 用途：完整系统/组件集成测试
   - 示例：`barter/tests/test_engine_process_engine_event_with_audit.rs`

3. **基准测试** - `benches/` 目录
   - 位置：`barter/benches/backtest/mod.rs`
   - 框架：Criterion
   - 工具：`harness = false` 用于自定义基准测试设置

**CI 测试命令 (.github/workflows/ci.yml)：**
```yaml
- name: Run cargo test
  uses: actions-rs/cargo@v1
  with:
    command: test
```

## 测试结构

**内联测试示例** (barter/src/engine/state/trading/mod.rs)：
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trading_state_update() {
        let mut state = TradingState::Disabled;
        state.update(TradingState::Enabled);
        assert_eq!(state, TradingState::Enabled);
    }
}
```

**集成测试示例** (barter/tests/test_engine_process_engine_event_with_audit.rs)：
```rust
#[test]
fn test_engine_process_engine_event_with_audit() {
    let (execution_tx, mut execution_rx) = mpsc_unbounded();

    let mut engine = build_engine(TradingState::Disabled, execution_tx);
    assert_eq!(engine.meta.sequence, Sequence(0));

    // Simulate AccountSnapshot from ExecutionManager::init
    let event = account_event_snapshot(&engine.state.assets);
    let audit = process_with_audit(&mut engine, event.clone());
    assert_eq!(audit.context.sequence, Sequence(0));
    // ... extensive assertions
}
```

## 测试工具

**内置测试工具** (barter/src/lib.rs)：
```rust
pub mod test_utils {
    use crate::{Timed, engine::state::asset::AssetState, ...};

    pub fn f64_is_eq(actual: f64, expected: f64, epsilon: f64) -> bool { ... }

    pub fn time_plus_days(base: DateTime<Utc>, plus: u64) -> DateTime<Utc> { ... }
    pub fn time_plus_secs(base: DateTime<Utc>, plus: i64) -> DateTime<Utc> { ... }
    pub fn time_plus_millis(base: DateTime<Utc>, plus: i64) -> DateTime<Utc> { ... }
    pub fn time_plus_micros(base: DateTime<Utc>, plus: i64) -> DateTime<Utc> { ... }

    pub fn trade(time_exchange: DateTime<Utc>, side: Side, ...) -> Trade<...> { ... }

    pub fn asset_state(symbol: &str, balance_total: f64, ...) -> AssetState { ... }
}
```

## 模拟

**模拟实现：**

1. **MockExecution** (barter-execution/src/exchange/mock/mod.rs)
   - 模拟交易所行为用于测试
   - 支持可配置延迟和费用的订单执行
   - 实现 `ExecutionClient` trait

```rust
pub struct MockExecution<FnTime> {
    pub mocked_exchange: ExchangeId,
    pub clock: FnTime,
    pub request_tx: mpsc::UnboundedSender<MockExchangeRequest>,
    pub event_rx: broadcast::Receiver<UnindexedAccountEvent>,
}
```

2. **MockExecutionClient** (barter-execution/src/client/mock/mod.rs)
   - 用于连接模拟交易所的客户端模拟
   - 用 ExchangeId::Mock 实现 `ExecutionClient` trait

3. **MockExchangeRequest** (barter-execution/src/exchange/mock/request.rs)
   - 基于枚举的请求类型用于模拟交互
   - 变体：FetchAccountSnapshot、FetchBalances、OpenOrder 等

**模拟模式：**
```rust
impl<FnTime> ExecutionClient for MockExecution<FnTime>
where
    FnTime: Fn() -> DateTime<Utc> + Clone + Sync,
{
    const EXCHANGE: ExchangeId = ExchangeId::Mock;
    type AccountStream = BoxStream<'static, UnindexedAccountEvent>;

    async fn open_order(&self, request: OrderRequestOpen<...>) -> Option<...> {
        let (response_tx, response_rx) = oneshot::channel();
        // Send request, return response
    }
}
```

## 测试夹具

**用于测试数据的辅助函数：**
```rust
fn build_engine(
    trading_state: TradingState,
    execution_tx: UnboundedTx<ExecutionRequest>,
) -> Engine<...> { ... }

fn account_event_snapshot(assets: &AssetStates) -> EngineEvent<DataKind> { ... }

fn market_event_trade(time_plus: u64, instrument: usize, price: f64) -> EngineEvent<DataKind> { ... }

fn account_event_order_response(...) -> EngineEvent<DataKind> { ... }

fn account_event_balance(...) -> EngineEvent<DataKind> { ... }

fn account_event_trade(...) -> EngineEvent<DataKind> { ... }

fn command_close_position(instrument: usize) -> EngineEvent<DataKind> { ... }
```

## 基准测试

**Criterion 设置** (barter/benches/backtest/mod.rs)：
```rust
criterion::criterion_main!(benchmark_backtest);

fn benchmark_backtest() {
    // Configuration embedded as JSON string
    let Config { risk_free_return, system: SystemConfig { ... } }
        = serde_json::from_str(CONFIG).unwrap();

    let mut c = Criterion::default().without_plots();
    bench_backtest(&mut c, Arc::clone(&args_constant), &args_dynamic);
}
```

**基准测试组：**
- "Backtest" - 单次回测性能
- "Backtest Concurrent" - 并发回测扩展 (10、500 并发)

**基准测试配置：**
```rust
group.warm_up_time(std::time::Duration::from_secs(1));
group.measurement_time(std::time::Duration::from_secs(10));
group.sample_size(50);
group.throughput(Throughput::Elements(1));
```

## 断言模式

**标准断言：**
```rust
assert_eq!(engine.meta.sequence, Sequence(0));
assert_eq!(engine.state.connectivity.global, Health::Reconnecting);
```

**自定义浮点数比较：**
```rust
pub fn f64_is_eq(actual: f64, expected: f64, epsilon: f64) -> bool {
    if actual.is_nan() && expected.is_nan() { true }
    else if actual.is_infinite() && expected.is_infinite() {
        actual.is_sign_positive() == expected.is_sign_positive()
    }
    // ... epsilon comparison
}
```

**Option/Result 断言：**
```rust
assert!(engine.state.instruments.instrument_index(&InstrumentIndex(0)).orders.0.is_empty());
assert!(some_option.is_some());
```

**Truth 断言：**
```rust
assert!(matches!(self, Self::Shutdown(_)));
assert!(matches!(output, EngineOutput::AlgoOrders(_)));
```

## 异步测试

**使用 tokio-test：**
```rust
#[tokio::test]
async fn test_async_operation() {
    // Async test body
}
```

## 测试命名

**模式：**
- `test_<unit>_<scenario>_<expected>`
- 示例：`test_engine_process_engine_event_with_audit`

**测试中的常量：**
```rust
const STARTING_TIMESTAMP: DateTime<Utc> = DateTime::<Utc>::MIN_UTC;
const RISK_FREE_RETURN: Decimal = dec!(0.05);
const STARTING_BALANCE_USDT: Balance = Balance { total: dec!(40_000.0), free: dec!(40_000.0) };
const QUOTE_FEES_PERCENT: f64 = 0.1;
```

## 覆盖率

**CI 中未强制要求覆盖率目标**，但代码库有大量测试包括：
- 状态转换的单元测试
- 完整引擎事件处理的集成测试
- 性能回归的基准测试

---

*测试分析：2026-03-20*
