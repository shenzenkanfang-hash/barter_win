================================================================================
架构文档 - Barter-rs 量化交易系统
================================================================================

项目: barter-rs
路径: D:\Rust项目\barter-rs-main
状态: 积极开发中（阶段6：集成）

================================================================================
1. 架构模式
================================================================================

1.1 整体模式：分层六边形架构
--------------------------------------------------------------------------------

系统采用六层垂直切片架构，引擎层包含六边形架构元素。每一层都有明确的职责和通信路径。

    上层（策略层）              下层（机制层）
    =======================    =========================
    g_test（测试）              a_common（基础设施）
    h_sandbox（实验）           b_data_source（数据网关）
    f_engine（运行时）         c_data_process（信号生成）
    e_risk_monitor（合规）     d_checktable（检查）
    ...

1.2 关键架构原则
--------------------------------------------------------------------------------

【1】高频路径无锁
    - Tick接收、指标更新、策略判断：无锁
    - 锁仅用于：订单执行、资金更新
    - 锁外预检所有风控条件

【2】增量计算 O(1)
    - EMA、SMA、MACD：增量计算
    - K线：增量更新当前Bar

【3】三层指标体系
    - TR（True Range）：波动率突破判断
    - Pine颜色：趋势信号（MACD + EMA10/20 + RSI）
    - 价格位置：周期极值判断

【4】混合持仓模式
    - 资金池：RwLock保护（低频）
    - 策略持仓：独立计算（无锁）

================================================================================
2. 分层架构（六层 + 2）
================================================================================

层级依赖（从上到下）：
    g_test --> h_sandbox --> f_engine --> e_risk_monitor --> d_checktable
                                                          --> c_data_process
                                                          --> b_data_source
                                                          --> a_common

--------------------------------------------------------------------------------

2.1 a_common - 基础设施层
================================================================================
职责：API/WS网关、错误类型、配置、共享模型

关键子模块：
- api/          BinanceApiGateway、RateLimiter、SymbolRulesFetcher（REST）
- ws/           BinanceTradeStream、BinanceCombinedStream、BinanceWsConnector
- models/       MarketKLine、MarketTick、VolatilityInfo、OrderBookSnapshot
- claint/       EngineError、MarketError（基于thiserror）
- config/       平台检测、路径配置（Windows: E:/shm, Linux: /dev/shm）
- volatility/   VolatilityCalc、VolatilityStats、VolatilityRank
- backup/       MemoryBackup、memory_backup_dir（E:/shm/backup）
- logs/         CheckpointLogger、CompositeCheckpointLogger

关键模式：
- 网关模式：BinanceApiGateway用于REST，BinanceWsConnector用于WS
- 限流：Token bucket算法
- 平台抽象：Platform::detect()跨平台路径

接口（lib.rs导出）：
- BinanceApiGateway、RateLimiter、SymbolRulesFetcher
- BinanceTradeStream、BinanceWsConnector
- VolatilityCalc、VolatilityStats
- EngineError、MarketError

--------------------------------------------------------------------------------

2.2 b_data_source - 数据源层
================================================================================
职责：数据供给、K线合成、订单簿、波动率检测

关键子模块：
- api/          DataFeeder（统一接口）、SymbolRegistry、TradeSettings
- ws/           kline_1m、kline_1d、order_books、volatility
- symbol_rules/ SymbolRuleService、ParsedSymbolRules
- trader_pool/  SymbolMeta、TradingStatus、TraderPool
- replay_source/KLineSource、ReplaySource（历史数据回放）

关键模式：
- DataFeeder：统一数据访问接口（所有查询必须通过此处）
- K线合成：实时从Tick构建1m/15m/1d K线
- 品种注册表：交易对管理

接口（lib.rs导出）：
- DataFeeder、SymbolRegistry
- KLine、Period、Tick、MarketStream
- VolatilityManager、SymbolVolatility

--------------------------------------------------------------------------------

2.3 c_data_process - 信号生成层
================================================================================
职责：指标计算、信号生成、策略状态

关键子模块：
- pine_indicator_full/  PineColorDetector（Pine v5）、EMA、RSI、colors
- min/         分钟级策略输入/输出
- day/         日线级策略输入/输出
- processor/   SignalProcessor
- strategy_state/ StrategyStateManager、StrategyStateDb、PositionState

关键模式：
- Pine颜色检测器：基于MACD + EMA10/20 + RSI的趋势信号
- 增量指标：EMA、RSI增量计算
- 信号类型：LongEntry、ShortEntry、LongHedge、ShortHedge、LongExit、ShortExit

接口（lib.rs导出）：
- PineColorDetectorV5、EMA、RSI
- SignalProcessor
- StrategyStateManager、PositionState

--------------------------------------------------------------------------------

2.4 d_checktable - 检查层
================================================================================
职责：按周期组织的策略检查表（异步并发执行）

关键子模块：
- check_table/  CheckTable、CheckEntry（FnvHashMap存储）
- h_15m/        高频15分钟策略检查
- l_1d/         低频1天策略检查

关键模式：
- CheckEntry：记录策略判断结果（品种、策略ID、周期）
- 异步并发：检查并发执行，引擎层协调
- 基于周期：不同时间框架的不同策略

接口（lib.rs导出）：
- CheckTable、CheckEntry

--------------------------------------------------------------------------------

2.5 e_risk_monitor - 风控层
================================================================================
职责：风控、仓位管理、持久化

关键子模块：
- risk/         common、pin、trend、minute_risk
- position/     LocalPositionManager、PositionExclusionChecker
- persistence/  PersistenceService、SqliteEventRecorder、DisasterRecovery
- shared/       AccountPool、MarketStatusDetector、PnlManager

关键模式：
- 两级风控：预检（锁外）+ 精检（锁内）
- 仓位互斥：防止冲突仓位
- 灾难恢复：内存盘备份 + SQLite持久化

接口（lib.rs导出）：
- RiskPreChecker、RiskReChecker
- LocalPositionManager、PositionExclusionChecker
- PersistenceService、SqliteEventRecorder

--------------------------------------------------------------------------------

2.6 f_engine - 交易引擎运行时
================================================================================
职责：核心执行、订单管理、模式切换

关键子模块（七目录结构）：
- core/         TradingEngineV2、EngineState、StrategyPool、State
- order/        OrderExecutor、ExchangeGateway、MockBinanceGateway
- channel/      ModeSwitcher
- strategy/     StrategyExecutor
- interfaces/   所有跨模块通信的Trait定义
- types.rs      核心类型定义

交易引擎V2执行流程（V1.4）：
    1. 并行触发器检查（分钟级/日线级）
    2. StrategyQuery + 策略执行（2秒超时）
    3. 一级风控预检（锁外）
    4. 品种级锁获取（1秒超时）
    5. 二级风控精检（锁内）
    6. 冻结资金 + 下单
    7. 成交确认 + 状态同步
    8. 确认资金/回滚

接口（interfaces/模块）：
- MarketDataProvider、MarketKLine、MarketTick、VolatilityInfo
- StrategyExecutor、StrategyInstance、TradingSignal
- RiskChecker、RiskLevel、PositionInfo
- ExchangeGateway
- CheckTableProvider

关键类型：
- TradingEngineV2、TradingEngineConfig
- EngineState、EngineStatus、EngineMode、EngineMetricsSnapshot
- StrategyQuery、StrategyResponse、RiskCheckResult
- OrderInfo、FundPool、OrderLifecycle

--------------------------------------------------------------------------------

2.7 g_test - 测试层
================================================================================
职责：集成测试、功能测试

关键子模块：
- b_data_source/   b_data_source相关测试
- strategy/        策略层黑盒测试

--------------------------------------------------------------------------------

2.8 h_sandbox - 沙盒层
================================================================================
职责：实验代码、模拟、回测

关键子模块：
- config/       ShadowConfig
- simulator/    Account、OrderEngine、Position、ShadowRiskChecker
- gateway/      ShadowBinanceGateway
- tick_generator/ TickGenerator、TickDriver、SimulatedTick
- perf_test/    PerformanceTracker、PerfTickDriver、EngineDriver
- backtest/     BacktestStrategy、BacktestTick、MaCrossStrategy

================================================================================
3. 数据流
================================================================================

3.1 实时交易流程
--------------------------

    市场数据（WS）
         |
         v
    b_data_source（DataFeeder）
    - K线合成（1m/15m/1d）
    - 订单簿聚合
    - 波动率计算
         |
         v
    c_data_process（信号生成）
    - Pine颜色检测
    - EMA/RSI计算
    - 信号生成
         |
         v
    d_checktable（策略检查）
    - h_15m（高频15m检查）
    - l_1d（低频1d检查）
         |
         v
    f_engine（交易引擎）
    - 触发器检查
    - StrategyQuery（2秒超时）
    - 风控预检（锁外）
    - 锁获取（1秒超时）
    - 风控精检（锁内）
    - 资金冻结 + 下单
         |
         v
    e_risk_monitor（风控）
    - 仓位验证
    - 保证金检查
    - 断路器
         |
         v
    交易所网关（模拟/真实）
    - 订单提交
    - 成交确认

3.2 状态同步流程
--------------------------

    本地仓位 vs 交易所仓位
              |
              v
    状态同步器（TradingPipeline）
    - 数量检查
    - 价格偏差检查（<1%）
    - 强制同步到交易所值

================================================================================
4. 关键抽象/接口
================================================================================

4.1 交易所网关Trait
--------------------------
位置：f_engine/src/interfaces/execution.rs

职责：抽象交易所操作（下单、撤单、获取成交）

方法：
- async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, Error>
- async fn cancel_order(&self, order_id: &str) -> Result<(), Error>
- async fn get_fills(&self) -> Result<Vec<Fill>, Error>

实现：
- MockBinanceGateway（用于模拟）
- （真实交易所网关待实现）

4.2 市场数据Provider Trait
--------------------------
位置：f_engine/src/interfaces/market_data.rs

职责：抽象市场数据访问

方法：
- async fn next_tick(&self) -> Option<MarketTick>
- async fn next_completed_kline(&self) -> Option<MarketTick>
- fn current_price(&self, symbol: &str) -> Option<Decimal>
- async fn get_klines(&self, symbol: &str, period: &str) -> Vec<MarketKLine>

4.3 风控检查Trait
--------------------------
位置：f_engine/src/interfaces/risk.rs

职责：抽象风控检查逻辑

方法：
- fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult
- fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult
- fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>
- fn thresholds(&self) -> RiskThresholds

4.4 策略执行器Trait
--------------------------
位置：f_engine/src/interfaces/strategy.rs

职责：抽象策略执行和信号聚合

方法：
- fn register(&self, strategy: Arc<dyn StrategyInstance>)
- fn dispatch(&self, bar: &MarketKLine) -> Vec<TradingSignal>
- fn get_signal(&self, symbol: &str) -> Option<TradingSignal>
- fn get_all_states(&self) -> Vec<StrategyState>

================================================================================
5. 入口点
================================================================================

5.1 主程序入口
--------------------------
位置：src/main.rs

职责：程序入口，tracing初始化

5.2 库入口
--------------------------
每个crate通过lib.rs导出其公共API：
- a_common::lib.rs：基础设施组件
- b_data_source::lib.rs：数据供给接口
- c_data_process::lib.rs：指标和信号类型
- d_checktable::lib.rs：检查表功能
- e_risk_monitor::lib.rs：风控和仓位管理
- f_engine::lib.rs：交易引擎核心（主入口）

================================================================================
6. 错误处理模式
================================================================================

6.1 错误类型层次
--------------------------
位置：a_common/src/claint/error.rs

- MarketError：数据源错误（WS断开、API失败）
- EngineError：交易引擎错误（超时、锁失败）

6.2 TradingError枚举
--------------------------
位置：f_engine/src/core/engine_v2.rs

变体：
- EngineNotRunning
- InsufficientFunds
- RiskRejected(String)
- LockFailed
- OrderFailed(String)
- Timeout(String)
- StateInconsistent

================================================================================
7. 关键技术决策
================================================================================

【1】禁止unsafe代码
    所有crate使用 #![forbid(unsafe_code)]

【2】PARKING_LOT RWLOCK
    使用parking_lot::RwLock代替std::sync::RwLock，性能更好

【3】FNVHASHMAP
    O(1)查找用于仓位/状态管理

【4】RUST_DECIMAL
    金融计算避免浮点精度问题

【5】CHRONO DATETIME<UTC>
    所有时间戳使用UTC

【6】THISERROR
    结构化错误类型，带derive(Error)

【7】SERDE
    所有类型派生Serialize/Deserialize用于持久化

================================================================================
架构文档结束
================================================================================
