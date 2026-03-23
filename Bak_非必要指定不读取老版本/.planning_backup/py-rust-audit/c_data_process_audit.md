# c_data_process/ Python 代码审计报告

## 文件清单
- [x] __init__.py (空文件)
- [x] indicator_1d/__init__.py (空文件)
- [x] indicator_1d/main.py
- [x] indicator_1d/big_cycle_calc.py
- [x] indicator_1d/check.py
- [x] indicator_1d/pine_scripts.py
- [x] indicator_1m/__init__.py (空文件)
- [x] indicator_1m/indicator_calc.py
- [x] indicator_1m/main.py
- [x] indicator_1m/check.py
- [x] load_indicators_from_redis.py

---

## 详细审计

### 文件: __init__.py (root)

空文件，无内容

---

### 文件: indicator_1d/__init__.py

空文件，无内容

---

### 文件: indicator_1d/main.py

```python
# 市场分析器主程序 - 区分已持仓(快速通道)和未持仓(慢速通道)
# 核心功能：从Redis加载持仓品种，计算波动率指标，写入Redis
```

#### class MarketAnalyzer
- `__init__()` - 初始化分析器，创建线程池，设置运行状态
- `_validate_and_clean_hold_symbol(symbol, timestamp)` - 校验单个持仓品种时间戳，过期删除
- `_refresh_hold_symbols()` - 从Redis Hash获取已持仓品种列表（含时间戳校验）
- `update_hold_symbol_timestamp(symbol, timestamp=None)` - 更新持仓品种时间戳到Redis Hash
- `_load_all_vol_symbols()` - 获取全量波动率排行ZSET
- `_assign_symbols()` - 核心分配逻辑：持仓<=20个品种，其余分配给高波动未持仓
- `load_klines(symbol, interval='1m', limit=500)` - 加载K线数据（历史+实时）
- `volatility_analysis(symbol, interval='1m')` - 波动分析核心逻辑，计算指标并存入Redis
- `process_fast()` - 快速通道处理逻辑（每10秒运行，已持仓品种）
- `process_slow()` - 慢速通道处理逻辑（每60秒运行，高波动未持仓品种）
- `start_analysis()` - 启动分析主逻辑，创建快速/慢速通道线程
- `startloop()` - 启动主循环
- `stoploop()` - 优雅停止所有任务，关闭线程池

#### 函数
- `serialize_numpy(obj)` - 序列化numpy类型为Python原生类型
- `add_test_hold_symbol(symbol, timestamp=None)` - 添加测试持仓品种
- `start()` - 启动分析器
- `run()` - 后台运行模式

---

### 文件: indicator_1d/big_cycle_calc.py

```python
# 金融指标计算核心类 - 日线版本
# 提供全量日线金融指标计算能力（TR、速度、加速度、位置、Jerk等）
```

#### @dataclass IndicatorConfig
配置数据类，包含以下字段分组：
- **TR窗口参数**: TR_WINDOW_HIST, WINDOW_5D, WINDOW_20D, WINDOW_60D, WINDOW_14D
- **动能平滑参数**: SMOOTH_VEL_WIN, SMOOTH_ACC_WIN, NORM_WIN
- **阈值参数**: NORM_CLIP_MIN/MAX, TR_RATIO_CLIP_MAX, RSI_OVERBOUGHT/OVERSOLD, TREND_FLAT_THRESHOLD
- **日线专属参数**: WINDOW_ACC_SMOOTH_D, WINDOW_VEL_PERCENTILE_D, WINDOW_ACC_PERCENTILE_D, VELOCITY_BASE_THRESHOLD_D, ACC_BASE_THRESHOLD_D, ACC_EXTREME_THRESHOLD_D, VELOCITY_TURN_THRESHOLD_D, MIN_VALID_SAMPLES_D
- **数学常量**: EPSILON
- **MA参数**: MEDIUM_SHORT_MA, MEDIUM_TREND_MA
- **Power参数**: WINDOW_POWER_PERCENTILE
- **TR比率参数**: TR_RATIO_RANK_WINDOW, TR_RATIO_MIN_VALID_SAMPLES
- **Jerk参数**: JERK_MID_MA_LEN, JERK_TREND_MA_LEN, JERK_SMOOTH_VEL, JERK_SMOOTH_ACC, JERK_NORM_WIN, JERK_EMA_SPAN

#### class IndicatorCalculator
- `__init__(tr_window, config)` - 初始化指标计算器
- `_validate_config_params()` - 参数校验，非法值重置为默认值
- `_safe_division(numerator, denominator)` - 安全除法，防止除零
- `_ts_to_readable(ts)` - 时间戳转换为可读格式
- `_validate_input_data(df_daily_closed)` - 输入数据校验，自动修正价格异常
- `_format_input_data(df_daily_closed)` - 格式化输入数据，生成时间戳字段
- `_get_daily_trend_dir(df)` - 计算日线趋势方向（考虑震荡区阈值）
- `_is_trend_switch(trend_series)` - 向量化检测趋势切换点
- `_calculate_daily_velocity_percentile(df)` - 计算日线速度百分位（范围-100~100）
- `_calculate_daily_acceleration_percentile(df)` - 计算日线加速度百分位（范围-100~100）
- `_calculate_power_indicators(df)` - 计算Power指标及60日百分比排位
- `_calculate_tr_ratio_rank(df, tr_ratio_col, rank_col_name)` - 计算TR比率的20日百分比排名
- `_calculate_velocity_acceleration(df)` - 计算速度/加速度相关指标
- `_calculate_tr_indicators(df)` - 计算TR系列指标（含排名）
- `_calculate_position_indicators(df)` - 计算区间位置指标
- `_vectorized_real_time_tr(df, hist_index)` - 向量化计算实时TR（5日/20日）
- `_precompute_hist_index(n_rows)` - 预计算历史索引矩阵
- `_calculate_high_order_indicators(df)` - 计算高阶动能指标（Jerk急动度）
- `_format_output_data(df, output_file_path)` - 格式化输出数据
- `_calculate_core_indicators(symbol, df, n_rows)` - 核心指标计算主流程
- `calculate_all_daily_indicators(symbol, df_daily_closed, output_file_path)` - 全量日线指标计算入口

---

### 文件: indicator_1d/check.py

```python
# Redis指标数据读取工具 - 支持1m/1h/1d多周期
# 核心功能：从Redis读取指标数据并转换为Pandas DataFrame
```

#### class RedisConfig
- `INDICATORS_PREFIX` - 指标Redis键前缀
- `DB_INDEX` - Redis数据库索引

#### 函数
- `serialize_numpy(obj)` - 序列化numpy类型为Python原生类型
- `load_indicators_from_redis(symbol, interval='1m')` - 从Redis读取指标数据返回DataFrame
- `load_multi_period_indicators(symbol, intervals=None)` - 批量读取多周期指标数据

---

### 文件: indicator_1d/pine_scripts.py

```python
# Pine指标计算 - 交易条件、颜色映射、振幅计算
# 支持Pine Script风格的颜色信号和交易触发条件
```

#### @dataclass PineConfig
- **Pine参数**: PINE_FAST_LENGTH, PINE_SLOW_LENGTH, PINE_SIGNAL_LENGTH, PINE_SMA_SOURCE, PINE_SMA_SIGNAL
- **CRSI参数**: PINE_DOMCYCLE, PINE_VIBRATION, PINE_LEVELING, PINE_CYCLICMEMORY
- **RSI阈值**: RSI_OVERBOUGHT, RSI_OVERSOLD, EPSILON
- **颜色映射**: PINE_COLOR_MAP, PINE_BG_COLOR_MAP

#### class PineColorOnlyCalculator
- `__init__(fast_length, slow_length, signal_length)` - 初始化Pine计算器
- `_validate_params()` - 参数校验
- `_safe_division(n, d)` - 安全除法
- `_rma_vectorized(series, window)` - 向量化RMA计算
- `_sma_or_ema(series, window, ma_type)` - SMA或EMA计算
- `_sliding_window_max_min(arr, window_size)` - 滑动窗口最大最小值
- `_vectorized_crsi(rsi, torque, phasingLag)` - 向量化CRSI计算
- `_vectorized_db_ub(crsi, lmax, lmin, cyclicmemory, aperc)` - 向量化上下轨计算
- `calculate_pine_macd(df)` - 计算Pine MACD指标
- `calculate_pine_rsi(df)` - 计算Pine RSI和CRSI指标
- `calculate_trade_conditions(df)` - 计算买卖条件
- `calculate_bar_color(df)` - 计算K线颜色
- `calculate_bg_color(df)` - 计算背景颜色
- `_calc_top3_avg_amplitude_pct(df)` - 计算TOP3平均振幅百分比
- `_calc_one_percent_amplitude_time_days()` - 计算1%振幅对应时间（天）
- `_check_trading_conditions(df, symbol, avg_amp_pct)` - 检查交易条件
- `_get_color_series_simple(bg)` - 简化色系判断
- `calculate_colors_only(df, symbol, output_path)` - 核心入口，计算所有颜色指标

#### 函数
- `get_binance_1d_data(symbol, limit)` - 拉取币安1日K线数据

---

### 文件: indicator_1m/__init__.py

空文件，无内容

---

### 文件: indicator_1m/indicator_calc.py

```python
# 指标计算工具 v2.6 - 1分钟K线版本
# 新增策略生成时间戳字段 strategy_calc_ts
```

#### class DBConfig
- `REDIS_DB_NAME` - Redis数据库名
- `REDIS_HOST`, `REDIS_PORT`, `REDIS_PASSWORD`, `REDIS_TIMEOUT`, `REDIS_MAX_CONNECTIONS` - Redis连接配置

#### class IndicatorCalculator
- `__init__(tr_window)` - 初始化，设置K线周期窗口参数
- `_init_logger()` - 初始化日志配置
- `_percentileofscore_numba(a, score, kind)` - Numba加速的百分位计算
- `_vectorized_rolling_percentile(arr, window, kind, default_val)` - 向量化滚动百分位计算
- `_calculate_velocity_percentile_vectorized(vel_series)` - 向量化速度百分位计算
- `_calculate_acc_percentile_vectorized(vel_series, acc_series)` - 向量化加速度百分位计算
- `_calculate_power_percentile_vectorized(power_series)` - 向量化Power百分位计算
- `_calculate_velocity_percentile(hist_base, current_vel)` - 原有逐行速度百分位计算（保留兼容）
- `_calculate_acc_percentile_1h(hist_base, current_acc, current_vel)` - 原有加速度百分位计算（保留兼容）
- `_calculate_power_percentile_60d(hist_base, current_power)` - 原有Power百分位计算（保留兼容）
- `_calculate_signed_acc_strength(hist_base, current_acc, current_vel)` - 带方向的加速度强度计算
- `_calculate_aligned_acc_percentile(hist_base, current_acc, trend_dir)` - 带趋势方向的加速度百分位
- `_precompute_df_1m_indicators_inplace(df_1m)` - 原地预计算核心指标
- `_calculate_real_time_tr_vectorized(close_series, hist_base, window, anchor_shift)` - 实时TR向量化计算
- `_calculate_high_order_kinetic_indicators(df)` - 高阶动能系列指标计算（Jerk等）
- `_calculate_pine_ema_indicators(df)` - Pine EMA指标计算
- `_calculate_pine_macd_indicators(df)` - Pine MACD相关指标计算
- `_calculate_pine_crsi_indicators(df)` - Pine CRSI相关指标计算
- `_calculate_pine_trade_conditions(df)` - Pine 买卖条件计算
- `_calculate_pine_color_indicators(df)` - Pine 颜色映射计算
- `calculate_indicators_batch(df_1m_combined, output_file_path)` - 主方法，批量计算所有指标

---

### 文件: indicator_1m/main.py

```python
# 核心分析器 - 1小时均匀处理版
# 核心功能：每小时内均匀分配处理所有品种，计算日线指标
```

#### 函数
- `retry_decorator(max_retries, delay)` - 重试装饰器
- `smart_sleep(seconds, is_running_flag)` - 智能睡眠，支持优雅退出
- `check_api_weight_limit(response_headers)` - 检查API权重限制
- `deduplicate_klines(klines_data)` - K线数据去重（基于开盘时间）

#### class MarketAnalyzer
- `__init__()` - 初始化分析器，动态计算处理间隔
- `_clean_expired_cache()` - 定期清理过期缓存线程
- `_validate_and_clean_symbol(symbol, timestamp)` - 校验交易对时间戳
- `_load_subscribed_symbols()` - 加载全量订阅品种
- `_load_slow_symbols()` - 加载所有交易对（1小时内均匀处理）
- `update_symbol_timestamp(symbol, timestamp=None)` - 更新交易对时间戳
- `_fetch_batch_klines(symbol, interval, start_ts, end_ts, batch_size)` - 拉取单批次K线数据
- `_fetch_historical_klines_from_2010(symbol, interval)` - 从2010年开始拉取全量历史K线
- `_fetch_klines_from_api(symbol, interval)` - 从API拉取K线（带重试）
- `_safe_redis_get(key, field)` - 安全读取Redis
- `_safe_redis_set(key, field, value)` - 安全写入Redis
- `load_klines(symbol, interval)` - 加载K线（支持缓存）
- `_validate_kline_time(data, interval)` - 校验K线时间有效性
- `volatility_analysis(symbol, interval)` - 波动分析
- `process_slow_symbols()` - 主处理逻辑（1小时内均匀处理所有品种）
- `startloop()` - 启动主循环
- `stoploop()` - 优雅停止

#### 函数
- `start()` - 生产环境启动入口
- `run()` - 后台运行

---

### 文件: indicator_1m/check.py

与 indicator_1d/check.py 内容相同，为空文件或占位文件

---

### 文件: load_indicators_from_redis.py

```python
# 独立工具脚本 - 从Redis读取指标数据并打印
# 核心功能：循环读取Redis中的指标数据，转换为CSV保存
```

#### 函数
- 无自定义类和函数
- 主程序逻辑在 `if __name__ == '__main__'` 中：
  - 循环读取 `indicators:1m:BTCUSDT` 键
  - 从Redis读取数据
  - 解码bytes/str，解析JSON
  - 转换为Pandas DataFrame
  - 保存为CSV文件
  - 每隔1秒读取一次

---

## 总结

### 目录结构
```
c_data_process/
├── __init__.py
├── load_indicators_from_redis.py          # Redis指标读取工具
├── indicator_1d/                          # 日线指标模块
│   ├── __init__.py
│   ├── main.py                            # 市场分析器主程序
│   ├── big_cycle_calc.py                  # 日线指标计算器
│   ├── check.py                           # Redis读取工具
│   └── pine_scripts.py                    # Pine颜色/交易条件计算
└── indicator_1m/                          # 1分钟指标模块
    ├── __init__.py
    ├── main.py                            # 核心分析器（1小时均匀处理）
    ├── indicator_calc.py                  # 1分钟指标计算器
    └── check.py                           # (占位文件)
```

### 核心依赖
- **数据处理**: pandas, numpy
- **科学计算**: scipy.stats (percentileofscore)
- **可选加速**: numba (jit编译)
- **Redis**: redis_tool (自定义客户端)
- **配置**: a_common.config.settings
- **日志**: EnhancedLogger

### 关键业务逻辑
1. **双通道模式**: 快速通道(已持仓，10秒间隔) + 慢速通道(高波动未持仓，60秒间隔)
2. **波动率指标**: TR比率、Z-Score、速度/加速度百分位
3. **Pine颜色信号**: 基于MACD+EMA+RSI的趋势颜色判断
4. **交易条件**: 振幅阈值(80%) + 效率阈值(3天/1%)
5. **Redis缓存**: K线数据、指标数据、持仓品种Hash

### 审计建议
- 代码存在大量重复逻辑（如多个百分位计算函数），建议抽取公共方法
- 日志输出较分散，建议统一日志模块
- 部分硬编码配置应提取到配置文件
