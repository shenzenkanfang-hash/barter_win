# e_strategy/ Python 代码审计报告

## 文件清单
- [x] 日线指标权重分析.py
- [x] market_status.py
- [x] range_status.py
- [x] pin_status_detector.py
- [x] trend_status_detector.py
- [x] trend_main.py
- [x] pin_main.py

## 详细审计

---

### 文件: 日线指标权重分析.py

```python
# 多指标综合打分框架，用于判断做多/做空信号强度
# 基于方向(ma排列/pine颜色)、空间位置、波动率、动能等维度进行加权评分
```

#### 函数

- `多指标物理逻辑打分(ctx1d: dict, ctx1m: dict) -> dict`
  - 多指标综合打分函数
  - 参数: ctx1d(日线指标上下文), ctx1m(分钟线指标上下文)
  - 功能: 基于正向条件加分、厌恶条件扣分，计算最终得分并给出交易建议
  - 返回: 包含正向加分、厌恶扣分、净得分、空间加权、波动率加权、最终得分、交易建议的字典

---

### 文件: market_status.py

```python
# 市场状态检测器模块
# 用于检测市场处于 PIN(插针)/RANGE(震荡)/TREND(趋势)/INVALID(无效) 哪种状态
```

#### 枚举类

**MarketStatus**
- 市场状态枚举: PIN(插针), RANGE(震荡), TREND(趋势), INVALID(无效)

**PinIntensity**
- 插针强度枚举: WEAK(弱), MODERATE(中等), STRONG(强)

#### class MarketDetector
- `__init__(data_timeout: int = 180)` - 初始化检测器，设置数据超时阈值
- `validate_data(ctx1m: Dict[str, Any]) -> bool` - 校验1分钟指标数据有效性(检查时间戳和超时)
- `detect_pin_status(ctx1m: Dict[str, Any]) -> tuple[bool, PinIntensity]` - 检测插针状态，基于tr_base_60min判断
- `detect_range_status(ctx1d: Dict[str, Any]) -> bool` - 检测震荡状态，基于低波动和动能指标
- `detect_market(ctx1m: Dict[str, Any], ctx1d: Dict[str, Any]) -> MarketStatus` - 主检测函数，整合所有判断逻辑
- `_record_history(timestamp: float, status: MarketStatus, reason: str)` - 记录检测历史

---

### 文件: range_status.py

```python
# 震荡状态检测模块（占位文件，内容为空或极简）
```

---

### 文件: pin_status_detector.py

```python
# 插针状态检测器模块
# 纯指标判断，无任何仓位逻辑，用于检测插针行情并给出开仓/对冲信号
```

#### 枚举类

**MarketStatus**
- 市场状态枚举: PIN, RANGE, TREND, INVALID

#### class PinStatusDetector

- `__init__(symbol: str, period: str = "1m", data_timeout: int = 180)` - 初始化检测器
- `_get_last_value(df: pd.DataFrame, col_name: str, default: Any = 0) -> Any` - 获取DataFrame最后一行的指定列值
- `load_indicator_data() -> pd.DataFrame` - 从Redis加载指标数据
- `validate_data(df: pd.DataFrame) -> bool` - 校验指标DataFrame时间戳有效性
- `_get_tr_base_60min() -> Optional[float]` - 获取60分钟TR基准值
- `check_long_entry() -> bool` - 纯指标判断做多开仓条件(7个极端条件满足>=4个)
- `check_short_entry() -> bool` - 纯指标判断做空开仓条件(7个极端条件满足>=4个)
- `check_long_exit() -> bool` - 纯指标判断做多平仓条件
- `check_short_exit() -> bool` - 纯指标判断做空平仓条件
- `check_long_hedge_condition() -> bool` - 纯指标判断多单回落对冲条件(TR<0.15时)
- `check_short_hedge_condition() -> bool` - 纯指标判断空单回升对冲条件(TR<0.15时)
- `check_exit_high_volatility() -> bool` - 纯指标判断退出高波动条件(TR比率<1)
- `_record_history(action: str, conditions: Dict[str, Any], satisfied: int)` - 记录检测历史

---

### 文件: trend_status_detector.py

```python
# 趋势状态检测器模块
# 纯指标判断，无任何仓位逻辑，用于检测日线趋势并给出开仓/平仓/对冲信号
```

#### 枚举类

**MarketStatus**
- 市场状态枚举: PIN, RANGE, TREND, INVALID

#### class TrendStatusDetector

- `__init__(symbol: str, period: str = "1d", data_timeout: int = 180)` - 初始化检测器
- `_get_last_value(df: pd.DataFrame, col_name: str, default: Any = 0) -> Any` - 获取DataFrame最后一行的指定列值
- `load_indicator_data() -> pd.DataFrame` - 从Redis String类型Key加载指标数据
- `validate_data(df: pd.DataFrame) -> bool` - 校验指标DataFrame时间戳有效性
- `_validate_pine_color_groups(df: pd.DataFrame) -> Tuple[bool, Dict[str, Dict[str, str]], Optional[str]]` - 按组校验pine颜色指标(12_26/20_50/100_200周期)
- `_check_all_pine_green_for_long(df: pd.DataFrame) -> Tuple[bool, Optional[str]]` - 检查pine颜色是否满足做多条件(全组纯绿)
- `_check_all_pine_red_purple_for_short(df: pd.DataFrame) -> Tuple[bool, Optional[str]]` - 检查pine颜色是否满足做空条件(全组紫色/纯红)
- `check_long_entry() -> bool` - 纯指标判断做多开仓条件(颜色纯绿+极端波动+位置条件)
- `check_short_entry() -> bool` - 纯指标判断做空开仓条件(颜色紫色/纯红+极端波动+位置条件)
- `check_long_exit() -> bool` - 纯指标判断做多平仓条件(使用最大有效周期)
- `check_short_exit() -> bool` - 纯指标判断做空平仓条件(使用最大有效周期)
- `check_long_hedge_condition() -> bool` - 纯指标判断多单回落对冲条件
- `check_short_hedge_condition() -> bool` - 纯指标判断空单回升对冲条件
- `_record_history(action: str, conditions: Dict[str, Any], satisfied: int)` - 记录检测历史

---

### 文件: trend_main.py

```python
# 趋势交易策略主模块
# 基于日线趋势检测器进行交易决策，管理单币种交易的生命周期
```

#### 枚举类

**MarketStatus**
- 市场状态枚举: PIN, RANGE, TREND, INVALID

**Direction**
- 交易方向枚举: LONG(做多), SHORT(做空)

**PinStatus**
- 多空插针状态枚举: INITIAL, HEDGE_ENTER, POS_LOCKED, Long_INITIAL, Long_FIRST_OPEN, Long_DOUBLE_ADD, Long_DAY_ALLOW, Short_INITIAL, Short_FIRST_OPEN, Short_DOUBLE_ADD, Short_DAY_ALLOW

#### 函数

- `convert_any_enum_to_str(obj)` - 枚举转字符串，用于JSON存储
- `convert_str_to_enum_with_mapping(obj, field_enum_map)` - 字符串转枚举，用于JSON加载

#### class singleAssetTrader

- `__init__(symbol: str, mode: str)` - 初始化交易对象，初始化仓位管理器、风控引擎、市场检测器等
- `_get_kline_close() -> float` - 获取K线收盘价
- `_safe_redis_hget(hash_key: str, field: str, default: any = None) -> any` - 安全获取Redis哈希值
- `_calculate_realized_pnl(side, close_price, qty, open_price)` - 计算已实现盈亏
- `_calculate_unrealized_pnl(side, open_price, qty, current_price)` - 计算未实现盈亏
- `place_order(order_type, side, position_side)` - 下单封装方法(0=初始开仓, 1=对冲, 2=翻倍加仓, 3/5=平仓, 4=日线对冲)
- `open_position()` - 核心开仓/平仓逻辑，整合盈亏计算、插针/趋势行情处理
- `_store_data()` - 存储交易数据到JSON
- `_load_data()` - 从JSON加载交易数据
- `_run_loop()` - 运行循环，获取分布式锁后执行open_position
- `startloop()` - 启动交易线程
- `run_once()` - 单次运行
- `stoploop()` - 停止交易线程，清理Redis标记
- `health_check()` - 健康检查，返回运行状态和盈亏信息

#### 外部调用函数

- `start(symbol, mode)` - 创建交易实例并启动线程
- `run(symbol)` - 启动守护线程执行start

---

### 文件: pin_main.py

```python
# 插针交易策略主模块
# 基于插针状态检测器进行交易决策，支持多单/空单的插针行情交易
```

#### 枚举类

**MarketStatus**
- 市场状态枚举: PIN, RANGE, TREND, INVALID

**Direction**
- 交易方向枚举: LONG, SHORT

**PinStatus**
- 多空插针状态枚举: INITIAL, HEDGE_ENTER, POS_LOCKED, Long_INITIAL, Long_FIRST_OPEN, Long_DOUBLE_ADD, Long_DAY_ALLOW, Short_INITIAL, Short_FIRST_OPEN, Short_DOUBLE_ADD, Short_DAY_ALLOW

#### 函数

- `convert_any_enum_to_str(obj)` - 枚举转字符串，用于JSON存储
- `convert_str_to_enum_with_mapping(obj, field_enum_map)` - 字符串转枚举，用于JSON加载

#### class singleAssetTrader

- `__init__(symbol: str, mode: str)` - 初始化交易对象，包含pnl_manager和symbol_manager
- `_get_kline_close() -> float` - 获取K线收盘价
- `_safe_redis_hget(hash_key: str, field: str, default: any = None) -> any` - 安全获取Redis哈希值
- `_calculate_realized_pnl(side, close_price, qty, open_price)` - 计算已实现盈亏
- `_calculate_unrealized_pnl(side, open_price, qty, current_price)` - 计算未实现盈亏
- `place_order(order_type, side, position_side)` - 下单封装方法
- `open_position()` - 核心开仓/平仓逻辑，包含盈利1%平仓、最低平仓线平仓、插针行情开仓、趋势行情处理
- `_all_close()` - 全平仓位(空方法)
- `_store_data()` - 存储交易数据到JSON
- `_load_data()` - 从JSON加载交易数据
- `_run_loop()` - 运行循环
- `startloop()` - 启动交易线程
- `run_once()` - 单次运行
- `stoploop()` - 停止交易线程
- `health_check()` - 健康检查

#### 外部调用函数

- `start(symbol, mode)` - 创建交易实例并启动线程
- `run(symbol)` - 启动守护线程执行start

---

## 审计总结

### 核心类分析

| 类名 | 文件 | 主要职责 | 关键方法 |
|------|------|---------|---------|
| MarketDetector | market_status.py | 市场状态检测(PIN/RANGE/TREND) | detect_market, detect_pin_status, detect_range_status |
| PinStatusDetector | pin_status_detector.py | 插针行情检测 | check_long_entry, check_short_entry, check_long_hedge_condition |
| TrendStatusDetector | trend_status_detector.py | 日线趋势检测 | check_long_entry, check_short_entry, check_long_hedge_condition |
| singleAssetTrader | trend_main.py | 趋势交易执行 | open_position, place_order, _run_loop |
| singleAssetTrader | pin_main.py | 插针交易执行 | open_position, place_order, _run_loop |

### 核心枚举分析

| 枚举名 | 定义位置 | 状态值 |
|--------|---------|--------|
| MarketStatus | 所有文件 | PIN, RANGE, TREND, INVALID |
| Direction | trend_main.py, pin_main.py | LONG, SHORT |
| PinStatus | trend_main.py, pin_main.py | INITIAL, HEDGE_ENTER, POS_LOCKED, Long_*, Short_* |
| PinIntensity | market_status.py | WEAK, MODERATE, STRONG |

### 关键业务逻辑

1. **市场状态检测流程**: validate_data -> detect_pin_status -> detect_range_status -> TREND(默认)
2. **插针交易流程**: 检测PIN状态 -> 做多/做空开仓 -> 对冲/加仓 -> 退出高波动 -> 切换TREND模式
3. **趋势交易流程**: 检测TREND状态 -> 根据日线指标平仓/对冲 -> 保本/指标平仓
4. **风控机制**: 下单频率限制、Redis分布式锁、盈亏计算、数据持久化

### 与Rust实现的对应关系提示

- `MarketDetector` -> Rust: MarketStatusDetector in market module
- `PinStatusDetector` -> Rust: PinStatusDetector in strategy module
- `TrendStatusDetector` -> Rust: TrendStatusDetector in strategy module
- `singleAssetTrader` -> Rust: TradingEngine in engine module
- `place_order` -> Rust: OrderExecutor
- `open_position` -> Rust: trading_engine.run_iteration()
