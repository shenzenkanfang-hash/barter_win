================================================================================
交易流程全链路闭环串联设计
================================================================================
Author: 产品经理
Created: 2026-03-21
Status: approved
GSD-Phase: planning
================================================================================

一、项目概述
================================================================================

目标：串联整个交易流程，实现 Tick 接收 → 指标计算 → 策略信号 → 风控预检 → 下单执行 → 持仓更新的全链路闭环。

运行模式：实时模拟（连接 Binance WebSocket）
数据来源：Binance WebSocket 实时行情
交易品种：全品种
交易方向：双向（做多/做空）

================================================================================
二、技术方案
================================================================================

2.1 架构分层

+----------------------------------------------------------+
|                    TradingEngine (主引擎)                  |
+----------------------------------------------------------+
|  on_tick(tick)                                            |
|    1. DataFeeder 广播 Tick                                |
|    2. 指标层计算 (TR/Pine颜色/价格位置)                    |
|    3. Strategy 生成信号                                   |
|    4. SignalSynthesisLayer 合成信号                        |
|    5. RiskPreChecker 锁外预检                            |
|    6. MockBinanceGateway 下单执行                         |
|    7. PositionManager 更新持仓                            |
+----------------------------------------------------------+

2.2 核心数据流

Binance WebSocket
       |
       v
   DataFeeder (数据分发器)
       |
       +---> VolatilityDetector (波动率检测)
       |           |
       |           v
       |     VolatilityChannel (通道类型: Slow/Fast)
       |
       +---> KLineSynthesizer (K线合成)
       |           |
       |           v
       +---> IndicatorLayer (指标层: EMA/RSI/PineColor/PricePosition)
       |           |
       |           v
       +---> Strategy (策略信号: LongEntry/ShortEntry/Exit)
       |           |
       |           v
       +---> SignalSynthesisLayer (信号合成)
       |           |
       |           v
       +---> RiskPreChecker (锁外风控预检)
       |           |
       |           v (如果通过)
       +---> MockBinanceGateway (订单执行)
       |           |
       |           v
       +---> PositionManager (持仓更新)
       |
       v
   SqliteEventRecorder (事件持久化)

================================================================================
三、分阶段实现计划
================================================================================

阶段 1: DataFeeder + 指标层集成
--------------------------------------------------------------------------------
目标: Tick 数据接收并驱动指标实时计算

任务:
1. 实现 DataFeeder 结构体
   - 建立 Binance WebSocket 连接 (wss://stream.binance.com:9443/ws)
   - 订阅 KLine stream (1m)
   - 实现 Tick 数据分发

2. 实现 VolatilityDetector
   - 计算 1m 波动率
   - 计算 15m 波动率
   - 判断通道类型 (Slow/Fast)

3. 集成到 IndicatorLayer
   - EMA/RSI/PineColor/PricePosition tick 级更新
   - 指标计算结果回调

阶段 2: Strategy + SignalSynthesisLayer 集成
--------------------------------------------------------------------------------
目标: 策略生成信号并合成最终交易决策

任务:
1. 实现 Strategy trait
   - on_kline_close(): K线完成时生成信号
   - on_tick(): Tick 级快速判断

2. 实现 SignalSynthesisLayer
   - 综合多信号源
   - check_enter_high_volatility(): 进入高速通道
   - check_exit_high_volatility(): 退出高速通道
   - check_daily_trend_exit(): 日线趋势退出

3. 实现 VolatilityChannel
   - Slow 通道: 基于 K线完成信号
   - Fast 通道: 基于波动率触发

阶段 3: MockBinanceGateway + OrderExecutor 集成
--------------------------------------------------------------------------------
目标: 订单执行并更新持仓

任务:
1. 集成 MockBinanceGateway
   - pre_risk_check(): 下单前风控检查
   - check_liquidation(): 强制平仓检查
   - place_order(): 订单执行

2. 实现 OrderExecutor
   - 市价单执行
   - 订单结果回调

3. 集成 PositionManager
   - 持仓更新
   - 盈亏计算

阶段 4: 闭环串联测试
--------------------------------------------------------------------------------
目标: 端到端验证整个流程

任务:
1. 实现 TradingEngine
   - 整合所有模块
   - 统一的 on_tick 入口

2. 实现 main.rs
   - 初始化所有组件
   - 启动 DataFeeder
   - 事件循环

3. 端到端测试
   - 接收真实 Tick 数据
   - 验证指标计算
   - 验证信号生成
   - 验证订单执行
   - 验证持仓更新
   - 验证事件持久化

================================================================================
四、接口设计
================================================================================

4.1 DataFeeder trait

pub trait MarketDataFeeder: Send + Sync {
    fn start(&self) -> Result<(), TradeError>;
    fn subscribe(&self, symbols: &[String]) -> Result<(), TradeError>;
    fn on_tick<F>(&self, callback: F)
    where F: Fn(Tick) + Send + 'static;
}

4.2 TradingEngine

pub struct TradingEngine {
    data_feeder: Arc<dyn MarketDataFeeder>,
    indicator_layer: Arc<IndicatorLayer>,
    strategy: Arc<dyn Strategy>,
    signal_synthesizer: Arc<SignalSynthesisLayer>,
    risk_checker: Arc<RiskPreChecker>,
    gateway: Arc<dyn ExchangeGateway>,
    position_manager: Arc<PositionManager>,
    event_recorder: Arc<dyn EventRecorder>,
}

impl TradingEngine {
    pub fn new(/* ... */) -> Self { ... }

    pub fn start(&self) -> Result<(), TradeError> {
        // 启动 DataFeeder
        // 注册 tick 回调
        // 启动事件循环
    }

    fn on_tick(&self, tick: Tick) {
        // 1. 分发 Tick
        // 2. 更新指标
        // 3. 生成信号
        // 4. 风控预检
        // 5. 执行订单
        // 6. 更新持仓
        // 7. 记录事件
    }
}

4.3 Tick 结构

#[derive(Debug, Clone)]
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    pub kline_1m: Option<KLine>,
    pub kline_15m: Option<KLine>,
    pub kline_1d: Option<KLine>,
}

================================================================================
五、关键实现点
================================================================================

5.1 高频路径无锁

- Tick 接收、指标更新、策略判断全部无锁
- 锁仅用于下单和资金更新
- RiskPreChecker 在锁外执行所有检查

5.2 增量计算 O(1)

- EMA、RSI 等指标必须增量计算
- K线增量更新当前K线
- 避免重复计算全量数据

5.3 事件驱动

- K线完成时触发策略信号
- Tick 驱动价格位置指标
- 波动率变化触发通道切换

================================================================================
六、测试验证
================================================================================

6.1 单元测试

- DataFeeder: WebSocket 连接/订阅/数据解析
- IndicatorLayer: EMA/RSI/PineColor 计算准确性
- Strategy: 信号生成逻辑
- MockBinanceGateway: 风控检查/订单执行

6.2 集成测试

- Tick → 指标 → 信号 全流程
- 信号 → 风控 → 下单 全流程
- 订单 → 持仓 → 资金 全流程

6.3 端到端测试

- 连接 Binance WebSocket
- 接收真实数据
- 验证完整流程

================================================================================
七、风险与对策
================================================================================

| 风险 | 对策 |
|------|------|
| WebSocket 断连 | 指数退避重连 (5s/10s/20s/.../120s) |
| 数据延迟 | 异步处理，不阻塞主流程 |
| 订单频率超限 | MockBinanceGateway 频率限制 (10次/秒) |
| 保证金不足 | 三层风控检查 |

================================================================================
八、产出物清单
================================================================================

1. DataFeeder 实现 (market crate)
2. VolatilityDetector 实现 (market crate)
3. TradingEngine 实现 (engine crate)
4. main.rs 集成
5. 单元测试
6. 集成测试
7. 端到端测试

================================================================================
