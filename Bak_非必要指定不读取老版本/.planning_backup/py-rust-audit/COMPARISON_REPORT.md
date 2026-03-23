# Python → Rust 功能缺失对比报告

## 审计概述

| 项目 | 状态 | 文件数 | 说明 |
|------|------|--------|------|
| a_common | ✅ 已审计 | 81 | 基础组件库 |
| b_data_source | ⚠️ Go项目 | - | 非Python |
| c_data_process | ✅ 已审计 | 11 | 指标计算模块 |
| d_risk_monitor | ✅ 已审计 | 8 | 风控模块 |
| e_strategy | ✅ 已审计 | 7 | 策略交易模块 |

---

## 一、d_risk_monitor 模块对比

### 1.1 RiskEngine 四层架构

| Python层级 | Rust实现 | 状态 | 说明 |
|-----------|----------|------|------|
| AccountPool | AccountPool | ✅ 已实现 | 账户保证金池、熔断保护 |
| StrategyPool | StrategyPool | ✅ 已实现 | 策略保证金池 |
| OrderCheck | OrderCheck | ✅ 已实现 | 订单风控检查 |
| RiskEngine | RiskPreChecker | ✅ 已实现 | 风控预检 |

#### Python 缺失功能 (d_risk_monitor/risk_engine.py)

| 功能 | 说明 | Rust状态 |
|------|------|----------|
| `sync_account_data()` | 同步账户数据到Redis (ba.client_u.account()) | ❌ 未实现 |
| `sync_position_data()` | 同步仓位数据到Redis | ❌ 未实现 |
| `release_preoccupy_margin()` | 释放预占保证金 | ❌ 未实现 |
| `_preoccupy_margin()` Lua脚本 | 原子预占保证金 | ❌ 未实现 |

### 1.2 PnlManager 盈亏管理

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `update_position_pnl()` | ❌ 未实现 | Redis集成缺失 |
| `get_position_pnl()` | ❌ 未实现 | Redis集成缺失 |
| `mark_low_volatility_symbol()` | `add_low_volatility_symbol()` | ✅ 部分 |
| `mark_high_volatility_symbol()` | `add_high_volatility_symbol()` | ✅ 部分 |
| `check_pnl_coverage()` | ❌ 未实现 | 无 |
| `rescue_low_volatility_symbols()` | `rescue_low_volatility_symbols()` | ✅ 已实现 |
| `get_rescued_history()` | ❌ 未实现 | 无 |
| `reset_strategy()` | `reset()` | ✅ 已实现 |
| `add_accumulated_profit()` | `update_cumulative_profit()` | ✅ 已实现 |

#### Python 缺失功能 (d_risk_monitor/pnl_manager.py)

| 功能 | 说明 |
|------|------|
| `mark_low_volatility_symbols()` | 批量标记低波动品种 |
| `mark_high_volatility_symbols()` | 批量标记高波动品种 |
| `clear_high_volatility_symbols()` | 清除高波动标记 |
| `unmark_high_volatility_symbol()` | 取消单个高波动标记 |
| `unmark_low_volatility_symbols()` | 取消低波动标记 |
| `get_low_volatility_symbols()` | 获取低波动品种列表 |
| `calculate_low_volatility_total_pnl()` | 计算低波动品种总盈亏 |
| `calculate_total_unrealized_loss()` | 计算未实现亏损总额 |

### 1.3 SymbolManager 品种管理

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `register_symbol()` | ❌ 未实现 | 需通过SymbolRulesFetcher |
| `unregister_symbol()` | ❌ 未实现 | 无 |
| `get_strategy_symbols()` | ❌ 未实现 | 无 |
| `clear_all_symbols()` | ❌ 未实现 | 无 |
| `_get_process_lock()` | ❌ 未实现 | 无进程锁 |

#### Python 缺失功能 (d_risk_monitor/symbol_manager.py)

- 线程+进程安全的品种注册管理
- 跨平台文件锁 (fcntl/msvcrt)
- 策略池品种配置JSON持久化

### 1.4 SymbolRuleParser 交易对规则

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `get_symbol_rules()` | `SymbolRules` 结构体 | ✅ 已实现 |
| `effective_min_qty` property | `effective_min_qty()` | ✅ 已实现 |
| `round_price()` | ❌ 未实现 | 无 |
| `round_qty()` | ❌ 未实现 | 无 |
| `validate_order()` | ❌ 未实现 | 无 |
| `calculate_open_qty()` | ❌ 未实现 | 无 |

#### Python 缺失功能 (d_risk_monitor/symbol_rule_service.py)

- `LeverageCommissionCache` - 杠杆档位和手续费率缓存 (1小时过期)
- Redis键名常量管理
- API实时拉取交易对规则

### 1.5 LocalPositionManagerDecimal 本地仓位

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `add_position()` | `open_position()` | ✅ 已实现 |
| `remove_position_by_index()` | ❌ 未实现 | 无索引删除 |
| `remove_position_by_item()` | ❌ 未实现 | 无项删除 |
| `get_position_list()` | `get_position()` | ✅ 部分 |
| `calculate_position_summary()` | ❌ 未实现 | 无汇总计算 |
| `_find_index_by_item()` | ❌ 未实现 | 无精准查找 |
| `reset_pst_info()` | `reset()` | ✅ 已实现 |
| JSON持久化 | ❌ 未实现 | 无 |

#### Python 缺失功能 (d_risk_monitor/local_position_manager.py)

- `pst_info` 多维度持仓管理 (this/past/all)
- Decimal精确计算的原地操作
- 按索引/项精准删除小仓位
- 仓位汇总计算 (平均价格、总数量、总价值)
- this与past仓位的互相转换
- JSON文件持久化

---

## 二、e_strategy 模块对比

### 2.1 MarketDetector 市场状态检测

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `detect_pin_status()` | `detect_pin()` | ✅ 已实现 |
| `detect_range_status()` | `is_range_market()` | ✅ 已实现 |
| `detect_market()` | `detect()` | ✅ 已实现 |
| `validate_data()` | ❌ 未实现 | 无数据校验 |
| `_record_history()` | ❌ 未实现 | 无历史记录 |
| `MarketStatus` 枚举 | `MarketStatus` 枚举 | ✅ 已实现 |
| `PinIntensity` 枚举 | `PinIntensity` 枚举 | ✅ 已实现 |

#### Python 缺失功能 (e_strategy/market_status.py)

- 数据有效性校验 (时间戳、超时检查)
- 检测历史记录功能
- Redis/配置集成

### 2.2 PinStatusDetector 插针检测

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `check_long_entry()` | ❌ 未实现 | 无 |
| `check_short_entry()` | ❌ 未实现 | 无 |
| `check_long_exit()` | ❌ 未实现 | 无 |
| `check_short_exit()` | ❌ 未实现 | 无 |
| `check_long_hedge_condition()` | ❌ 未实现 | 无 |
| `check_short_hedge_condition()` | ❌ 未实现 | 无 |
| `check_exit_high_volatility()` | ❌ 未实现 | 无 |
| `load_indicator_data()` | ❌ 未实现 | 无Redis |
| 7个极端条件判断 | ❌ 未实现 | 无 |

#### Python 缺失功能 (e_strategy/pin_status_detector.py)

- 完整的插针状态检测器
- 7个极端条件满足判断 (>=4个触发)
- 多空开仓/平仓/对冲条件
- Redis指标数据加载

### 2.3 TrendStatusDetector 趋势检测

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `check_long_entry()` | ❌ 未实现 | 无 |
| `check_short_entry()` | ❌ 未实现 | 无 |
| `check_long_exit()` | ❌ 未实现 | 无 |
| `check_short_exit()` | ❌ 未实现 | 无 |
| `_validate_pine_color_groups()` | ❌ 未实现 | 无 |
| `_check_all_pine_green_for_long()` | ❌ 未实现 | 无 |
| `_check_all_pine_red_purple_for_short()` | ❌ 未实现 | 无 |

#### Python 缺失功能 (e_strategy/trend_status_detector.py)

- 日线趋势检测器
- Pine颜色分组校验 (12_26/20_50/100_200周期)
- 全组纯绿/纯红/紫色判断
- 多周期指标综合判断

### 2.4 singleAssetTrader 单币种交易器

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `open_position()` | ❌ 未实现 | 无完整交易引擎 |
| `place_order()` | `OrderExecutor` | ✅ 部分 |
| `_get_kline_close()` | ❌ 未实现 | 无 |
| `_calculate_realized_pnl()` | `calculate_realized_pnl()` | ✅ 已实现 |
| `_calculate_unrealized_pnl()` | `calculate_unrealized_pnl()` | ✅ 已实现 |
| `_store_data()` | ❌ 未实现 | 无JSON持久化 |
| `_load_data()` | ❌ 未实现 | 无JSON加载 |
| `_run_loop()` | ❌ 未实现 | 无交易循环 |
| `startloop()` | ❌ 未实现 | 无启动逻辑 |
| `run_once()` | ❌ 未实现 | 无单次执行 |
| `stoploop()` | ❌ 未实现 | 无停止逻辑 |
| `health_check()` | ❌ 未实现 | 无健康检查 |
| Redis分布式锁 | ❌ 未实现 | 无 |

#### Python 缺失功能 (e_strategy/trend_main.py, pin_main.py)

- 完整的交易生命周期管理
- 分布式锁 (fcntl文件锁)
- 插针/趋势行情自动切换
- 多状态机管理 (INITIAL/HEDGE_ENTER/POS_LOCKED等)
- JSON数据持久化
- 健康检查与状态汇报

---

## 三、c_data_process 模块对比

### 3.1 MarketAnalyzer 市场分析器

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `_validate_and_clean_hold_symbol()` | ❌ 未实现 | 无 |
| `_refresh_hold_symbols()` | ❌ 未实现 | 无Redis |
| `update_hold_symbol_timestamp()` | ❌ 未实现 | 无Redis |
| `_load_all_vol_symbols()` | ❌ 未实现 | 无 |
| `_assign_symbols()` | ❌ 未实现 | 无 |
| `process_fast()` | ❌ 未实现 | 无快速通道 |
| `process_slow()` | ❌ 未实现 | 无慢速通道 |
| 双通道模式 | `VolatilityChannel` | ✅ 部分 |

#### Python 缺失功能 (c_data_process/indicator_1d/main.py)

- 已持仓/未持仓品种分离管理
- Redis持仓品种Hash管理
- 波动率排行ZSET
- 快速通道 (10秒间隔)
- 慢速通道 (60秒间隔)

### 3.2 IndicatorCalculator 指标计算

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| EMA增量计算 | ✅ 已实现 | indicator模块 |
| RSI计算 | ✅ 已实现 | indicator模块 |
| Pine颜色 | ✅ 已实现 | indicator模块 |
| 价格位置 | ✅ 已实现 | indicator模块 |
| TR比率排名 | ❌ 未实现 | 无 |
| 速度百分位 | ❌ 未实现 | 无 |
| 加速度百分位 | ❌ 未实现 | 无 |
| Power百分位 | ❌ 未实现 | 无 |
| 高阶动能(Jerk) | ❌ 未实现 | 无 |

#### Python 缺失功能 (c_data_process/indicator_1d/big_cycle_calc.py)

- 全量日线金融指标 (TR、速度、加速度、位置、Jerk等)
- 60日/14日多窗口参数
- TR比率20日百分比排名
- 高阶动能指标 (Jerk急动度)
- Power指标及百分比排位

### 3.3 PineColorOnlyCalculator Pine颜色

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| `calculate_pine_macd()` | ✅ 已实现 | indicator模块 |
| `calculate_pine_rsi()` | ✅ 已实现 | indicator模块 |
| `calculate_trade_conditions()` | ❌ 未实现 | 无 |
| `calculate_bar_color()` | ✅ 已实现 | indicator模块 |
| `calculate_bg_color()` | ✅ 已实现 | indicator模块 |
| `calculate_amplitude()` | ❌ 未实现 | 无 |
| `check_trading_conditions()` | ❌ 未实现 | 无 |

#### Python 缺失功能 (c_data_process/indicator_1d/pine_scripts.py)

- CRSI计算 (PINE_DOMCYCLE/PINE_VIBRATION等)
- 上下轨计算 (vectorized_db_ub)
- 交易条件检查 (振幅80%阈值、效率阈值)
- TOP3平均振幅百分比
- 1%振幅对应时间计算

---

## 四、a_common 模块对比

### 4.1 Redis客户端

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| Redis连接管理 | ❌ 未实现 | 无Redis客户端 |
| Hash操作 | ❌ 未实现 | 无 |
| ZSET操作 | ❌ 未实现 | 无 |
| String操作 | ❌ 未实现 | 无 |

#### Python 缺失功能 (a_common/redis_client/)

- 完整的Redis客户端封装
- 连接池管理
- 自动重连机制
- 序列化/反序列化

### 4.2 WebSocket管理器

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| WebSocket连接 | ❌ 未实现 | 无 |
| 心跳机制 | ❌ 未实现 | 无 |
| 重连机制 | ❌ 未实现 | 无 |
| 消息队列 | ❌ 未实现 | 无 |

#### Python 缺失功能 (a_common/websocket/)

- Binance WebSocket实时行情
- 成交明细推送
- K线数据推送
- 深度数据推送

### 4.3 Binance API客户端

| Python功能 | Rust实现 | 状态 |
|-----------|----------|------|
| UM Futures API | ❌ 未实现 | 无 |
| Spot API | ❌ 未实现 | 无 |
| 账户信息 | `AccountPool` | ✅ 部分 |
| 持仓信息 | `LocalPositionManager` | ✅ 部分 |
|下单 | `OrderExecutor` | ✅ 部分 |
| 杠杆档位 | `SymbolRules` | ✅ 部分 |
| 手续费率 | ❌ 未实现 | 无 |

---

## 五、汇总：缺失功能清单

### 5.1 高优先级 (核心交易功能)

| 序号 | 模块 | 功能 | 说明 |
|------|------|------|------|
| 1 | e_strategy | 完整交易引擎 | open_position/place_order/run_loop |
| 2 | e_strategy | PinStatusDetector | 插针状态检测7条件 |
| 3 | e_strategy | TrendStatusDetector | 日线趋势检测 |
| 4 | d_risk_monitor | Redis集成 | PnlManager/SymbolManager |
| 5 | d_risk_monitor | LocalPosition多维度 | this/past仓位转换 |
| 6 | d_risk_monitor | 进程锁 | 跨平台文件锁 |

### 5.2 中优先级 (指标与风控)

| 序号 | 模块 | 功能 | 说明 |
|------|------|------|------|
| 7 | c_data_process | TR比率排名 | 20日百分比排名 |
| 8 | c_data_process | 速度/加速度百分位 | 多窗口动能指标 |
| 9 | c_data_process | 高阶动能(Jerk) | 急动度指标 |
| 10 | c_data_process | Pine CRSI | 循环指标 |
| 11 | d_risk_monitor | 保证金预占Lua | 原子操作 |
| 12 | d_risk_monitor | 交易对规则API拉取 | 实时规则更新 |

### 5.3 低优先级 (辅助功能)

| 序号 | 模块 | 功能 | 说明 |
|------|------|------|------|
| 13 | e_strategy | 健康检查 | health_check |
| 14 | e_strategy | JSON持久化 | _store_data/_load_data |
| 15 | c_data_process | MarketAnalyzer双通道 | 快速/慢速处理 |
| 16 | c_data_process | Redis持仓管理 | Hash/ZSET操作 |
| 17 | a_common | Redis客户端 | 完整封装 |
| 18 | a_common | WebSocket | Binance实时行情 |

---

## 六、Rust已实现但Python无对应

| 功能 | Rust模块 | 说明 |
|------|----------|------|
| SignalSynthesisLayer | order/mock_binance_gateway | 信号综合层 |
| VolatilityChannel | channel/channel | 波动率通道 |
| ModeSwitcher | channel/mode | 模式切换器 |
| RoundGuard | shared/round_guard | 回合守卫 |
| CheckpointLogger | shared/checkpoint | 检查点日志 |
| CompositeCheckpointLogger | shared/checkpoint | 组合检查点 |
| MemoryBackup | persistence/memory_backup | 内存备份 |
| SqliteEventRecorder | persistence/sqlite_persistence | SQLite记录 |
| DisasterRecovery | persistence/disaster_recovery | 灾备恢复 |
| SymbolRulesFetcher | shared/symbol_rules_fetcher | 规则拉取 |
| TelegramNotifier | shared/telegram_notifier | Telegram通知 |

---

## 七、建议实施顺序

### Phase 1: 核心交易引擎
1. 实现完整的 `TradingEngine.run_iteration()`
2. 实现 `PinStatusDetector` 插针检测
3. 实现 `TrendStatusDetector` 趋势检测
4. 实现 `SignalSynthesisLayer` 通道切换

### Phase 2: 风控与仓位
5. 实现 `LocalPositionManager` 多维度持仓 (this/past)
6. 实现 `PnlManager` Redis集成
7. 实现 `SymbolManager` 品种管理

### Phase 3: 指标计算增强
8. 实现 TR比率排名
9. 实现速度/加速度百分位
10. 实现 Pine CRSI

### Phase 4: 外部接口
11. 实现 Redis 客户端
12. 实现 WebSocket Binance行情
13. 实现交易对规则API拉取

---

**审计时间**: 2026-03-21
**审计依据**: Python源码 + Rust实现对比
**状态**: 待补充
