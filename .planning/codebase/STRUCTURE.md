项目结构
=================

顶层结构
=========

交易系统 v5.5

Cargo.toml          - 定义所有 crate 的 workspace 清单
src/main.rs         - 交易系统的单一入口点
crates/             - 所有 crate 源代码目录


Crate 结构
===============

a_common
--------
共享基础 crate，包含通用工具模块。

主要模块:
- api/          - 通用 API 工具和类型
- ws/           - WebSocket 通用基础设施
- config/       - 配置加载和管理
- backup/       - 备份和恢复工具
- heartbeat/    - 心跳监控
- claint/       - 客户端接口抽象
- volatility/   - 波动率计算
- logs/         - 结构化日志工具
- models/       - 通用数据模型
- exchange/     - 交易所相关通用类型


b_data_source
--------------
真实交易所数据的市场数据源实现。

主要模块:
- api/                    - REST API 客户端实现
- ws/kline_1m/           - WebSocket 1分钟 K 线数据
- ws/kline_1d/           - WebSocket 1天 K 线数据
- ws/order_books/        - WebSocket 订单簿数据
- store/                 - 核心市场数据存储
  - store_trait.rs       - MarketDataStore trait 定义（关键 trait 位置）
- replay_source.rs       - 历史数据回放实现


b_data_mock
-----------
模拟市场数据源，结构与 b_data_source 对齐。

包含用于测试的模拟实现，无需真实交易所连接。
模块结构与 b_data_source 相同，但使用模拟数据。


c_data_process
--------------
数据处理和指标计算。

主要模块:
- pine_indicator_full.rs - 完整 Pine 脚本指标实现
- processor.rs          - 数据处理器核心
- min/                  - 分钟级处理
- day/                  - 日级处理
- strategy_state/       - 策略状态管理


d_checktable
------------
交易检查表和验证逻辑。

主要模块:
- h_15m/                - 15分钟时间框架交易检查
- l_1d/                 - 1天时间框架交易检查
- h_volatility_trader/  - 高波动率交易策略

关键类型:
- StoreRef 类型别名位于 d_checktable/src/h_15m/trader.rs


e_risk_monitor
--------------
风险监控和控制系统。

主要模块:
- risk/common/         - 通用风险工具
- risk/pin/            - Pin 风险监控
- risk/trend/          - 趋势风险监控
- position/            - 仓位风险管理
- persistence/         - 风险状态持久化
- shared/              - 共享风险组件


f_engine
--------
交易引擎核心实现。

主要模块:
- event/               - 事件处理引擎
  - event_engine.rs    - EventEngine 定义
                        - 关键 trait 位置: Strategy trait
                        - 关键 trait 位置: ExchangeGateway trait
- interfaces/         - 接口定义
- core/                - 引擎核心实现


x_data
------
共享领域类型和数据结构。

主要模块:
- position/            - 仓位领域类型
- account/            - 账户领域类型
- market/             - 市场数据领域类型
- trading/            - 交易领域类型
- state/              - 状态管理类型


关键 Trait 位置
==================

MarketDataStore trait
b_data_source/src/store/store_trait.rs

定义市场数据操作的仓库接口。
所有市场数据访问都通过此 trait 进行抽象。


Strategy trait
f_engine/src/event/event_engine.rs

定义交易策略接口。
实现提供 decide() 方法用于信号生成。


ExchangeGateway trait
f_engine/src/event/event_engine.rs

定义交易所连接接口。
实现处理订单下单、取消和市场数据获取。


单一入口点
==================

src/main.rs

交易系统 v5.5

所有系统初始化和事件循环从这里开始。
运行交易系统时不应使用其他入口点。


StoreRef 类型别名
===================

d_checktable/src/h_15m/trader.rs

StoreRef 是引用市场数据存储的类型别名。
在 checktable 层中用于访问市场数据。
