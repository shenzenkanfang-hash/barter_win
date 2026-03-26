================================================================
策略自循环 + 引擎协程启动 设计方案
================================================================
Author: 软件架构师
Date: 2026-03-27
Status: 待实现
================================================================

一、设计目标
================================================================

1. 策略自主循环
   - 策略启动后自动循环执行 tick()
   - 退出条件: status=Ended 或连续错误超限

2. 引擎协程管理
   - TradingEngineV2 统一管理策略生命周期
   - spawn_strategy() 启动策略协程
   - stop_strategy() 停止策略协程

3. 数据解耦
   - 策略只依赖 trait 接口
   - 可注入 Mock 数据源用于测试

================================================================

二、架构设计
================================================================

┌─────────────────────────────────────────────────────────────┐
│  TradingEngineV2                                           │
├─────────────────────────────────────────────────────────────┤
│  strategy_tasks: RwLock<HashMap<symbol, JoinHandle>>      │
│                                                             │
│  spawn_strategy(symbol) → tokio::spawn                    │
│       │                                                    │
│       ▼                                                    │
│  StrategyLoop (Future)                                     │
│       ├── tick_interval: 500ms                            │
│       ├── trader.tick(market, position)                   │
│       ├── 有信号 → order_sender.send_order()              │
│       └── 退出条件: should_exit || max_errors             │
└─────────────────────────────────────────────────────────────┘

数据流:

  DataSource ──get_market_data()──► MarketData ──tick()──► Signal
  PositionSource ──get_position()──► PositionData            │
                                                          ▼
                                                   OrderSender
                                                          │
                                                          ▼
                                                   ExchangeGateway

================================================================

三、模块结构
================================================================

┌─────────────────────────────────────────────────────────────┐
│  f_engine/src/core/                                       │
├─────────────────────────────────────────────────────────────┤
│  strategy_loop.rs    [新建] 策略自循环核心                  │
│  engine_v2.rs       [修改] 添加 spawn/stop 方法            │
│  mod.rs             [修改] 导出新模块                      │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  crates/d_checktable/src/h_15m/                           │
├─────────────────────────────────────────────────────────────┤
│  trader.rs        已实现 tick() 方法                       │
│  indicator.rs     已实现信号生成逻辑                        │
│  mod.rs           已导出 Trader, Status 等                 │
└─────────────────────────────────────────────────────────────┘

================================================================

四、核心类型设计
================================================================

4.1 StrategyLoopConfig - 循环配置

```rust
pub struct StrategyLoopConfig {
    /// 循环间隔 (ms)
    pub tick_interval_ms: u64,
    /// 最大连续错误数
    pub max_consecutive_errors: u32,
    /// 心跳超时 (s)
    pub heartbeat_timeout_secs: u64,
}

impl Default for StrategyLoopConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 500,
            max_consecutive_errors: 3,
            heartbeat_timeout_secs: 30,
        }
    }
}
```

4.2 StrategyLoop - 策略自循环

```rust
pub struct StrategyLoop<S, P, O> {
    pub symbol: String,
    pub data_source: Arc<S>,
    pub position_source: Arc<P>,
    pub order_sender: Arc<O>,
    pub config: StrategyLoopConfig,
    consecutive_errors: u32,
    should_exit: bool,
}
```

4.3 Trait 定义

```rust
/// 市场数据提供者
pub trait MarketDataProvider {
    fn get_market_data(&self, symbol: &str)
        -> impl Future<Output = Result<MarketData, impl Error + Send + Sync>> + Send;
}

/// 持仓数据提供者
pub trait PositionDataProvider {
    fn get_position_data(&self, symbol: &str)
        -> impl Future<Output = Result<PositionData, impl Error + Send + Sync>> + Send;
}

/// 订单发送器
pub trait OrderSender {
    fn send_order(&self, signal: StrategySignal)
        -> impl Future<Output = Result<(), impl Error + Send + Sync>> + Send;
}
```

================================================================

五、引擎方法设计
================================================================

5.1 spawn_strategy - 启动策略

```rust
pub fn spawn_strategy<S, P, O>(
    &self,
    symbol: String,
    data_source: Arc<S>,
    position_source: Arc<P>,
    order_sender: Arc<O>,
    config: StrategyLoopConfig,
) -> Result<(), TradingError>
where
    S: MarketDataProvider + Send + Sync + 'static,
    P: PositionDataProvider + Send + Sync + 'static,
    O: OrderSender + Send + Sync + 'static;
```

5.2 stop_strategy - 停止策略

```rust
pub fn stop_strategy(&self, symbol: &str) -> Result<(), TradingError>;
```

5.3 stop_all_strategies - 停止所有策略

```rust
pub fn stop_all_strategies(&self);
```

================================================================

六、实现文件
================================================================

6.1 新建 strategy_loop.rs

路径: crates/f_engine/src/core/strategy_loop.rs

完整代码:

```rust
//! strategy_loop.rs - 策略自循环协程
//!
//! 引擎 spawn 后策略自主循环，结束后自动退出

#![forbid(unsafe_code)]

use d_checktable::h_15m::{Indicator, MarketData, PositionData, Status, Trader};
use tokio::time::{interval, Duration};
use std::sync::Arc;
use tracing::{info, warn, error};

/// 策略自循环配置
#[derive(Debug, Clone)]
pub struct StrategyLoopConfig {
    /// 循环间隔 (ms)
    pub tick_interval_ms: u64,
    /// 最大连续错误数
    pub max_consecutive_errors: u32,
    /// 心跳超时 (s)
    pub heartbeat_timeout_secs: u64,
}

impl Default for StrategyLoopConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 500,
            max_consecutive_errors: 3,
            heartbeat_timeout_secs: 30,
        }
    }
}

/// 策略自循环 Future
///
/// 使用方式:
/// ```ignore
/// let task = StrategyLoop::new(
///     symbol: "BTCUSDT".into(),
///     data_provider: Arc::new(...),
///     position_provider: Arc::new(...),
///     order_sender: Arc::new(...),
///     config: StrategyLoopConfig::default(),
/// );
/// tokio::spawn(task.run());
/// ```
pub struct StrategyLoop<S, P, O> {
    /// 交易对
    pub symbol: String,
    /// 市场数据提供器
    pub data_source: Arc<S>,
    /// 持仓数据提供器
    pub position_source: Arc<P>,
    /// 订单发送器
    pub order_sender: Arc<O>,
    /// 配置
    pub config: StrategyLoopConfig,
    /// 内部状态
    consecutive_errors: u32,
    /// 退出标志
    should_exit: bool,
}

impl<S, P, O> StrategyLoop<S, P, O>
where
    S: MarketDataProvider + Send + Sync,
    P: PositionDataProvider + Send + Sync,
    O: OrderSender + Send + Sync,
{
    /// 创建策略循环
    pub fn new(
        symbol: String,
        data_source: Arc<S>,
        position_source: Arc<P>,
        order_sender: Arc<O>,
        config: StrategyLoopConfig,
    ) -> Self {
        Self {
            symbol,
            data_source,
            position_source,
            order_sender,
            config,
            consecutive_errors: 0,
            should_exit: false,
        }
    }

    /// 标记退出
    pub fn stop(&mut self) {
        self.should_exit = true;
    }

    /// 运行自循环
    pub async fn run(mut self) {
        info!("[{}] Strategy loop started", self.symbol);

        let mut trader = Trader::new(&self.symbol);
        let mut tick_interval = interval(Duration::from_millis(self.config.tick_interval_ms));

        loop {
            tokio::select! {
                // 定时 tick
                _ = tick_interval.tick() => {
                    if self.should_exit {
                        info!("[{}] Strategy loop exiting (signal)", self.symbol);
                        break;
                    }

                    if let Err(e) = self.tick(&mut trader).await {
                        self.consecutive_errors += 1;
                        error!("[{}] Tick error: {}", self.symbol, e);

                        if self.consecutive_errors >= self.config.max_consecutive_errors {
                            error!("[{}] Max errors reached, exiting", self.symbol);
                            break;
                        }
                    } else {
                        self.consecutive_errors = 0;
                    }
                }
            }
        }

        info!("[{}] Strategy loop stopped", self.symbol);
    }

    /// 单次 tick
    async fn tick(&self, trader: &mut Trader) -> Result<(), StrategyLoopError> {
        // 1. 获取市场数据
        let market = self.data_source
            .get_market_data(&self.symbol)
            .await
            .map_err(|e| StrategyLoopError::DataError(e.to_string()))?;

        // 2. 获取持仓数据
        let position = self.position_source
            .get_position_data(&self.symbol)
            .await
            .map_err(|e| StrategyLoopError::PositionError(e.to_string()))?;

        // 3. 执行策略
        if let Some(signal) = trader.tick(market, position) {
            info!("[{}] Signal generated: {:?}", self.symbol, signal.command);

            // 4. 发送订单
            self.order_sender
                .send_order(signal)
                .await
                .map_err(|e| StrategyLoopError::OrderError(e.to_string()))?;
        }

        Ok(())
    }
}

/// 策略循环错误
#[derive(Debug, thiserror::Error)]
pub enum StrategyLoopError {
    #[error("数据错误: {0}")]
    DataError(String),

    #[error("持仓错误: {0}")]
    PositionError(String),

    #[error("订单错误: {0}")]
    OrderError(String),
}

/// 市场数据提供者 trait
pub trait MarketDataProvider {
    fn get_market_data(&self, symbol: &str)
        -> impl std::future::Future<Output = Result<MarketData, impl std::error::Error + Send + Sync>> + Send;
}

/// 持仓数据提供者 trait
pub trait PositionDataProvider {
    fn get_position_data(&self, symbol: &str)
        -> impl std::future::Future<Output = Result<PositionData, impl std::error::Error + Send + Sync>> + Send;
}

/// 订单发送器 trait
pub trait OrderSender {
    fn send_order(&self, signal: x_data::trading::signal::StrategySignal)
        -> impl std::future::Future<Output = Result<(), impl std::error::Error + Send + Sync>> + Send;
}
```

================================================================

6.2 修改 mod.rs

路径: crates/f_engine/src/core/mod.rs

添加导出:

```rust
pub mod strategy_loop;
pub use strategy_loop::{StrategyLoop, StrategyLoopConfig, MarketDataProvider, PositionDataProvider, OrderSender};
```

================================================================

6.3 修改 engine_v2.rs

在 TradingEngineV2 结构体中添加:

```rust
use crate::core::strategy_loop::{StrategyLoop, StrategyLoopConfig, MarketDataProvider, PositionDataProvider, OrderSender};
use std::collections::HashMap;
use tokio::task::JoinHandle;

/// 策略任务注册表
strategy_tasks: RwLock<HashMap<String, JoinHandle<()>>>,
```

在 impl TradingEngineV2 中添加方法:

```rust
/// 启动策略协程
pub fn spawn_strategy<S, P, O>(
    &self,
    symbol: String,
    data_source: Arc<S>,
    position_source: Arc<P>,
    order_sender: Arc<O>,
    config: StrategyLoopConfig,
) -> Result<(), TradingError>
where
    S: MarketDataProvider + Send + Sync + 'static,
    P: PositionDataProvider + Send + Sync + 'static,
    O: OrderSender + Send + Sync + 'static,
{
    // 检查是否已存在
    {
        let tasks = self.strategy_tasks.read();
        if tasks.contains_key(&symbol) {
            return Err(TradingError::StrategyAlreadyRunning(symbol));
        }
    }

    let loop_task = StrategyLoop::new(
        symbol.clone(),
        data_source,
        position_source,
        order_sender,
        config,
    );
    let handle = tokio::spawn(async move {
        loop_task.run().await;
    });

    // 注册任务
    let mut tasks = self.strategy_tasks.write();
    tasks.insert(symbol.clone(), handle);

    info!("Strategy spawned for {}", symbol);
    Ok(())
}

/// 停止策略协程
pub fn stop_strategy(&self, symbol: &str) -> Result<(), TradingError> {
    let mut tasks = self.strategy_tasks.write();
    if let Some(handle) = tasks.remove(symbol) {
        handle.abort();
        info!("Strategy stopped for {}", symbol);
        Ok(())
    } else {
        Err(TradingError::StrategyNotFound(symbol.to_string()))
    }
}

/// 停止所有策略
pub fn stop_all_strategies(&self) {
    let mut tasks = self.strategy_tasks.write();
    for (symbol, handle) in tasks.drain() {
        handle.abort();
        info!("Strategy stopped for {}", symbol);
    }
}
```

在 TradingError 枚举中添加:

```rust
#[error("策略 {0} 已运行")]
StrategyAlreadyRunning(String),

#[error("策略 {0} 未找到")]
StrategyNotFound(String),
```

================================================================

6.4 修改 main.rs

```rust
use f_engine::core::strategy_loop::{StrategyLoopConfig, MarketDataProvider, PositionDataProvider, OrderSender};
use std::sync::Arc;

// 创建引擎
let config = TradingEngineV2::default_config();
config.gateway = Some(gateway.clone());
config.risk_checker = Some(risk_checker.clone());
let engine = Arc::new(TradingEngineV2::new(config));
engine.start();

// 创建数据提供者实现 (示例)
struct BtcMarketProvider {
    // ...
}

impl MarketDataProvider for BtcMarketProvider {
    async fn get_market_data(&self, symbol: &str) -> Result<MarketData, Box<dyn Error + Send + Sync>> {
        // 从 DataFeeder 或 IndicatorCache 获取
        todo!()
    }
}

struct BtcPositionProvider {
    // ...
}

impl PositionDataProvider for BtcPositionProvider {
    async fn get_position_data(&self, symbol: &str) -> Result<PositionData, Box<dyn Error + Send + Sync>> {
        // 从 PositionManager 获取
        todo!()
    }
}

struct EngineOrderSender {
    engine: Arc<TradingEngineV2>,
}

impl OrderSender for EngineOrderSender {
    async fn send_order(&self, signal: StrategySignal) -> Result<(), Box<dyn Error + Send + Sync>> {
        // 调用 engine.process_signal()
        todo!()
    }
}

// 启动策略自循环
engine.spawn_strategy(
    "BTCUSDT".to_string(),
    Arc::new(BtcMarketProvider {}),
    Arc::new(BtcPositionProvider {}),
    Arc::new(EngineOrderSender { engine: engine.clone() }),
    StrategyLoopConfig::default(),
)?;

info!("All strategies started");

// 主循环
loop {
    tokio::select! {
        msg = kline_1m_stream.next_message() => { /* 处理消息 */ }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down...");
            engine.stop_all_strategies();
            break;
        }
    }
}
```

================================================================

七、实现步骤
================================================================

| 步骤 | 操作 | 文件 |
|------|------|------|
| 1 | 新建 strategy_loop.rs | f_engine/src/core/ |
| 2 | 修改 mod.rs 添加导出 | f_engine/src/core/ |
| 3 | 修改 engine_v2.rs 添加字段 | f_engine/src/core/ |
| 4 | 修改 engine_v2.rs 添加方法 | f_engine/src/core/ |
| 5 | 添加错误类型 | f_engine/src/core/ |
| 6 | 实现 main.rs 集成 | src/ |
| 7 | 编译验证 | cargo check --all |

================================================================

八、测试方案
================================================================

8.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strategy_loop_runs() {
        // Mock 数据源
        let data_source = MockMarketProvider::always_return(MarketData {
            price: dec!(50000),
            volatility: 0.1,
            tr_ratio: 0.05,
        });

        let position_source = MockPositionProvider::empty();

        let order_sender = MockOrderSender::new();

        let loop_task = StrategyLoop::new(
            "BTCUSDT".into(),
            Arc::new(data_source),
            Arc::new(position_source),
            Arc::new(order_sender),
            StrategyLoopConfig {
                tick_interval_ms: 100,
                max_consecutive_errors: 3,
                heartbeat_timeout_secs: 5,
            },
        );

        // 运行 500ms
        let handle = tokio::spawn(async move {
            loop_task.run().await;
        });

        tokio::time::sleep(Duration::from_millis(500)).await;
        handle.abort();
    }
}
```

8.2 集成测试

在 h_sandbox/simulator 中实现模拟数据源，进行完整流程测试。

================================================================

九、版本历史
================================================================

| 版本 | 日期       | 说明 |
|------|------------|------|
| 1.0  | 2026-03-27 | 初始设计方案 |

================================================================
End of Document
================================================================
