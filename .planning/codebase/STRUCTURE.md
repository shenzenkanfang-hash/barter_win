================================================================
STRUCTURE.md - 目录结构文档
================================================================

Author: Claude Code
Created: 2026-03-28
Status: 初始版本
================================================================

一、项目根目录结构
=================

    D:\Rust项目\barter-rs-main\
    |
    +-- Cargo.toml           # Workspace配置
    +-- Cargo.lock
    +-- rustfmt.toml
    +-- src/                 # 主程序入口
    |   +-- main.rs
    |   +-- multi_engine.rs
    |
    +-- crates/              # 所有crate源码
    |   +-- a_common/       # 基础设施层
    |   +-- x_data/         # 数据状态层
    |   +-- b_data_source/  # 业务数据层
    |   +-- c_data_process/  # 数据处理层
    |   +-- d_checktable/   # 检查层
    |   +-- e_risk_monitor/ # 风控层
    |   +-- f_engine/       # 引擎层
    |   +-- g_test/          # 测试工具
    |
    +-- .planning/          # 项目规划文档
    +-- data/                # 运行时数据
    +-- deploy/              # 部署配置
    +-- sandbox/             # 沙盒相关
    +-- venv/                # Python虚拟环境


二、crates目录布局
==================

2.1 a_common/ - 基础设施层

    crates/a_common/
    |
    +-- src/
    |   +-- lib.rs
    |   +-- error.rs
    |   +-- api/             # Binance API网关
    |   +-- ws/              # WebSocket连接
    |   +-- config/          # 配置管理
    |   +-- logs/            # 日志基础设施
    |   +-- models/          # 通用数据模型
    |   +-- claint/          # 错误类型
    |   +-- util/            # 工具函数
    |   +-- backup/          # 备份类型定义
    |   +-- exchange/        # 交易所抽象
    |   +-- volatility/      # 波动率基础类型
    |
    +-- Cargo.toml

2.2 x_data/ - 数据状态层

    crates/x_data/
    |
    +-- src/
    |   +-- lib.rs
    |   +-- state/           # 状态管理
    |   +-- ...
    |
    +-- Cargo.toml

2.3 b_data_source/ - 业务数据层

    crates/b_data_source/
    |
    +-- src/
    |   +-- lib.rs           # 主入口
    |   +-- api/             # REST API接口
    |   |   +-- mock_api/    # Mock API网关
    |   |   +-- symbol_registry/
    |   |   +-- trade_settings/
    |   |   +-- DataFeeder.rs
    |   |
    |   +-- ws/              # WebSocket接口
    |   |   +-- mock_ws/     # Mock WS (StreamTickGenerator)
    |   |   +-- VolatilityManager.rs
    |   |
    |   +-- store/          # MarketDataStore实现
    |   +-- recovery/       # 检查点恢复
    |   +-- trader_pool/    # 品种池
    |   +-- replay_source/  # 历史回放
    |   +-- history/        # 历史数据管理
    |   +-- symbol_rules/   # 交易对规则
    |   +-- models/         # 业务数据类型
    |   +-- trader_pool.rs
    |   +-- replay_source.rs
    |
    +-- Cargo.toml

2.4 c_data_process/ - 数据处理层

    crates/c_data_process/
    |
    +-- src/
    |   +-- lib.rs
    |   +-- processor.rs     # SignalProcessor核心
    |   +-- types.rs         # 类型定义
    |   +-- pine_indicator_full.rs  # Pine指标
    |   +-- min/             # 分钟K线处理
    |   +-- day/             # 日K线处理
    |   +-- strategy_state/ # 策略状态
    |
    +-- Cargo.toml

2.5 d_checktable/ - 检查层

    crates/d_checktable/
    |
    +-- src/
    |   +-- lib.rs
    |   +-- check_table.rs
    |   +-- types.rs
    |   +-- recovery.rs
    |   +-- h_15m/           # 高频15分钟策略
    |   +-- l_1d/            # 低频1天策略
    |
    +-- Cargo.toml

2.6 e_risk_monitor/ - 风控层

    crates/e_risk_monitor/
    |
    +-- src/
    |   +-- lib.rs
    |   +-- risk/            # 风控模块
    |   |   +-- common/      # 通用风控
    |   |   +-- pin/         # PIN风险
    |   |   +-- trend/       # 趋势风险
    |   |   +-- minute_risk/ # 分钟风控
    |   |
    |   +-- position/       # 持仓管理
    |   +-- persistence/     # 持久化
    |   +-- shared/          # 共享状态
    |
    +-- Cargo.toml

2.7 f_engine/ - 引擎层

    crates/f_engine/
    |
    +-- src/
    |   +-- lib.rs
    |   +-- types.rs         # 核心类型
    |   +-- core/            # 基础引擎
    |   +-- event/           # 事件驱动引擎 (推荐)
    |   +-- interfaces/      # 接口定义
    |   +-- strategy/        # 策略管理
    |
    +-- Cargo.toml

2.8 g_test/ - 测试工具

    crates/g_test/
    +-- Cargo.toml


三、命名约定
============

3.1 Crate命名

    a_common      - 基础设施公共模块
    b_data_source - 业务数据源
    c_data_process - 数据处理
    d_checktable  - 检查表/调度
    e_risk_monitor - 风险监控
    f_engine      - 交易引擎
    g_test        - 测试工具
    x_data        - 数据状态

3.2 模块命名

    - 小写下划线：api, ws, risk, position
    - 描述性名称：trader_pool, symbol_rules, volatility
    - 特定用途：mock_api, mock_ws, replay_source

3.3 类型命名

    结构体：UpperCamelCase
    - MarketDataStore, SignalProcessor, EventEngine

    枚举：UpperCamelCase
    - OrderStatus, Side, OrderType

    trait：UpperCamelCase
    - RiskChecker, DataFeeder, MarketConnector

    函数/方法：小写下划线
    - write_kline, get_volatility, place_order

3.4 文件命名

    - lib.rs：crate主入口
    - 模块文件：snake_case.rs
    - 子模块目录：snake_case/

3.5 宏命名

    - store_write_kline!：存储写入宏
    - store_get_kline!：存储读取宏
    - store_get_volatility!：波动率读取宏


四、关键文件位置
================

4.1 入口点

    src/main.rs              - 主程序入口
    src/multi_engine.rs      - 多引擎示例

4.2 核心trait定义

    b_data_source/src/store/           - MarketDataStore trait
    b_data_source/src/api/             - DataFeeder trait
    b_data_source/src/ws/             - MarketConnector trait
    e_risk_monitor/src/risk/common/   - RiskChecker trait

4.3 核心实现

    b_data_source/src/store/market_data_store.rs  - 存储实现
    c_data_process/src/processor.rs                - 信号处理
    f_engine/src/event/                           - 事件引擎

4.4 全局单例

    b_data_source/src/lib.rs:
    - DEFAULT_STORE (OnceCell<Arc<MarketDataStoreImpl>>)
    - default_store() 函数
    - store_write_kline! / store_get_kline! / store_get_volatility! 宏

4.5 Mock组件

    b_data_source/src/ws/mock_ws/:
    - StreamTickGenerator
    - SimulatedTick
    - TickHandshakeChannel

    b_data_source/src/api/mock_api/:
    - MockApiGateway
    - OrderEngine
    - Account
    - MockRiskChecker

4.6 指标计算

    c_data_process/src/pine_indicator_full.rs:
    - PineColorDetector (V5)
    - EMA, RSI计算

4.7 风控模块

    e_risk_monitor/src/risk/:
    - common/: RiskPreChecker, RiskReChecker
    - pin/: PinRiskLeverageGuard
    - trend/: TrendRiskLimitGuard
    - minute_risk/: 分钟级风控计算


五、编译配置
============

5.1 Workspace配置 (Cargo.toml根目录)

    members:
    - a_common, b_data_source, c_data_process
    - d_checktable, e_risk_monitor, f_engine
    - g_test, x_data

5.2 共享依赖 (workspace.dependencies)

    parking_lot = "0.12"
    rust_decimal = { version = "1.36", features = ["maths"] }
    thiserror = "2.0"
    tracing = "0.1"
    chrono = { version = "0.4", features = ["serde"] }
    tokio = { version = "1", features = ["full"] }
    serde = { version = "1.0", features = ["derive"] }


六、配置目录
============

6.1 .planning/

    .planning/
    +-- PROJECT.md           - 项目总览
    +-- ROADMAP.md           - 路线图
    +-- milestones/          - 里程碑目录
    +-- codebase/            - 代码库文档
        +-- ARCHITECTURE.md  - 本文档
        +-- STRUCTURE.md     - 本文档

6.2 data/ - 运行时数据

    data/                   - K线、指标、持仓等CSV文件

6.3 deploy/ - 部署配置

    deploy/                 - 部署脚本和配置

================================================================
