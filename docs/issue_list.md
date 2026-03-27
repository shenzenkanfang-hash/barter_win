# 问题清单

## 待处理问题

### 1. Tick 结构冗余（可简化）

**问题描述：**
- `b_data_source::Tick` 包含 `kline_1m: Option<KLine>`，但 Binance WS 实际推送的就是 K 线
- `Tick` 是历史遗留设计，现在只是 K 线的包装器
- DataFeeder 用 `HashMap<Symbol, Tick>` 存储，但只用到其中的 K 线字段

**当前使用文件：**
```
b_data_source/src/api/data_feeder.rs
b_data_source/src/models/types.rs
h_sandbox/src/historical_replay/memory_injector.rs
f_engine/src/interfaces/adapters.rs
...
```

**建议方案：**
简化 DataFeeder，直接用 `HashMap<Symbol, KLine>` 替代 `HashMap<Symbol, Tick>`

**影响范围：**
- DataFeeder 接口变更
- 所有使用 push_tick() 的代码
- 沙盒相关代码

**状态：** 待处理

---

### 2. `process_tick()` 函数命名不准确

**问题：**
- 函数名 `process_tick` 暗示处理原始 tick 数据
- 实际功能：触发器检查 → 策略 → 风控 → 下单
- 建议改名为 `check_and_trade()`

**状态：** 待处理

---

### 3. 缺失：两层触发机制说明

**问题描述：**
- 文档缺失两层触发架构
- 日线级：MACD变色 → 选品种加入关注列表
- 分钟级：50ms检查 → 高波动 + Idle → 开仓
- 全局层负责维护品种状态（Idle/Watching/Trading/Closing）

**建议：**
- 更新 architecture_full.md 添加两层触发机制
- 添加 GlobalState 结构说明

**状态：** 待处理

---

### 4. 引擎层与数据源未连接

**问题描述：**
- `main.rs` 中 `Kline1mStream` 和 `TradingEngineV2` 是分离的
- 主循环订阅 K 线，但只打印日志
- `engine.process_tick()` 从未被调用
- 引擎没有数据输入

**代码证据：**
```rust
// 引擎创建了
let _engine = TradingEngineV2::new(config);

// K线订阅了
let mut kline_1m_stream = Kline1mStream::new(trading_symbols).await?;

// 但没有连接！
loop {
    msg_1m = kline_1m_stream.next_message() => {
        count_1m += 1;  // 只是计数
        // 没有 engine.process_tick()
    }
}
```

**需要修复：**
- 主循环收到 K 线后调用 `engine.process_tick()`
- 或者引擎内部订阅数据

**状态：** 待处理

---

### 3. project_dataflow.md 中网关实现类描述有误（已修正）

**问题：** 文档中写了 `BinanceGateway` 在 f_engine 层，但实际不存在

**修正：** 
- `MockBinanceGateway` 在 f_engine/src/order/
- `ShadowBinanceGateway` 在 h_sandbox/src/gateway/
- 真实 Binance 网关在其他层

**状态：** ✅ 已修正

---

## 已完成问题

### 1. 沙盒两端拦截架构（已完成）

**说明：**
- ShadowBinanceGateway：拦截订单/账户/持仓请求
- StreamTickGenerator：模拟 WS 推送数据
- 中间业务层 c/d/e/f_engine 完全不变

**文档：** `docs/sandbox_realtime_plan.md`

**状态：** ✅ 完成

---

### 2. 项目全层数据流文档（已完成）

**说明：**
- 完整描述各层数据流
- 接口映射
- 全流程闭环

**文档：** `docs/project_dataflow.md`

**状态：** ✅ 完成
