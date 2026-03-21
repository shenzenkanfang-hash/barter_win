# 目录重组迁移计划

## 1. 重新分析：engine/ 文件性质

### 1.1 真正的业务代码 (e_strategy/)
| 文件/目录 | 说明 | 业务性质 |
|-----------|------|----------|
| core/engine.rs | 交易引擎主循环 | ✅ 核心业务 |
| core/pipeline.rs | 交易管道 | ✅ 核心业务 |
| core/pipeline_form.rs | 管道表单 | ✅ 核心业务 |
| core/strategy_pool.rs | 策略池 | ✅ 核心业务 |
| order/order.rs | 订单执行器 | ✅ 核心业务 |
| order/gateway.rs | 网关抽象 | ✅ 核心业务 |
| channel/channel.rs | 波动率通道切换 | ✅ 核心业务 |
| channel/mode.rs | 交易模式切换 | ✅ 核心业务 |
| risk/ (5文件) | 风控预检/复核/阈值 | ✅ 风控业务 |
| position/ (2文件) | 持仓管理 | ✅ 持仓业务 |
| symbol_rules.rs | 交易规则 | ✅ 规则业务 |
| symbol_rules_fetcher.rs | 规则API拉取 | ✅ 规则业务 |

### 1.2 非业务代码 (→ a_common/ 或 d_risk_monitor/)
| 文件/目录 | 说明 | 建议归属 |
|-----------|------|----------|
| order/mock_binance_gateway.rs | Mock实现，非生产 | a_common/ |
| persistence/sqlite_persistence.rs | 数据库持久化 | d_risk_monitor/ |
| persistence/memory_backup.rs | 内存备份 | d_risk_monitor/ |
| persistence/disaster_recovery.rs | 灾备恢复 | d_risk_monitor/ |
| account_pool.rs | 账户池 | a_common/ |
| margin_config.rs | 保证金配置 | d_risk_monitor/ |
| check_table.rs | 检查表 | a_common/ |
| checkpoint.rs | 检查点 | a_common/ |
| checkpoint_integration.rs | 检查点集成 | a_common/ |
| round_guard.rs | 回合守卫 | d_risk_monitor/ |
| market_status.rs | 市场状态 | d_risk_monitor/ |
| pnl_manager.rs | 盈亏管理 | d_risk_monitor/ |
| telegram_notifier.rs | Telegram通知 | a_common/ |
| platform.rs | 平台检测 | a_common/ |
| error.rs | 错误类型 | a_common/ |

## 2. 目标结构

| 新目录 | 功能划分 | 对应原模块 |
|--------|----------|------------|
| a_common/ | 通用基础层：error、platform、account_pool、check_table、checkpoint、telegram_notifier、mock_binance_gateway | engine/shared/基础设施 + mock |
| b_data_source/ | 数据源层：WebSocket、K线、市场数据、订单簿 | market/ |
| c_data_process/ | 数据处理层：指标计算、Pine颜色、波动率排名 | indicator/ |
| d_risk_monitor/ | 风控监控层：risk/、position/、margin_config、round_guard、market_status、pnl_manager、persistence/ | engine/业务支撑部分 |
| e_strategy/ | 策略执行层：core/、order/(除mock外)、channel/、symbol_rules/ | engine/业务核心 |
| g_test/ | 测试层：集成测试 | 测试代码 |

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

## 5. 重组原则

### 5.1 模块依赖方向
```
b_data_source → c_data_process → e_strategy → d_risk_monitor
                        ↑              ↓
                        └──────────────┘
```

### 5.2 业务 vs 非业务划分
- **业务代码 (e_strategy/)**: 交易策略、引擎核心、订单执行、通道切换
- **非业务代码**: 数据库、API调用、通知、检查点等基础设施

### 5.3 persistence 模块归属
persistence (持久化) 放入 d_risk_monitor/，因为主要用于风控监控的数据持久化

## 6. 实施步骤

### Phase 1: 创建目录结构
1. 创建 a_common/, b_data_source/, c_data_process/, d_risk_monitor/, e_strategy/, g_test/

### Phase 2: 移动代码文件
1. account/ + engine/shared/基础设施 → a_common/
2. market/ → b_data_source/
3. indicator/ → c_data_process/
4. engine/risk/, engine/position/, engine/shared/风控相关, engine/persistence/ → d_risk_monitor/
5. strategy/, engine/core/, engine/order/(除mock), engine/channel/, engine/shared/symbol_* → e_strategy/

### Phase 3: 更新 Cargo.toml 和 import 路径

### Phase 4: 编译验证

## 7. 测试目录位置

**推荐**: g_test/ 独立测试层
- 测试代码与业务代码分离
- 便于运行集成测试
