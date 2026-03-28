# 团队B - b_data 模块测试

## 负责测试点

### b_data_source (真实市场数据层)

| 序号 | 测试点ID | 测试内容 | 优先级 | 状态 |
|------|---------|---------|--------|------|
| 1 | BS-001 | Kline1mStream 1分钟K线WebSocket订阅 | P0 | 待测 |
| 2 | BS-002 | Kline1mStream 分片订阅 | P1 | 待测 |
| 3 | BS-003 | Kline1dStream 1天K线WebSocket订阅 | P0 | 待测 |
| 4 | BS-004 | DepthStream 订单簿深度流订阅 | P0 | 待测 |
| 5 | BS-005 | DepthStream BTC专订阅 | P1 | 待测 |
| 6 | BS-006 | Kline1m::KLinePersistence K线数据持久化 | P1 | 待测 |
| 7 | BS-007 | FuturesDataSyncer 账户数据同步 | P0 | 待测 |
| 8 | BS-008 | FuturesDataSyncer 测试网账户同步 | P1 | 待测 |
| 9 | BS-009 | SymbolRegistry 交易对注册与管理 | P1 | 待测 |
| 10 | BS-010 | TradeSettings 交易设置 | P1 | 待测 |
| 11 | BS-011 | MarketDataStore 内存数据存储写入 | P1 | 待测 |
| 12 | BS-012 | MarketDataStore 内存数据存储读取 | P1 | 待测 |
| 13 | BS-013 | VolatilityManager 波动率管理 | P1 | 待测 |
| 14 | BS-014 | HistoricalClock 回测时钟系统 | P1 | 待测 |
| 15 | BS-015 | ReplaySource 历史数据回放 | P1 | 待测 |
| 16 | BS-016 | TraderPool 品种池管理 | P2 | 待测 |
| 17 | BS-017 | CheckpointManager 数据恢复检查点 | P1 | 待测 |

### b_data_mock (模拟数据层)

| 序号 | 测试点ID | 测试内容 | 优先级 | 状态 |
|------|---------|---------|--------|------|
| 1 | BM-001 | MockApiGateway 模拟API网关创建 | P0 | 待测 |
| 2 | BM-002 | MockApiGateway 模拟账户数据 | P0 | 待测 |
| 3 | BM-003 | MockApiGateway 模拟持仓数据 | P0 | 待测 |
| 4 | BM-004 | MockAccount 模拟账户操作 | P0 | 待测 |
| 5 | BM-005 | MockAccount 模拟资金计算 | P1 | 待测 |
| 6 | BM-006 | Kline1mStream(mock) 模拟1分钟K线生成 | P0 | 待测 |
| 7 | BM-007 | Kline1dStream(mock) 模拟1天K线生成 | P0 | 待测 |
| 8 | BM-008 | DepthStream(mock) 模拟订单簿数据 | P1 | 待测 |
| 9 | BM-009 | KlineGenerator K线合成器 | P0 | 待测 |
| 10 | BM-010 | KlineGenerator 波动率模拟 | P1 | 待测 |
| 11 | BM-011 | MockMarketConnector 模拟市场连接器 | P1 | 待测 |
| 12 | BM-012 | MockConfig 模拟配置参数 | P1 | 待测 |
| 13 | BM-013 | DataFeeder 统一数据注入接口 | P1 | 待测 |
| 14 | BM-014 | SymbolRuleService 模拟交易对规则服务 | P1 | 待测 |
| 15 | BM-015 | ReplaySource(mock) 历史数据回放模拟 | P1 | 待测 |
| 16 | BM-016 | CheckpointManager(mock) 模拟恢复检查点 | P2 | 待测 |

## 执行日志存放路径
testbench/并行测试/执行日志/团队B_*.log

## 测试报告输出路径
testbench/并行测试/团队B_b_data/

## 开始时间: ________
## 预计完成时间: ________
## 实际完成时间: ________
## 执行人签字: ________
