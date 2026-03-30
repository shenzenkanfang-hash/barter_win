架构文档
=====================

层级依赖链
=====================

a_common -> x_data -> b_data_source -> c_data_process -> d_checktable -> e_risk_monitor -> f_engine

系统遵循严格单向数据流通过这些层级。
每个层级只依赖其直接左侧的层级。


设计模式
===============

网关模式
---------------
用于交易所连接抽象。具体实现：
- BinanceApiGateway: REST API 连接 Binance
- MockApiGateway: 用于测试的模拟交易所
- BinanceWsConnector: 用于实时数据的 WebSocket 连接器

这些网关将交易所接口从系统其余部分抽象出来。


仓库模式
------------------
MarketDataStore trait 定义了市场数据的仓库接口。
位置: b_data_source/src/store/store_trait.rs

所有市场数据访问都通过此 trait，实现：
- 真实数据源 (b_data_source)
- 模拟数据源 (b_data_mock)
- 回测用的重放数据源

此 trait 定义了 K线、订单簿和成交的标准 CRUD 操作。


流水线模式
----------------
EventEngine 中的事件处理流水线：

tick -> update_store -> calc_indicators -> decide -> risk_check -> place_order

每个阶段是独立的处理步骤：
1. tick: 市场数据 tick 到达
2. update_store: 更新内部市场数据存储
3. calc_indicators: 计算技术指标
4. decide: 策略决策逻辑
5. risk_check: 风险验证
6. place_order: 订单执行


观察者模式
---------------
EventBus 通过 mpsc 通道分发实现观察者模式。

组件订阅它们关心的事件。当事件发布时，通过通道分发给所有订阅者。这将事件生产者与消费者解耦。


关键架构约束
=============================

a_common 必须不包含业务类型
---------------------------------------
a_common crate 是共享基础 crate。它必须不包含：
- TradingDecision
- OrderRequest
- LocalPosition
- 任何其他业务域类型

业务类型应放在 x_data（共享域类型）或各自的功能 crate 中。


仅允许增量 O(1) 计算
---------------------------------
所有热路径计算必须是 O(1) 复杂度的增量计算。
每次 tick 不允许全量重算。例如：
- 指标更新使用增量公式
- 持仓更新使用增量计算

这确保了无论数据量如何都能保持一致的低延迟处理。


热路径无锁设计
------------------------
热路径（tick 处理）必须是无锁的以最小化延迟。
仅冷路径操作可使用 parking_lot::RwLock 共享状态。

热路径特征：
- 单生产者，多消费者
- 无锁数据结构
- 无阻塞操作


热路径中零 tokio::spawn
-----------------------------
热路径中不允许异步生成。热路径：
- 必须不分配或生成任务
- 在单个异步上下文中使用同步处理
- 异步/等待仅用于可容忍延迟的 I/O 操作


零轮询（recv().await 阻塞）
------------------------------------
热路径必须不使用基于轮询的接收（recv().await）。相反：
- 使用有界延迟的阻塞通道接收
- 设计背压处理
- 避免忙等待或唤醒模式。


依赖注入
====================

EventEngine 泛型于两个关键 trait：

EventEngine<S, G> 其中：
- S: Strategy trait - 定义交易策略接口
- G: ExchangeGateway trait - 定义交易所连接接口

这实现了：
- 可插入的不同策略
- 不同的交易所网关（实盘、模拟、重放）
- 使用模拟实现轻松测试


版本追踪
================

AtomicU64 版本控制系统用于数据 lineage 追踪：
- data version: 原始市场数据版本
- indicator version: 指标计算版本
- signal version: 信号生成版本
- decision version: 交易决策版本

这实现了：
- 无需全量比较即可检测变更
- 版本未变更时增量处理
- 数据流的调试和追踪


a_common 从 x_data 的重导出
===============================

为开发者方便，a_common 从 x_data 重导出特定类型。
这减少了导入变动并提供了稳定的内部 API 表面。

所有重导出都有文档记录和版本控制以防止破坏性变更。
