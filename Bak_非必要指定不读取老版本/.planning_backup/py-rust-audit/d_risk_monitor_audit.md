# d_risk_monitor/ Python 代码审计报告

## 文件清单
- [x] __init__.py
- [x] config.py
- [x] utils.py
- [x] pnl_manager.py
- [x] symbol_manager.py
- [x] symbol_rule_service.py
- [x] risk_engine.py
- [x] local_position_manager.py

## 详细审计

---

### 文件: __init__.py

```python
# 风险监控模块包初始化文件
# 导出核心组件供外部使用
```

#### 导入模块
- `from .risk_engine import RiskEngine` - 导入风险引擎
- `from .pnl_manager import PnlManager` - 导入盈亏管理器

---

### 文件: config.py

```python
# 风险监控模块全局配置
# 包含 Redis 配置、策略级别定义、保证金池配置等
```

#### 全局配置常量

| 配置项 | 说明 |
|--------|------|
| `REDIS_CONFIG` | Redis连接配置字典 (host, port, db, decode_responses) |
| `REDIS_KEYS` | Redis键名常量 (盈亏相关键名) |
| `STRATEGY_LEVELS` | 策略级别列表 ["MINUTE", "HOUR"] |
| `STRATEGY_CONFIG_FILE` | 策略品种映射配置文件路径 |
| `FALLBACK_TOTAL_MARGIN` | 账户保证金兜底值 1000.0 |
| `MIN_EFFECTIVE_MARGIN` | 最小有效保证金 0.01 |
| `MINUTE_OPEN_CONFIG` | 分钟级动态开仓配置 |
| `MARGIN_POOL_CONFIG` | 保证金池配置 (全局+策略级别) |

#### 数据结构

| 配置结构 | 说明 |
|----------|------|
| `MARGIN_POOL_CONFIG["GLOBAL"]` | 全局配置: max_usage_ratio=0.8, reserve_ratio=0.2 |
| `MARGIN_POOL_CONFIG["STRATEGY"]["MINUTE"]` | 分钟级: allocation_ratio=0.4, new_open_ratio=0.15, double_open_ratio=0.5 |
| `MARGIN_POOL_CONFIG["STRATEGY"]["HOUR"]` | 小时级: allocation_ratio=0.4, new_open_ratio=0.3, double_open_ratio=0.5 |

---

### 文件: utils.py

```python
# 线程安全工具类和数据类定义
# 提供线程安全字典和盈亏相关数据模型
```

#### class ThreadSafeDict
线程安全的字典封装类

- `__init__()` - 初始化，创建内部字典和线程锁
- `get(key, default=None)` - 获取值（线程安全）
- `set(key, value)` - 设置值（线程安全）
- `update(data: Dict)` - 批量更新（线程安全）
- `clear()` - 清空字典（线程安全）
- `data` (property) - 获取字典副本

#### 数据类 (dataclass)

| 类名 | 用途 |
|------|------|
| `AccountMargin` | 账户保证金数据 (total_margin_balance, unrealized_pnl, effective_margin 等) |
| `StrategyMargin` | 策略保证金数据 (strategy_level, allocation_margin, used_margin, available_margin 等) |
| `OpenNotionalResult` | 开仓名义价值计算结果 |
| `PnlCoverageResult` | 盈亏覆盖检查结果 (can_cover, total_unrealized_loss, current_symbol_profit 等) |
| `StrategyPositionPnl` | 策略品种盈亏数据 (symbol, long_avg_price, long_qty, short_avg_price, short_qty, unrealized_pnl 等) |

---

### 文件: symbol_manager.py

```python
# 线程+进程安全的品种管理器
# 管理策略池中的交易品种注册
```

#### class SymbolManager
管理分钟级和小时级策略池中的交易品种

- `__init__(logger=None)` - 初始化 (加载Redis配置、策略配置)
- `_get_process_lock()` - 获取进程锁（跨平台：fcntl/msvcrt）
- `_release_process_lock(lock_file)` - 释放进程锁
- `_load_strategy_config()` - 从JSON文件加载策略品种配置
- `_save_strategy_config()` - 保存策略品种配置到JSON文件
- `register_symbol(symbol: str, strategy_level: str)` - 注册品种到策略池（线程+进程安全）
- `unregister_symbol(symbol: str)` - 从所有策略池删除品种
- `get_strategy_symbols(strategy_level: str)` - 获取指定策略池的品种列表
- `clear_all_symbols(strategy_level: str = None)` - 清空策略池品种

---

### 文件: symbol_rule_service.py

```python
# 交易对规则解析服务
# 从Redis读取交易对规则、杠杆档位、手续费率并解析
```

#### class RedisKeyConstants
Redis键名常量类

| 常量 | 键名 |
|------|------|
| `SYMBOL_RULES` | "symbol_rules" - 主交易对规则 |
| `LEVERAGE_BRACKETS` | "leverage_brackets" - 杠杆档位缓存 |
| `COMMISSION_INFO` | "commission_info" - 手续费率缓存 |

#### class LeverageCommissionCache
杠杆档位和手续费率缓存管理器

- `__init__()` - 初始化Redis客户端和API客户端
- `_is_cache_expired(cache_ts: int)` - 判断缓存是否过期（超过1小时）
- `get_leverage_brackets(symbol: str)` - 获取交易对杠杆档位（优先缓存，1小时刷新）
- `get_commission_info(symbol: str)` - 获取交易对手续费率（优先缓存，1小时刷新）

#### class SymbolRuleParser
交易对规则解析器

- `__init__()` - 初始化解析器
- `get_symbol_rules(symbol: str)` - 获取指定交易对的完整规则（包含effective_min_qty）
- `_parse_raw_rule_data(symbol, raw_data, leverage_brackets, commission_info)` - 解析原始规则数据为标准字典

#### @dataclass SymbolRules (frozen=True)
交易对规则数据模型（不可变）

| 属性 | 说明 |
|------|------|
| `symbol` | 交易对名称 |
| `price_precision` | 价格精度 |
| `quantity_precision` | 数量精度 |
| `tick_size` | 价格最小变动 |
| `min_qty` | 交易所原始最小数量 |
| `min_notional` | 最小名义价值 |
| `max_notional` | 最大名义价值 |
| `leverage` | 杠杆倍数 |
| `maker_fee` | 挂单手续费率 |
| `taker_fee` | 吃单手续费率 |
| `close_min_ratio` | 平仓最小盈亏比阈值 |
| `min_value_threshold` | 下单最小名义价值阈值 |
| `update_ts` | 规则最后更新时间戳 |

| 方法 | 说明 |
|------|------|
| `effective_min_qty` (property) | 实际有效最小开仓数量（自动计算） |
| `round_price(price)` | 根据规则对订单价格取整 |
| `round_qty(qty)` | 根据规则对订单数量取整（使用effective_min_qty） |
| `validate_order(price, qty)` | 验证订单是否符合最小名义价值要求 |
| `step_size` (property) | 从数量精度自动换算数量最小步进 |
| `to_dict()` | 转换为字典（包含effective_min_qty） |
| `calculate_open_qty(open_notional, open_price)` | 基于名义价值计算合规开仓数量（Decimal高精度） |

---

### 文件: pnl_manager.py

```python
# 盈亏管理模块
# 独立管理盈亏相关逻辑：低/高波动品种标记、盈亏覆盖检查、解救操作
```

#### class RescueResult (TypedDict)
解救结果类型定义

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | bool | 是否成功 |
| `strategy_level` | str | 策略级别 |
| `can_rescue` | bool | 能否解救 |
| `total_loss` | float | 总亏损 |
| `total_available_profit` | float | 可用盈利 |
| `rescued_symbols` | List[str] | 被解救的品种 |
| `remaining_profit` | float | 剩余盈利 |
| `error_msg` | str | 错误信息 |

#### class ResetResult (TypedDict)
重置结果类型定义

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | bool | 是否成功 |
| `strategy_level` | str | 策略级别 |
| `cleared_pnl_data` | int | 清除的盈亏数据数 |
| `cleared_low_volatility_symbols` | int | 清除的低波动品种数 |
| `cleared_rescued_history` | int | 清除的解救历史数 |
| `reset_accumulated_profit` | float | 重置的累计盈利 |
| `error_msg` | str | 错误信息 |

#### class PnlManager
盈亏管理核心类

| 方法 | 说明 |
|------|------|
| `__init__(symbol_manager, logger)` | 初始化盈亏管理器 |
| `update_position_pnl(symbol, strategy_level, position_data)` | 更新品种盈亏数据到Redis |
| `get_position_pnl(symbol, strategy_level)` | 获取品种盈亏数据 |
| `mark_low_volatility_symbol(strategy_level, symbol)` | 标记单个品种为低波动（互斥删除高波动标记） |
| `mark_high_volatility_symbol(strategy_level, symbol)` | 标记单个品种为高波动（互斥删除低波动标记） |
| `mark_low_volatility_symbols(strategy_level, symbols, incremental)` | 批量标记低波动品种（兼容旧接口） |
| `get_high_volatility_symbols(strategy_level)` | 获取高波动品种列表 |
| `is_high_volatility(symbol, strategy_level)` | 判断品种是否为高波动 |
| `clear_high_volatility_symbols(strategy_level)` | 清除高波动品种标记 |
| `unmark_high_volatility_symbol(strategy_level, symbol)` | 取消单个高波动标记 |
| `unmark_low_volatility_symbols(strategy_level, symbols)` | 取消标记低波动品种 |
| `get_low_volatility_symbols(strategy_level)` | 获取低波动品种列表 |
| `is_low_volatility(symbol, strategy_level)` | 判断品种是否为低波动 |
| `clear_low_volatility_symbols(strategy_level)` | 清除低波动品种标记 |
| `calculate_low_volatility_total_pnl(strategy_level)` | 计算低波动品种总体盈亏 |
| `calculate_total_unrealized_loss(strategy_level)` | calculate_low_volatility_total_pnl的别名 |
| `check_pnl_coverage(strategy_level, symbol, current_profit, is_realized)` | 检查盈亏覆盖情况 |
| `_calculate_low_volatility_total_pnl_internal(strategy_level)` | 内部计算低波动品种总体盈亏（无锁版本） |
| `rescue_low_volatility_symbols(strategy_level, high_vol_profit, is_realized, max_cover_ratio)` | 解救低波动品种 |
| `add_accumulated_profit(strategy_level, profit_amount)` | 添加累计盈利 |
| `get_rescued_history(strategy_level, limit)` | 获取已解救品种的历史记录 |
| `reset_strategy(strategy_level)` | 重置策略盈亏数据（同时清除高低波动标记） |
| `get_all_pnl_data(strategy_level)` | 获取所有品种的盈亏数据 |

---

### 文件: risk_engine.py

```python
# 风控引擎核心模块
# 四层架构：AccountPool -> StrategyPool -> OrderCheck -> RiskEngine
```

#### 工具函数

| 函数 | 说明 |
|------|------|
| `round_decimal(value, precision=4)` | 四舍五入保留指定小数位（Decimal精度） |

#### class AccountPool
账户保证金池组件

| 方法 | 说明 |
|------|------|
| `__init__(logger)` | 初始化账户池 |
| `update_account_data_from_redis()` | 从Redis更新账户本地缓存（带熔断保护） |
| `get_account_margin()` | 获取账户保证金（带缓存过期检查、熔断判断） |

#### class StrategyPool
策略保证金池组件

| 方法 | 说明 |
|------|------|
| `__init__(account_pool, logger)` | 初始化策略池 |
| `register_symbol(symbol, strategy_level)` | 注册品种到策略池 |
| `unregister_symbol(symbol)` | 从策略池删除品种 |
| `get_strategy_symbols(strategy_level)` | 获取策略池品种列表 |
| `_get_position_data()` | 统一读取仓位数据（1秒缓存） |
| `update_strategy_used_margin(strategy_level)` | 更新策略已用保证金 |
| `calculate_strategy_margin(strategy_level)` | 计算策略保证金分配 |
| `calculate_minute_open_notional(leverage)` | 计算分钟级开仓名义价值 |
| `calculate_hour_open_notional(leverage)` | 计算小时级开仓名义价值 |

#### class OrderCheck
订单风控检查器

| 方法 | 说明 |
|------|------|
| `__init__(strategy_pool, logger)` | 初始化订单检查器 |
| `_calculate_actual_margin(notional, leverage)` | 计算实际保证金 |
| `_preoccupy_margin(strategy_level, required_margin, global_used, global_ceiling, strategy_allocation)` | 原子预占保证金（Lua脚本） |
| `check_minute_order(symbol, notional, leverage, open_type)` | 检查分钟级订单 |
| `check_hour_order(symbol, notional, leverage)` | 检查小时级订单 |
| `_check_strategy_order(strategy_level, symbol, notional, leverage, open_type)` | 通用策略订单检查 |

#### class RiskEngine
顶级风控引擎（整合所有组件）

| 方法 | 说明 |
|------|------|
| `__init__(logger)` | 初始化风控引擎（校验配置） |
| `_validate_config()` | 校验配置合法性 |
| `register_symbol(symbol, strategy_level)` | 注册品种 |
| `unregister_symbol(symbol)` | 删除品种 |
| `update_all_risk_data()` | 更新所有风控数据（同步账户+仓位+缓存） |
| `sync_account_data()` | 同步账户数据到Redis（使用ba.client_u.account()） |
| `sync_position_data()` | 同步仓位数据到Redis（使用ba.client_u.position_information()） |
| `release_preoccupy_margin(strategy_level, amount)` | 释放预占的保证金 |
| `get_account_margin()` | 获取账户保证金 |
| `get_strategy_margin(strategy_level)` | 获取策略保证金 |
| `get_minute_open_notional(leverage)` | 获取分钟级开仓名义价值 |
| `get_hour_open_notional(leverage)` | 获取小时级开仓名义价值 |
| `check_minute_order(symbol, notional, leverage, open_type)` | 检查分钟级订单 |
| `check_hour_order(symbol, notional, leverage)` | 检查小时级订单 |

---

### 文件: local_position_manager.py

```python
# Decimal精确版本地仓位管理类
# 支持多维度仓位汇总、this/past仓位转换、JSON持久化
```

#### class LocalPositionManagerDecimal
Decimal精确版本地仓位管理器

| 属性 | 说明 |
|------|------|
| `_VALID_POSITION_KEYS` | 有效仓位键列表 ["long_pst_past", "short_pst_past", "long_pst_this", "short_pst_this"] |
| `_VALID_SIDES` | 有效方向 ["long", "short"] |
| `_VALID_DIMENSIONS` | 有效维度 ["all", "this", "past"] |

| 方法 | 说明 |
|------|------|
| `__init__(symbol, price_precision, quantity_precision, min_qty)` | 初始化仓位管理器 |
| `_ensure_json_dir_exists()` | 确保JSON存储目录存在 |
| `_get_json_file_path()` | 获取JSON文件完整路径 |
| `_decimal_to_str(data)` | 递归将Decimal转换为字符串（JSON序列化） |
| `_str_to_decimal(data)` | 递归将字符串转换回Decimal |
| `_load_from_json()` | 从JSON文件加载仓位数据 |
| `_save_to_json(data)` | 将仓位数据写入JSON文件 |
| `_to_decimal(value)` | 安全转换任意数值为Decimal |
| `_validate_position_item(position_item)` | 校验小仓位 [Decimal(价格), Decimal(数量)] 合法性 |
| `_format_position_item(price, qty)` | 格式化小仓位为指定精度 |
| `_find_index_by_item(position_key, target_item)` | 通过小仓位项精准查找索引（原子操作） |
| `reset_pst_info()` | 重置pst_info为初始空状态 |
| `reset_side_position(side)` | 清空重置指定方向（long/short）的所有仓位数据 |
| `get_pst_info(deep_copy, return_float)` | 查询完整pst_info |
| `check_pst_info_stability()` | 巡检pst_info结构和Decimal数据合法性 |
| `add_position(position_key, price, qty)` | 新增Decimal类型小仓位（原子操作+JSON持久化） |
| `remove_position_by_index(position_key, index, return_float)` | 按索引精准删除小仓位（原子操作） |
| `remove_position_by_item(position_key, target_item, return_float)` | 按小仓位项精准删除（Decimal精确匹配） |
| `get_position_list(position_key, deep_copy, return_float)` | 查询指定仓位列表 |
| `calculate_position_summary(side, dimension, return_float)` | 精确计算指定方向+维度的仓位汇总 (平均价格, 总数量, 总价值) |
| `convert_this_and_past(source_key, target_item, return_str)` | 实现this与past仓位的互相转换 |

---

## 审计总结

### 架构概览

| 层级 | 组件 | 职责 |
|------|------|------|
| 数据层 | config.py, utils.py | 全局配置、线程安全工具、数据模型 |
| 品种管理层 | symbol_manager.py | 策略池品种注册（线程+进程安全） |
| 规则解析层 | symbol_rule_service.py | 交易对规则解析（Redis+API） |
| 盈亏管理层 | pnl_manager.py | 盈亏数据、低高波动标记、解救逻辑 |
| 风控核心层 | risk_engine.py | 四层架构：AccountPool -> StrategyPool -> OrderCheck -> RiskEngine |
| 本地仓位层 | local_position_manager.py | Decimal精确本地仓位管理 |

### 核心设计模式

1. **线程+进程安全**: SymbolManager使用threading.Lock + 文件锁(fcntl/msvcrt)
2. **熔断保护**: AccountPool的redis_failure_count计数，连续失败3次触发熔断
3. **Decimal高精度**: LocalPositionManagerDecimal全程使用Decimal避免浮点误差
4. **Redis缓存**: 规则数据1小时过期、仓位数据30秒过期
5. **Lua原子操作**: 保证金预占使用Lua脚本保证并发安全

### 依赖关系

```
risk_engine.py (RiskEngine)
├── AccountPool
├── StrategyPool
│   └── SymbolManager
├── OrderCheck
│   └── StrategyPool
└── 依赖外部: redis_tool, ba (交易所API)

pnl_manager.py (PnlManager)
└── SymbolManager (可选)

symbol_rule_service.py (SymbolRuleParser)
└── LeverageCommissionCache
    └── redis_tool, ba (交易所API)

local_position_manager.py (LocalPositionManagerDecimal)
└── JSON文件系统持久化
```
