================================================================
ARCHITECTURE.md - 交易系统架构文档
================================================================

Author: Claude Code
Created: 2026-03-28
Status: 初始版本
================================================================

一、架构模式
============

1.1 分层架构（6层 + 1测试）

    a_common (基础设施层)
         |
    x_data (数据状态层)
         |
    b_data_source (业务数据层)
         |
    c_data_process (数据处理层)
         |
    d_checktable (检查层)
         |
    e_risk_monitor (风控层)
         |
    f_engine (引擎层)
         |
    g_test (测试工具)

1.2 依赖方向

    a_common --> x_data --> b_data_source --> c_data_process --> d_checktable --> e_risk_monitor --> f_engine
         ^                                                                  |
         +------------------------------------------------------------------+
                                        (可选回退)

1.3 核心设计原则

    - 高频路径无锁：Tick接收、指标更新、策略判断全部无锁
    - 锁仅用于下单和资金更新
    - 增量计算O(1)：EMA、SMA、MACD等指标必须增量计算
    - 混合持仓模式：资金池RwLock保护（低频），策略持仓独立计算（无锁）


二、各层职责
============

2.1 a_common - 基础设施层

    职责：
    - API/WS网关抽象
    - 通用错误类型定义
    - 配置管理
    - 日志基础设施
    - 备份数据类型定义
    - 波动率计算基础

    关键模块：
    - api/: Binance API网关、限流器、交易对规则
    - ws/: WebSocket连接器、市场数据流
    - config/: 平台配置、路径配置
    - logs/: 检查点日志、阶段追踪
    - models/: 通用数据模型
    - volatility/: 波动率统计基础类型

2.2 x_data - 数据状态层

    职责：
    - 统一状态管理
    - 系统快照
    - 状态查询接口

    关键模块：
    - state/: StateViewer, StateManager, UnifiedStateView, SystemSnapshot

2.3 b_data_source - 业务数据层

    职责：
    - 市场数据摄取
    - K线合成与存储
    - 订单簿数据
    - 波动率检测
    - 历史数据回放
    - Mock数据生成

    关键模块：
    - ws/: WebSocket数据接口、波动率管理器
    - api/: REST API接口、交易设置
    - store/: MarketDataStore（统一存储接口）
    - mock_ws/: StreamTickGenerator（回放/模拟）
    - mock_api/: MockApiGateway（模拟交易所）
    - recovery/: 检查点管理、Redis恢复
    - trader_pool/: 品种池管理
    - history/: 历史数据管理

    全局单例：
    - default_store(): 全局MarketDataStore实例

2.4 c_data_process - 数据处理层

    职责：
    - K线处理（分钟级、日级）
    - Pine指标计算
    - 策略状态管理
    - 信号生成

    关键模块：
    - processor/: SignalProcessor（信号处理器）
    - pine_indicator_full/: Pine颜色检测器、EMA、RSI
    - min/: 分钟K线处理
    - day/: 日K线处理
    - strategy_state/: 策略状态持久化

    导出类型：
    - PineColorDetector (V5版本)
    - SignalProcessor
    - StrategyStateManager

2.5 d_checktable - 检查层

    职责：
    - 按周期组织的策略检查
    - 高频15分钟Trader
    - 低频1天策略检查

    关键模块：
    - h_15m/: 高频15分钟策略检查
    - l_1d/: 低频1天策略检查
    - check_table/: CheckTable、CheckEntry

    类型：
    - CheckChainContext, CheckSignal, CheckChainResult

2.6 e_risk_monitor - 风控层

    职责：
    - 风险预检/复检
    - 持仓管理
    - 持久化服务
    - 共享状态管理（账户池、保证金配置）

    关键模块：
    - risk/common/: RiskPreChecker、VolatilityMode、RiskReChecker
    - risk/pin/: PinRiskLeverageGuard（杠杆守护）
    - risk/trend/: TrendRiskLimitGuard（趋势风险限制）
    - risk/minute_risk/: 分钟级风控计算
    - position/: LocalPositionManager（持仓管理）
    - persistence/: 事件记录、灾难恢复
    - shared/: AccountPool、MarginConfig、MarketStatus

2.7 f_engine - 引擎层

    职责：
    - 交易引擎核心
    - 事件驱动架构
    - 策略调度
    - 订单执行

    关键模块：
    - event/: EventEngine（推荐使用）
    - core/: 基础引擎、协程管理
    - strategy/: TraderManager（多品种管理）
    - types/: 核心类型定义

    推荐用法：
    - EventEngine + EventBus 事件驱动模式

    类型：
    - StrategyId, TradingDecision, OrderRequest, Side, OrderType, TradingAction


三、数据流
==========

3.1 实时交易数据流

    WebSocket K线/Tick
         |
         v
    b_data_source (ws module)
         |
         v
    MarketDataStore (全局存储)
         |
         v
    c_data_process (processor)
         |
         v
    d_checktable (按周期检查)
         |
         v
    e_risk_monitor (风控检查)
         |
         v
    f_engine (订单执行)
         |
         v
    MockApiGateway / Binance API

3.2 回放/沙盒数据流

    StreamTickGenerator (历史数据回放)
         |
         v
    MarketDataStore (独立实例)
         |
         v
    MockApiGateway (模拟交易所)
         |
         v
    本地账户/持仓/PnL更新

3.3 风控检查点

    OrderRequest
         |
         +--> RiskPreChecker (开仓前预检)
         |
         +--> PinRiskLeverageGuard (杠杆守护)
         |
         +--> TrendRiskLimitGuard (趋势限制)
         |
         +--> RiskReChecker (下单前复检)
         |
         v
    OrderResult / RejectReason


四、抽象接口
============

4.1 数据存储接口

    MarketDataStore trait:
    - write_kline(symbol, kline, closed)
    - get_current_kline(symbol)
    - get_volatility(symbol)
    - write_depth(symbol, depth)
    - get_orderbook(symbol)

4.2 数据源接口

    DataFeeder trait:
    - 统一数据注入接口

    MarketConnector trait:
    - connect(), disconnect(), subscribe()

    HistoryDataProvider trait:
    - fetch_history(request) -> HistoryResponse

4.3 风控接口

    RiskChecker trait:
    - pre_check(order) -> OrderCheckResult
    - re_check(order) -> OrderCheckResult

4.4 交易接口

    ExchangeGateway trait:
    - place_order(request) -> OrderResult
    - cancel_order(symbol, order_id)
    - get_position(symbol)


五、入口点
==========

5.1 主程序入口

    src/main.rs
    src/multi_engine.rs

    初始化流程：
    1. tracing初始化
    2. 配置加载
    3. 创建EventEngine
    4. 启动WebSocket连接
    5. 进入事件循环

5.2 bin入口

    trading-system (src/main.rs)


六、技术栈
==========

    Runtime: Tokio (异步IO、多线程任务调度)
    状态管理: FnvHashMap (O(1)查找)
    同步原语: parking_lot (比std RwLock更高效)
    数值计算: rust_decimal (金融计算避免浮点精度问题)
    时间处理: chrono (DateTime<Utc>)
    错误处理: thiserror (清晰的错误类型层次)
    日志: tracing (结构化日志)
    序列化: serde (Serialize/Deserialize)


七、关键设计决策
================

7.1 沙盒与生产共代码

    沙盒和生产用同一套代码，只是数据来源不同：
    - 生产：WS实盘数据 -> Store，API发真实Binance
    - 沙盒：WS历史回放数据 -> Store，API发MockApiGateway

7.2 mock_api组件

    MockApiGateway: 模拟API网关
    OrderEngine: 订单执行引擎
    Account: 账户状态机
    MockRiskChecker: 风控检查（Strict/Audit/Bypass模式）

7.3 mock_ws组件

    StreamTickGenerator: K线生成Tick流（Iterator模式）
    GaussianNoise: 高斯噪声生成

================================================================
