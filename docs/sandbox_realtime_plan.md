# 实时沙盒完整落地方案

## 一、整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│              sandbox_main.rs (沙盒入口)                          │
│   1. fetch_klines_from_api() → 拉取历史K线                     │
│   2. StreamTickGenerator → 生成模拟Tick                          │
│   3. TradingEngineV2::new_sandbox() → 启动真实引擎             │
└─────────────────────────────────────────────────────────────────┘
                              ↓ push_tick
┌─────────────────────────────────────────────────────────────────┐
│                     DataFeeder                                   │
│   latest_ticks: HashMap<Symbol, Tick>                          │
└─────────────────────────────────────────────────────────────────┘
                              ↓ ws_get_1m()
┌─────────────────────────────────────────────────────────────────┐
│              TradingEngineV2 (真实业务引擎)                       │
│   策略 → 风控 → 订单执行                                        │
└─────────────────────────────────────────────────────────────────┘
                              ↓ place_order/get_account
┌─────────────────────────────────────────────────────────────────┐
│         ShadowBinanceGateway (沙盒网关)                          │
│   模拟成交/账户/持仓（不发出真实请求）                           │
└─────────────────────────────────────────────────────────────────┘
```

## 二、核心文件修改清单

| 文件 | 修改内容 |
|-----|---------|
| `f_engine/src/core/mod.rs` | 新增 `TradingEngineV2::new_sandbox(config, data_feeder)` |
| `src/sandbox_main.rs` | 重写，接入真实 TradingEngineV2 |

## 三、具体代码改造点

### 1. TradingEngineV2 新增沙盒入口

**文件**: `f_engine/src/core/mod.rs`

```rust
// 新增沙盒模式构造方法
impl TradingEngineV2 {
    /// 创建沙盒模式引擎（从 DataFeeder 拉取行情）
    pub fn new_sandbox(
        config: EngineConfig,
        data_feeder: Arc<DataFeeder>,
        gateway: Arc<dyn ExchangeGateway>,
        risk_checker: Arc<dyn RiskChecker>,
    ) -> Self {
        Self {
            config,
            data_feeder: Some(data_feeder),
            gateway,
            risk_checker,
            state: EngineState::Idle,
            // ... 其他字段
        }
    }
}
```

### 2. sandbox_main.rs 重写

**文件**: `src/sandbox_main.rs`

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志
    init_tracing();
    
    // 2. 解析参数
    let config = parse_args();
    
    // 3. 从API拉取K线
    let klines = fetch_klines_from_api(&config.symbol, &config.start, &config.end).await?;
    
    // 4. 创建 DataFeeder
    let data_feeder = Arc::new(DataFeeder::new());
    
    // 5. 创建 ShadowBinanceGateway（拦截订单/账户/持仓）
    let gateway: Arc<dyn ExchangeGateway> = Arc::new(ShadowBinanceGateway::with_default_config(config.initial_fund));
    
    // 6. 创建真实风控（使用项目风控，非mock）
    let risk_checker = create_real_risk_checker()?;
    
    // 7. 创建 TradingEngineV2（沙盒模式）
    let engine_config = TradingEngineV2::default_config();
    let mut engine = TradingEngineV2::new_sandbox(
        engine_config,
        data_feeder.clone(),
        gateway.clone(),
        risk_checker,
    );
    
    // 8. 启动 TickGenerator → push_tick 循环（后台任务）
    tokio::spawn({
        let data_feeder = data_feeder.clone();
        let klines = klines.into_iter();
        async move {
            let gen = StreamTickGenerator::from_loader(config.symbol, klines);
            for tick_data in gen {
                let kline_1m = build_kline(&tick_data);
                let tick = Tick {
                    symbol: tick_data.symbol,
                    price: tick_data.price,
                    qty: tick_data.qty,
                    timestamp: tick_data.timestamp,
                    kline_1m: Some(kline_1m),
                    kline_15m: None,
                    kline_1d: None,
                };
                data_feeder.push_tick(tick);
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        }
    });
    
    // 9. 启动引擎主循环
    engine.run().await?;
    
    Ok(())
}
```

## 四、运行命令

```bash
# 沙盒测试 HOTUSDT 2025-10-09 ~ 2025-10-11
cargo run --bin sandbox -- --symbol HOTUSDT --start 2025-10-09 --end 2025-10-11 --fund 10000

# 快速模式（无延迟）
cargo run --bin sandbox -- --symbol HOTUSDT --start 2025-10-09 --end 2025-10-11 --fund 10000 --fast
```

## 五、验证步骤

1. **编译检查**: `cargo check -p trading-system`
2. **运行沙盒**: `cargo run --bin sandbox -- --symbol HOTUSDT --start 2025-10-09 --end 2025-10-11 --fund 10000 --fast`
3. **验证指标**:
   - DataFeeder ws_get_1m() 返回非空K线
   - ShadowBinanceGateway 拦截订单请求
   - TradingEngineV2 执行真实策略逻辑
   - 账户/持仓模拟正确

## 六、改造原则

| 原则 | 说明 |
|-----|------|
| 最小侵入 | 不修改原有 TradingEngineV2::new()，新增 new_sandbox() |
| 可逆 | 沙盒代码独立，上线不打包无影响 |
| 复用真实逻辑 | 风控/策略/订单全用业务层代码 |

## 七、当前已完成的修改

| 文件 | 改动 |
|-----|------|
| sandbox_main.rs | fetch_klines_from_api() 从API拉取真实K线，支持--start/--end |
| sandbox_main.rs | DEFAULT_SYMBOL=HOTUSDT, DEFAULT_START=2025-10-09, DEFAULT_END=2025-10-11 |
| README.md | 更新两端拦截模式说明 |
