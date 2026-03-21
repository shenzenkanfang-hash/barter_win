# 目录重组详细迁移文档

## 1. 迁移后目录结构

```
crates/
├── a_common/              # 基础设施层 - 工具和接口，不生产数据
│   └── src/
│       ├── error.rs           # 通用错误类型
│       ├── platform.rs        # 平台检测工具
│       ├── check_table.rs     # 检查表工具
│       ├── checkpoint.rs       # 检查点工具
│       ├── checkpoint_integration.rs  # 检查点集成工具
│       ├── telegram_notifier.rs     # Telegram通知工具
│       ├── mock_binance_gateway.rs  # Mock网关(开发用)
│       └── lib.rs
│
├── b_data_source/        # 数据获取层 - 获取市场数据
│   └── src/
│       ├── binance_ws.rs          # Binance WebSocket连接
│       ├── data_feeder.rs          # 数据供给器
│       ├── kline.rs                # K线结构
│       ├── kline_persistence.rs    # K线持久化
│       ├── orderbook.rs            # 订单簿
│       ├── recovery.rs              # 恢复机制
│       ├── symbol_registry.rs       # 交易对注册
│       ├── volatility.rs            # 波动率计算
│       ├── websocket.rs             # WebSocket基础
│       ├── types.rs                # 数据类型
│       ├── error.rs                # 数据源错误类型
│       └── lib.rs
│
├── c_data_process/      # 数据加工层 - 指标计算
│   └── src/
│       ├── day/                     # 日线指标
│       │   ├── indicator_1d.rs          # 日线指标计算
│       │   ├── market_status_generator.rs # 日线市场状态生成
│       │   ├── price_control_generator.rs # 日线价格控制生成
│       │   ├── signal_generator.rs       # 日线信号生成
│       │   └── mod.rs
│       ├── min/                      # 分钟指标
│       │   ├── indicator_1m.rs          # 分钟指标计算
│       │   ├── market_status_generator.rs # 分钟市场状态生成
│       │   ├── price_control_generator.rs # 分钟价格控制生成
│       │   ├── signal_generator.rs       # 分钟信号生成
│       │   └── mod.rs
│       ├── pine_indicator_full.rs   # Pine指标完整版
│       ├── trading_trigger.rs       # 交易触发器
│       ├── volatility_rank.rs        # 波动率排名
│       ├── types.rs                 # 指标类型
│       └── lib.rs
│
├── d_risk_monitor/      # 风控监控层 - 风控检查和持仓监控
│   └── src/
│       ├── risk/                    # 风控子模块
│       │   ├── risk.rs                   # 风控预检
│       │   ├── risk_rechecker.rs          # 风控复核
│       │   ├── order_check.rs            # 订单检查
│       │   ├── thresholds.rs              # 阈值配置
│       │   ├── minute_risk.rs             # 分钟级风控
│       │   └── mod.rs
│       ├── position/                # 持仓子模块
│       │   ├── position_manager.rs        # 持仓管理
│       │   ├── position_exclusion.rs       # 持仓互斥检查
│       │   └── mod.rs
│       ├── shared/                  # 风控共享
│       │   ├── account_pool.rs           # 账户池
│       │   ├── margin_config.rs          # 保证金配置
│       │   ├── market_status.rs          # 市场状态
│       │   ├── pnl_manager.rs            # 盈亏管理
│       │   ├── round_guard.rs            # 回合守卫
│       │   └── mod.rs
│       ├── persistence/             # 持久化子模块
│       │   ├── sqlite_persistence.rs     # SQLite持久化
│       │   ├── memory_backup.rs          # 内存备份
│       │   ├── disaster_recovery.rs      # 灾备恢复
│       │   ├── persistence.rs           # 持久化服务
│       │   └── mod.rs
│       └── lib.rs
│
├── e_strategy/          # 策略执行层 - 数据消费者
│   └── src/
│       ├── strategy/                # 策略子模块
│       │   ├── pin_strategy.rs          # Pin策略
│       │   ├── trend_strategy.rs         # 趋势策略
│       │   ├── traits.rs                 # 策略特征
│       │   ├── types.rs                  # 策略类型
│       │   ├── error.rs                  # 策略错误类型
│       │   └── mod.rs
│       ├── core/                   # 核心引擎子模块
│       │   ├── engine.rs                 # 交易引擎
│       │   ├── pipeline.rs               # 交易管道
│       │   ├── pipeline_form.rs          # 管道表单
│       │   ├── strategy_pool.rs          # 策略池
│       │   └── mod.rs
│       ├── order/                  # 订单子模块
│       │   ├── gateway.rs                # 网关抽象
│       │   ├── order.rs                  # 订单执行器
│       │   └── mod.rs
│       ├── channel/                 # 通道子模块
│       │   ├── channel.rs               # 波动率通道
│       │   ├── mode.rs                  # 交易模式
│       │   └── mod.rs
│       ├── symbol/                  # 规则子模块
│       │   ├── symbol_rules.rs           # 交易规则
│       │   ├── symbol_rules_fetcher.rs   # 规则API拉取
│       │   └── mod.rs
│       └── lib.rs
│
├── g_test/              # 测试层
│   └── src/
│       ├── integration/             # 集成测试
│       │   ├── engine_test.rs
│       │   └── mod.rs
│       └── lib.rs
│
└── [旧目录待删除]
    ├── account/              # → a_common/
    ├── market/              # → b_data_source/
    ├── indicator/           # → c_data_process/
    ├── strategy/            # → e_strategy/strategy/
    └── engine/             # 拆分到 d_risk_monitor/ 和 e_strategy/
```

## 2. 文件迁移对应表

### 2.1 a_common/ (原 account/ + engine/shared/基础设施)

| 原路径 | 新路径 | 说明 |
|--------|--------|------|
| account/Cargo.toml | a_common/Cargo.toml | 重命名 |
| account/src/error.rs | a_common/src/error.rs | 通用错误 |
| account/src/lib.rs | a_common/src/lib.rs | 模块入口 |
| account/src/types.rs | a_common/src/types.rs | 通用类型 |
| engine/src/shared/error.rs | a_common/src/error.rs | 合并到已有 |
| engine/src/shared/platform.rs | a_common/src/platform.rs | 平台检测 |
| engine/src/shared/check_table.rs | a_common/src/check_table.rs | 检查表 |
| engine/src/shared/checkpoint.rs | a_common/src/checkpoint.rs | 检查点 |
| engine/src/shared/checkpoint_integration.rs | a_common/src/checkpoint_integration.rs | 检查点集成 |
| engine/src/shared/telegram_notifier.rs | a_common/src/telegram_notifier.rs | 通知 |
| engine/src/order/mock_binance_gateway.rs | a_common/src/mock_binance_gateway.rs | Mock网关 |

### 2.2 b_data_source/ (原 market/)

| 原路径 | 新路径 | 说明 |
|--------|--------|------|
| market/Cargo.toml | b_data_source/Cargo.toml | 重命名 |
| market/src/binance_ws.rs | b_data_source/src/binance_ws.rs | WebSocket |
| market/src/data_feeder.rs | b_data_source/src/data_feeder.rs | 数据供给 |
| market/src/error.rs | b_data_source/src/error.rs | 错误类型 |
| market/src/kline.rs | b_data_source/src/kline.rs | K线 |
| market/src/kline_persistence.rs | b_data_source/src/kline_persistence.rs | K线持久化 |
| market/src/lib.rs | b_data_source/src/lib.rs | 模块入口 |
| market/src/orderbook.rs | b_data_source/src/orderbook.rs | 订单簿 |
| market/src/recovery.rs | b_data_source/src/recovery.rs | 恢复 |
| market/src/symbol_registry.rs | b_data_source/src/symbol_registry.rs | 注册 |
| market/src/types.rs | b_data_source/src/types.rs | 类型 |
| market/src/volatility.rs | b_data_source/src/volatility.rs | 波动率 |
| market/src/websocket.rs | b_data_source/src/websocket.rs | WebSocket基础 |

### 2.3 c_data_process/ (原 indicator/)

| 原路径 | 新路径 | 说明 |
|--------|--------|------|
| indicator/Cargo.toml | c_data_process/Cargo.toml | 重命名 |
| indicator/src/day/indicator_1d.rs | c_data_process/src/day/indicator_1d.rs | 日线指标 |
| indicator/src/day/market_status_generator.rs | c_data_process/src/day/market_status_generator.rs | 日线状态 |
| indicator/src/day/mod.rs | c_data_process/src/day/mod.rs | 日线模块 |
| indicator/src/day/price_control_generator.rs | c_data_process/src/day/price_control_generator.rs | 日线价格控制 |
| indicator/src/day/signal_generator.rs | c_data_process/src/day/signal_generator.rs | 日线信号 |
| indicator/src/lib.rs | c_data_process/src/lib.rs | 模块入口 |
| indicator/src/min/indicator_1m.rs | c_data_process/src/min/indicator_1m.rs | 分钟指标 |
| indicator/src/min/market_status_generator.rs | c_data_process/src/min/market_status_generator.rs | 分钟状态 |
| indicator/src/min/mod.rs | c_data_process/src/min/mod.rs | 分钟模块 |
| indicator/src/min/price_control_generator.rs | c_data_process/src/min/price_control_generator.rs | 分钟价格控制 |
| indicator/src/min/signal_generator.rs | c_data_process/src/min/signal_generator.rs | 分钟信号 |
| indicator/src/pine_indicator_full.rs | c_data_process/src/pine_indicator_full.rs | Pine指标 |
| indicator/src/trading_trigger.rs | c_data_process/src/trading_trigger.rs | 交易触发 |
| indicator/src/types.rs | c_data_process/src/types.rs | 类型 |
| indicator/src/volatility_rank.rs | c_data_process/src/volatility_rank.rs | 波动率排名 |

### 2.4 d_risk_monitor/ (原 engine/risk/, engine/position/, engine/persistence/, engine/shared/风控相关)

| 原路径 | 新路径 | 说明 |
|--------|--------|------|
| engine/src/risk/minute_risk.rs | d_risk_monitor/src/risk/minute_risk.rs | 分钟风控 |
| engine/src/risk/mod.rs | d_risk_monitor/src/risk/mod.rs | 风控模块 |
| engine/src/risk/order_check.rs | d_risk_monitor/src/risk/order_check.rs | 订单检查 |
| engine/src/risk/risk.rs | d_risk_monitor/src/risk/risk.rs | 风控 |
| engine/src/risk/risk_rechecker.rs | d_risk_monitor/src/risk/risk_rechecker.rs | 风控复核 |
| engine/src/risk/thresholds.rs | d_risk_monitor/src/risk/thresholds.rs | 阈值 |
| engine/src/position/mod.rs | d_risk_monitor/src/position/mod.rs | 持仓模块 |
| engine/src/position/position_exclusion.rs | d_risk_monitor/src/position/position_exclusion.rs | 持仓互斥 |
| engine/src/position/position_manager.rs | d_risk_monitor/src/position/position_manager.rs | 持仓管理 |
| engine/src/persistence/disaster_recovery.rs | d_risk_monitor/src/persistence/disaster_recovery.rs | 灾备 |
| engine/src/persistence/memory_backup.rs | d_risk_monitor/src/persistence/memory_backup.rs | 内存备份 |
| engine/src/persistence/mod.rs | d_risk_monitor/src/persistence/mod.rs | 持久化模块 |
| engine/src/persistence/persistence.rs | d_risk_monitor/src/persistence/persistence.rs | 持久化服务 |
| engine/src/persistence/sqlite_persistence.rs | d_risk_monitor/src/persistence/sqlite_persistence.rs | SQLite |
| engine/src/shared/account_pool.rs | d_risk_monitor/src/shared/account_pool.rs | 账户池 |
| engine/src/shared/margin_config.rs | d_risk_monitor/src/shared/margin_config.rs | 保证金 |
| engine/src/shared/market_status.rs | d_risk_monitor/src/shared/market_status.rs | 市场状态 |
| engine/src/shared/pnl_manager.rs | d_risk_monitor/src/shared/pnl_manager.rs | PnL管理 |
| engine/src/shared/round_guard.rs | d_risk_monitor/src/shared/round_guard.rs | 回合守卫 |

### 2.5 e_strategy/ (原 strategy/, engine/core/, engine/order/业务, engine/channel/, engine/shared/策略相关)

| 原路径 | 新路径 | 说明 |
|--------|--------|------|
| strategy/Cargo.toml | e_strategy/Cargo.toml | 重命名 |
| strategy/src/error.rs | e_strategy/src/strategy/error.rs | 策略错误 |
| strategy/src/lib.rs | e_strategy/src/strategy/mod.rs | 策略模块入口 |
| strategy/src/pin_strategy.rs | e_strategy/src/strategy/pin_strategy.rs | Pin策略 |
| strategy/src/traits.rs | e_strategy/src/strategy/traits.rs | 策略特征 |
| strategy/src/trend_strategy.rs | e_strategy/src/strategy/trend_strategy.rs | 趋势策略 |
| strategy/src/types.rs | e_strategy/src/strategy/types.rs | 策略类型 |
| engine/Cargo.toml | e_strategy/Cargo.toml | 合并engine配置 |
| engine/src/lib.rs | e_strategy/src/lib.rs | engine入口改为e_strategy入口 |
| engine/src/core/engine.rs | e_strategy/src/core/engine.rs | 引擎 |
| engine/src/core/mod.rs | e_strategy/src/core/mod.rs | 核心模块 |
| engine/src/core/pipeline.rs | e_strategy/src/core/pipeline.rs | 管道 |
| engine/src/core/pipeline_form.rs | e_strategy/src/core/pipeline_form.rs | 管道表单 |
| engine/src/core/strategy_pool.rs | e_strategy/src/core/strategy_pool.rs | 策略池 |
| engine/src/order/gateway.rs | e_strategy/src/order/gateway.rs | 网关 |
| engine/src/order/mod.rs | e_strategy/src/order/mod.rs | 订单模块 |
| engine/src/order/order.rs | e_strategy/src/order/order.rs | 订单执行 |
| engine/src/channel/channel.rs | e_strategy/src/channel/channel.rs | 通道 |
| engine/src/channel/mode.rs | e_strategy/src/channel/mode.rs | 模式 |
| engine/src/channel/mod.rs | e_strategy/src/channel/mod.rs | 通道模块 |
| engine/src/shared/symbol_rules.rs | e_strategy/src/symbol/symbol_rules.rs | 交易规则 |
| engine/src/shared/symbol_rules_fetcher.rs | e_strategy/src/symbol/symbol_rules_fetcher.rs | 规则拉取 |

### 2.6 g_test/ (新建测试目录)

| 新路径 | 说明 |
|--------|------|
| g_test/Cargo.toml | 测试模块配置 |
| g_test/src/lib.rs | 测试模块入口 |
| g_test/src/integration/mod.rs | 集成测试模块 |
| g_test/src/integration/engine_test.rs | 引擎集成测试 |

## 3. 需要删除的目录

```
crates/
├── account/          # 全部迁移后删除
├── market/           # 全部迁移后删除
├── indicator/        # 全部迁移后删除
├── strategy/         # 全部迁移后删除
└── engine/           # 全部迁移后删除(拆分到d和e)
    └── src/          # 所有文件已迁移
```

**特别注意**: engine/Cargo.toml 和 engine/src/lib.rs 合并到 e_strategy/

## 4. 新的 workspace 结构

Cargo.toml 需要更新 members:

```toml
[workspace]
members = [
    "a_common",
    "b_data_source",
    "c_data_process",
    "d_risk_monitor",
    "e_strategy",
    "g_test",
]
```

## 5. 迁移顺序

1. **Phase 1**: 创建新目录结构
2. **Phase 2**: 迁移 a_common, b_data_source, c_data_process (基础设施层)
3. **Phase 3**: 迁移 d_risk_monitor (风控层)
4. **Phase 4**: 迁移 e_strategy (策略层)
5. **Phase 5**: 创建 g_test
6. **Phase 6**: 更新 Cargo.toml 和 workspace
7. **Phase 7**: 更新所有 import 路径
8. **Phase 8**: 删除旧目录
9. **Phase 9**: 编译验证

## 7. 文件统计验证

| 目标目录 | 文件数 | 说明 |
|----------|--------|------|
| a_common/ | 11 | account(4) + engine/shared基础设施(7) |
| b_data_source/ | 13 | market(13) |
| c_data_process/ | 16 | indicator(16) |
| d_risk_monitor/ | 21 | engine/risk(6) + engine/position(3) + engine/persistence(5) + engine/shared风控(5) + engine/2 |
| e_strategy/ | 23 | strategy(7) + engine/core(5) + engine/order业务(3) + engine/channel(3) + engine/shared/symbol(2) + engine/2 |
| g_test/ | 4 | 新建(4) |
| **总计** | **88** | |

**注意**: 实际文件82个，上述数字包含engine/Cargo.toml和engine/src/lib.rs合并计算

## 8. 层级依赖关系

```
a_common/      ← 所有层都依赖(工具层)
     ↑
b_data_source/ ← c_data_process 依赖
     ↑
c_data_process/ ← e_strategy 依赖
     ↑
e_strategy/   ← d_risk_monitor 依赖(订单执行后需要风控检查)
     ↓
d_risk_monitor/
```
