# Engine 目录重构设计方案

## 1. 背景

`crates/engine/src/` 当前有 37 个文件，职责混杂，难以维护。参照其他 crate 的良好结构（account 3 文件、indicator 有子目录、market 12 文件、strategy 6 文件），对 engine 进行子目录拆分。

## 2. 目标

- 将 37 个文件按业务领域拆分为 7 个子模块
- 消除循环依赖依赖
- 保持 `cargo check --all` 编译通过
- 所有现有单元测试继续运行通过

## 3. 重构后结构

```
crates/engine/src/
├── core/                      # 核心引擎
│   ├── mod.rs
│   ├── engine.rs              # TradingEngine 主循环
│   ├── pipeline.rs           # 交易管道
│   └── pipeline_form.rs      # 管道表单
│
├── risk/                      # 风控模块
│   ├── mod.rs
│   ├── risk.rs               # 基础风控
│   ├── risk_rechecker.rs     # 复核风控
│   ├── minute_risk.rs        # 分钟级风控
│   ├── order_check.rs        # 订单风控检查
│   └── thresholds.rs          # 风控阈值
│
├── order/                     # 订单模块
│   ├── mod.rs
│   ├── order.rs              # 订单结构
│   ├── gateway.rs            # 网关抽象
│   └── mock_binance_gateway.rs # Mock 交易所
│
├── position/                  # 持仓模块
│   ├── mod.rs
│   ├── position_manager.rs    # 持仓管理
│   └── position_exclusion.rs  # 持仓排除
│
├── persistence/               # 持久化模块
│   ├── mod.rs
│   ├── sqlite_persistence.rs  # SQLite 存储
│   ├── memory_backup.rs       # 内存备份
│   ├── disaster_recovery.rs   # 灾备恢复
│   └── persistence.rs         # 持久化 trait
│
├── channel/                  # 通道模块
│   ├── mod.rs
│   ├── channel.rs            # 通道判断
│   └── mode.rs               # 交易模式
│
├── shared/                    # 共享模块
│   ├── mod.rs
│   ├── symbol_rules.rs        # 交易对规则
│   ├── symbol_rules_fetcher.rs # 规则拉取
│   ├── account_pool.rs        # 账户池
│   ├── margin_config.rs       # 保证金配置
│   ├── market_status.rs       # 市场状态
│   ├── checkpoint.rs          # 检查点
│   ├── checkpoint_integration.rs
│   ├── pnl_manager.rs         # PnL 管理
│   ├── telegram_notifier.rs   # Telegram 通知
│   ├── platform.rs           # 平台检测
│   ├── round_guard.rs        # 回合守卫
│   └── check_table.rs        # 检查表
│
└── lib.rs                    # 库入口（顶层模块声明）
```

## 4. 模块依赖关系

```
shared (基础类型/配置)
    ↓
risk (风控规则检查)
    ↓
order (订单创建/校验)
    ↓
position (持仓管理)
    ↓
persistence (数据持久化)

channel ← shared (共享交易模式)

core ← risk, order, position, channel (核心引擎编排)
```

## 5. 文件移动映射

| 原路径 | 新路径 |
|--------|--------|
| engine.rs | core/engine.rs |
| pipeline.rs | core/pipeline.rs |
| pipeline_form.rs | core/pipeline_form.rs |
| risk.rs | risk/risk.rs |
| risk_rechecker.rs | risk/risk_rechecker.rs |
| minute_risk.rs | risk/minute_risk.rs |
| order_check.rs | risk/order_check.rs |
| thresholds.rs | risk/thresholds.rs |
| order.rs | order/order.rs |
| gateway.rs | order/gateway.rs |
| mock_binance_gateway.rs | order/mock_binance_gateway.rs |
| position_manager.rs | position/position_manager.rs |
| position_exclusion.rs | position/position_exclusion.rs |
| sqlite_persistence.rs | persistence/sqlite_persistence.rs |
| memory_backup.rs | persistence/memory_backup.rs |
| disaster_recovery.rs | persistence/disaster_recovery.rs |
| persistence.rs | persistence/persistence.rs |
| channel.rs | channel/channel.rs |
| mode.rs | channel/mode.rs |
| symbol_rules.rs | shared/symbol_rules.rs |
| symbol_rules_fetcher.rs | shared/symbol_rules_fetcher.rs |
| account_pool.rs | shared/account_pool.rs |
| margin_config.rs | shared/margin_config.rs |
| market_status.rs | shared/market_status.rs |
| checkpoint.rs | shared/checkpoint.rs |
| checkpoint_integration.rs | shared/checkpoint_integration.rs |
| pnl_manager.rs | shared/pnl_manager.rs |
| telegram_notifier.rs | shared/telegram_notifier.rs |
| platform.rs | shared/platform.rs |
| round_guard.rs | shared/round_guard.rs |
| check_table.rs | shared/check_table.rs |
| error.rs | shared/error.rs |
| lib.rs | lib.rs (更新模块声明) |

## 6. 实施步骤

1. 创建 7 个子目录 (core, risk, order, position, persistence, channel, shared)
2. 移动文件到对应子目录
3. 在每个子目录创建 `mod.rs`，声明该模块的子模块
4. 更新顶层 `lib.rs`，声明 7 个子模块
5. 检查 `cargo check --all` 编译结果
6. 运行 `cargo test -p engine` 确保测试通过
7. 提交 git

## 7. 风险控制

- **原子提交**: 每移动一批文件就 commit 一次
- **编译验证**: 每次移动后运行 `cargo check -p engine`
- **测试验证**: 移动完成后运行完整测试
