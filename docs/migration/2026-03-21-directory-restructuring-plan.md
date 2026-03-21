# 目录重组迁移计划

## 1. 目标结构

| 新目录 | 功能划分 | 对应原模块 |
|--------|----------|------------|
| a_common/ | 通用基础层：账户类型、错误定义、平台检测、工具函数 | account/ |
| b_data_source/ | 数据源层：WebSocket、K线、市场数据、订单簿 | market/ |
| c_data_process/ | 数据处理层：指标计算、Pine颜色、波动率排名 | indicator/ |
| d_risk_monitor/ | 风控监控层：风控、持仓管理、保证金、回合守卫 | engine/risk/, engine/position/, engine/shared/ |
| e_strategy/ | 策略执行层：策略逻辑、核心引擎、订单执行、通道判断 | strategy/, engine/core/, engine/order/, engine/channel/ |
| g_test/ | 测试层：集成测试、模拟网关测试 | 测试代码 |

## 2. 当前结构

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
    ├── persistence/    # 持久化 → 特殊处理
    ├── channel/       # 通道 → e_strategy
    └── shared/        # 共享 (拆分)
```

## 3. 重组后结构

```
crates/
├── a_common/          # [新建] 通用基础层
│   └── src/
│       ├── lib.rs
│       ├── error.rs         # 错误类型
│       └── types.rs         # 通用类型
│
├── b_data_source/      # [新建] 数据源层
│   └── src/
│       ├── lib.rs
│       ├── binance_ws.rs    # Binance WebSocket
│       ├── data_feeder.rs   # 数据供给
│       ├── kline.rs         # K线结构
│       ├── kline_persistence.rs  # K线持久化
│       ├── orderbook.rs      # 订单簿
│       ├── recovery.rs       # 恢复机制
│       ├── symbol_registry.rs # 交易对注册
│       ├── volatility.rs     # 波动率计算
│       ├── websocket.rs      # WebSocket基础
│       └── types.rs          # 数据源类型
│
├── c_data_process/     # [新建] 数据处理层
│   └── src/
│       ├── lib.rs
│       ├── pine_indicator_full.rs  # Pine指标
│       ├── trading_trigger.rs      # 交易触发
│       ├── types.rs                # 指标类型
│       └── volatility_rank.rs       # 波动率排名
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
│       │   ├── account_pool.rs    # 账户池
│       │   ├── margin_config.rs   # 保证金配置
│       │   ├── check_table.rs     # 检查表
│       │   ├── round_guard.rs     # 回合守卫
│       │   ├── market_status.rs   # 市场状态
│       │   └── pnl_manager.rs     # PnL管理
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
│       ├── strategy/         # 策略子模块
│       │   ├── mod.rs
│       │   ├── traits.rs
│       │   ├── pin_strategy.rs
│       │   ├── trend_strategy.rs
│       │   └── types.rs
│       ├── core/            # 核心引擎
│       │   ├── mod.rs
│       │   ├── engine.rs
│       │   ├── pipeline.rs
│       │   ├── pipeline_form.rs
│       │   └── strategy_pool.rs
│       ├── order/           # 订单子模块
│       │   ├── mod.rs
│       │   ├── gateway.rs
│       │   ├── order.rs
│       │   └── mock_binance_gateway.rs
│       ├── channel/         # 通道子模块
│       │   ├── mod.rs
│       │   ├── channel.rs
│       │   └── mode.rs
│       └── shared/          # 共享子模块 (策略相关)
│           ├── mod.rs
│           ├── symbol_rules.rs
│           ├── symbol_rules_fetcher.rs
│           ├── checkpoint.rs
│           ├── checkpoint_integration.rs
│           ├── error.rs
│           ├── platform.rs
│           └── telegram_notifier.rs
│
└── g_test/              # [新建] 测试层
    └── src/
        ├── lib.rs
        └── integration/     # 集成测试
            ├── mod.rs
            └── engine_test.rs
```

## 4. 重组原则

### 4.1 模块依赖方向
```
b_data_source → c_data_process → e_strategy → d_risk_monitor
                        ↑              ↓
                        └──────────────┘
```

### 4.2 非业务层划分
- **a_common/**: 纯粹的工具层，无业务依赖
- **b_data_source/**: 数据获取层，被所有模块依赖
- **c_data_process/**: 指标计算，被策略层依赖

### 4.3 业务层划分
- **d_risk_monitor/**: 负责资金安全、持仓控制
- **e_strategy/**: 负责策略决策、订单执行

## 5. persistence 模块归属说明

persistence (持久化) 同时被 d_risk_monitor 和 e_strategy 使用：
- d_risk_monitor: 持仓快照、账户快照
- e_strategy: 交易记录、K线缓存

**决策**: 放入 d_risk_monitor/，因为 persistence 主要是风控监控的数据持久化

## 6. shared 模块拆分

engine/shared/ 拆分为两部分：
- **d_risk_monitor/shared/**: account_pool, margin_config, check_table, round_guard, market_status, pnl_manager
- **e_strategy/shared/**: symbol_rules, symbol_rules_fetcher, checkpoint, checkpoint_integration, error, platform, telegram_notifier

## 7. 实施步骤

### Phase 1: 创建目录结构 (预计 10 分钟)
1. 创建 a_common/ 目录
2. 创建 b_data_source/ 目录
3. 创建 c_data_process/ 目录
4. 创建 d_risk_monitor/ 目录
5. 创建 e_strategy/ 目录
6. 创建 g_test/ 目录

### Phase 2: 移动代码文件 (预计 30 分钟)
1. 移动 account/ → a_common/
2. 移动 market/ → b_data_source/
3. 移动 indicator/ → c_data_process/
4. 移动 engine/src/risk/, engine/src/position/, engine/src/shared/(风控部分) → d_risk_monitor/
5. 移动 strategy/, engine/src/core/, engine/src/order/, engine/src/channel/, engine/src/shared/(策略部分) → e_strategy/
6. 移动 engine/src/persistence/ → d_risk_monitor/persistence/

### Phase 3: 更新 Cargo.toml (预计 10 分钟)
1. 更新 workspace Cargo.toml
2. 更新各 crate 的 Cargo.toml
3. 更新路径依赖

### Phase 4: 更新 import 路径 (预计 60 分钟)
1. 批量替换所有 `crate::xxx` 路径
2. 验证所有模块间依赖正确

### Phase 5: 编译验证 (预计 20 分钟)
1. cargo check --all
2. 修复路径错误
3. 验证所有测试通过

## 8. 测试目录位置

**方案**: g_test/ 作为独立测试层

优点：
- 测试代码与业务代码分离
- 便于运行集成测试
- 测试配置集中管理

**备选方案**: 将测试分散到各模块的 tests/ 目录
- 优点: 测试与代码更近
- 缺点: 分散管理

**推荐**: g_test/ 独立测试层
