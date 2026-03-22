# 目录重组迁移计划

> **注意**: 此文档描述的是迁移计划。实际完成的结构有所不同：
> - 策略蓝图层: `d_blueprint` (而非 `e_strategy`)
> - 风控层: `e_risk_monitor` (而非 `d_risk_monitor`)

## 1. 核心设计理念

### 1.1 数据流架构
```
数据流:
b_data_source (获取) → c_data_process (加工) → e_strategy (消费)
                                          ↓
                                     d_risk_monitor (风控监控)
```

### 1.2 模块角色
| 目录 | 角色 | 职责 |
|------|------|------|
| a_common/ | 基础设施层 | 只提供工具和接口，不生产数据 |
| b_data_source/ | 数据获取者 | 获取市场数据(WebSocket、K线等) |
| c_data_process/ | 数据加工者 | 指标计算、数据处理 |
| d_risk_monitor/ | 风控监控者 | 风控检查、持仓监控、持久化 |
| e_strategy/ | 数据消费者 | 策略决策、订单执行(不获取/处理数据) |

### 1.3 重组原则
1. **a_common/** = 基础设施，被所有层使用
2. **b_data_source/** = 生产原始数据
3. **c_data_process/** = 生产加工后数据
4. **d_risk_monitor/** = 风控数据消费者 + 生产风控报告
5. **e_strategy/** = 最终消费者，不生产数据

## 2. 目标结构

| 新目录 | 角色 | 对应原模块 |
|--------|------|------------|
| a_common/ | 基础设施层(工具/接口) | account/ + engine/shared/基础设施 |
| b_data_source/ | 数据获取者 | market/ |
| c_data_process/ | 数据加工者 | indicator/ |
| d_risk_monitor/ | 风控监控者 | engine/risk/, engine/position/, engine/persistence/, engine/shared/风控相关 |
| e_strategy/ | 数据消费者 | strategy/, engine/core/, engine/order/, engine/channel/, engine/shared/策略相关 |

## 3. 当前结构

```
crates/
├── account/      # 账户层 (→ a_common)
├── market/       # 市场数据层 (→ b_data_source)
├── indicator/    # 指标层 (→ c_data_process)
├── strategy/     # 策略层 (→ e_strategy)
└── engine/       # 引擎层 (拆分到 d_risk_monitor + e_strategy)
    ├── core/           # 核心引擎 → e_strategy
    ├── risk/           # 风控 → d_risk_monitor
    ├── order/          # 订单 → e_strategy
    ├── position/       # 持仓 → d_risk_monitor
    ├── persistence/    # 持久化 → d_risk_monitor
    ├── channel/       # 通道 → e_strategy
    └── shared/        # 共享 (拆分)
```

## 4. 重组后结构

```
crates/
├── a_common/          # [新建] 通用基础层 (非业务)
│   └── src/
│       ├── lib.rs
│       ├── error.rs           # 错误类型
│       ├── platform.rs        # 平台检测
│       ├── account_pool.rs    # 账户池
│       ├── check_table.rs     # 检查表
│       ├── checkpoint.rs      # 检查点
│       ├── checkpoint_integration.rs  # 检查点集成
│       ├── telegram_notifier.rs  # Telegram通知
│       └── mock_binance_gateway.rs  # Mock网关 (开发用)
│
├── b_data_source/      # [新建] 数据源层
│   └── src/
│       ├── lib.rs
│       ├── binance_ws.rs
│       ├── data_feeder.rs
│       ├── kline.rs
│       ├── kline_persistence.rs
│       ├── orderbook.rs
│       ├── recovery.rs
│       ├── symbol_registry.rs
│       ├── volatility.rs
│       ├── websocket.rs
│       └── types.rs
│
├── c_data_process/     # [新建] 数据处理层
│   └── src/
│       ├── lib.rs
│       ├── pine_indicator_full.rs
│       ├── trading_trigger.rs
│       ├── types.rs
│       └── volatility_rank.rs
│
├── d_risk_monitor/     # [新建] 风控监控层
│   └── src/
│       ├── lib.rs
│       ├── risk/             # 风控子模块
│       │   ├── mod.rs
│       │   ├── risk.rs
│       │   ├── risk_rechecker.rs
│       │   ├── order_check.rs
│       │   ├── thresholds.rs
│       │   └── minute_risk.rs
│       ├── position/         # 持仓子模块
│       │   ├── mod.rs
│       │   ├── position_manager.rs
│       │   └── position_exclusion.rs
│       ├── shared/           # 共享子模块 (风控相关)
│       │   ├── mod.rs
│       │   ├── margin_config.rs
│       │   ├── round_guard.rs
│       │   ├── market_status.rs
│       │   └── pnl_manager.rs
│       └── persistence/      # 持久化子模块
│           ├── mod.rs
│           ├── sqlite_persistence.rs
│           ├── memory_backup.rs
│           ├── disaster_recovery.rs
│           └── persistence.rs
│
├── e_strategy/         # [新建] 策略执行层 (主要关注)
│   └── src/
│       ├── lib.rs
│       ├── core/            # 核心引擎
│       │   ├── mod.rs
│       │   ├── engine.rs
│       │   ├── pipeline.rs
│       │   ├── pipeline_form.rs
│       │   └── strategy_pool.rs
│       ├── order/           # 订单子模块
│       │   ├── mod.rs
│       │   ├── gateway.rs
│       │   └── order.rs
│       ├── channel/         # 通道子模块
│       │   ├── mod.rs
│       │   ├── channel.rs
│       │   └── mode.rs
│       └── symbol/          # 规则子模块
│           ├── mod.rs
│           ├── symbol_rules.rs
│           └── symbol_rules_fetcher.rs
│
└── g_test/              # [新建] 测试层
    └── src/
        ├── lib.rs
        └── integration/
            ├── mod.rs
            └── engine_test.rs
```

## 5. 实施步骤

### Phase 1: 创建目录结构
1. 创建 a_common/, b_data_source/, c_data_process/, d_risk_monitor/, e_strategy/, g_test/

### Phase 2: 移动代码文件
| 源目录 | 目标目录 | 说明 |
|--------|----------|------|
| account/ | a_common/ | 基础设施 |
| market/ | b_data_source/ | 数据获取 |
| indicator/ | c_data_process/ | 数据加工 |
| engine/risk/, engine/position/, engine/persistence/ | d_risk_monitor/ | 风控监控 |
| engine/shared/ (部分) | d_risk_monitor/shared/ | 风控相关共享 |
| engine/shared/ (部分) | a_common/ | 基础设施共享 |
| strategy/, engine/core/, engine/order/(除mock), engine/channel/, engine/shared/symbol_* | e_strategy/ | 策略执行 |

### Phase 3: 更新 Cargo.toml 和 import 路径

### Phase 4: 编译验证

## 6. 测试目录位置

**推荐**: g_test/ 独立测试层
