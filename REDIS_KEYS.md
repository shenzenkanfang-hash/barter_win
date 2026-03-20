# Redis Keys Configuration | Redis键值配置文档

## 文档信息

- **Author:** 产品经理
- **Created:** 2026-03-20
- **Stage:** map-codebase
- **Status:** 已整理
- **Next:** 开发者

---

## 概述

本文档整理了项目中所有使用的Redis键值，按照业务域分组，包含键名、用途说明和数据类型。

---

## 1. 统一前缀规范

### 当前使用的前缀

| 前缀 | 来源 | 说明 |
|------|------|------|
| `atlas:v2:` | `backup_old_code/a_common/constants.py` | 新版统一前缀 |
| 无前缀（旧） | `infrastructure/config/settings.py` | 老版交易系统 |
| `lock:` | `infrastructure/client/redis.py` | 分布式锁专用 |

---

## 2. 市场数据域 (Market Data)

### 2.1 实时K线数据

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `kline_{interval}` | `kline_1m`, `kline_1h`, `kline_1d` | 实时K线数据（WebSocket推送） | Hash | `infrastructure/client/binance.py:88` |
| `kline:realtime:{symbol}` | `kline:realtime:BTCUSDT` | 实时K线（老版本） | String | `infrastructure/config/settings.py:35` |
| `kline:history:{symbol}:` | `kline:history:BTCUSDT:1d` | 历史K线数据 | String | `infrastructure/config/settings.py:36` |

### 2.2 盘口深度数据

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `{symbol}_depthUpdate` | `BTCUSDT_depthUpdate` | 盘口深度更新 | String | `infrastructure/client/binance.py:108` |
| `mkt:depth:{symbol}` | `atlas:v2:mkt:depth:BTCUSDT` | 盘口深度（新版） | String | `backup_old_code/a_common/constants.py:50` |

### 2.3 行情数据

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `mkt:ticker:{symbol}` | `atlas:v2:mkt:ticker:BTCUSDT` | 市场行情实时价格 | String | `backup_old_code/a_common/constants.py:44` |
| `mkt:ticker:batch` | `atlas:v2:mkt:ticker:batch` | 批量行情缓存（全市场扫描） | String | `backup_old_code/a_common/constants.py:53` |

### 2.4 交易对列表

| 键名 | 用途 | 类型 | 来源 |
|------|------|------|------|
| `gateway:symbol_list` | 交易对列表缓存 | String | `infrastructure/config/settings.py:34` |
| `exchange_info` | 交易所交易规则信息（全币种） | Hash | `infrastructure/client/binance.py:72` |
| `trade:exchange:info` | 交易所交易规则（新版） | String | `backup_old_code/a_common/constants.py:118` |

---

## 3. 账户与资产域 (Account & Asset)

### 3.1 账户余额

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `acc:balance:{asset}` | `atlas:v2:acc:balance:USDT` | 账户余额 | String | `backup_old_code/a_common/constants.py:61` |
| `acc:info` | `atlas:v2:acc:info` | 账户完整信息 | String | `backup_old_code/a_common/constants.py:64` |
| `acc:margin` | `atlas:v2:acc:margin` | 账户保证金信息 | String | `backup_old_code/a_common/constants.py:67` |

---

## 4. 持仓与仓位域 (Position)

### 4.1 持仓信息

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `position:{symbol}` | `position:BTCUSDT` | 当前持仓信息 | String | `services/position/manager.py:45` |
| `pos:info:{symbol}` | `atlas:v2:pos:info:BTCUSDT` | 持仓详细信息（新版） | String | `backup_old_code/a_common/constants.py:87` |
| `{symbol}positionRisk` | `BTCUSDTpositionRisk` | 持仓风险信息 | String | `infrastructure/client/binance.py:154` |
| `pst_risk` | `pst_risk` | 持仓风险列表 | String | `infrastructure/client/binance.py:146` |

### 4.2 持仓风险

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `pos:risk:long:{symbol}` | `atlas:v2:pos:risk:long:BTCUSDT` | 多头持仓风险 | String | `backup_old_code/a_common/constants.py:75` |
| `pos:risk:short:{symbol}` | `atlas:v2:pos:risk:short:BTCUSDT` | 空头持仓风险 | String | `backup_old_code/a_common/constants.py:78` |
| `pos:risk:long` | `atlas:v2:pos:risk:long` | 多头持仓哈希表（全局） | Hash | `backup_old_code/a_common/constants.py:81` |
| `pos:risk:short` | `atlas:v2:pos:risk:short` | 空头持仓哈希表（全局） | Hash | `backup_old_code/a_common/constants.py:84` |

---

## 5. 风控域 (Risk Management)

### 5.1 保证金

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `risk:strategy:used:{level}` | `atlas:v2:risk:strategy:used:MINUTE` | 策略已用保证金 | String | `backup_old_code/a_common/constants.py:95` |
| `risk:global:used` | `atlas:v2:risk:global:used` | 全局已用保证金 | String | `backup_old_code/a_common/constants.py:98` |

### 5.2 风控限制

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `risk:limit:{strategy_id}` | `atlas:v2:risk:limit:trend_001` | 策略风控限制配置 | String | `backup_old_code/a_common/constants.py:101` |
| `risk:brackets:{symbol}` | `atlas:v2:risk:brackets:BTCUSDT` | 杠杆分层信息 | String | `backup_old_code/a_common/constants.py:104` |
| `brackets` | `brackets` | 杠杆档位缓存 | Hash | `infrastructure/client/binance.py:134` |

---

## 6. 订单与交易域 (Order & Trade)

### 6.1 订单

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `trade:order:pending:{order_id}` | `atlas:v2:trade:order:pending:123456` | 挂单信息 | String | `backup_old_code/a_common/constants.py:112` |
| `trade:order:history:{symbol}` | `atlas:v2:trade:order:history:BTCUSDT` | 订单历史 | String | `backup_old_code/a_common/constants.py:115` |

---

## 7. 策略状态域 (Strategy State)

### 7.1 策略状态

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `strat:state:{strategy_id}:{symbol}` | `atlas:v2:strat:state:trend_001:BTCUSDT` | 策略当前状态 | String | `backup_old_code/a_common/constants.py:126` |
| `strat:signal:{strategy_id}:{symbol}` | `atlas:v2:strat:signal:trend_001:BTCUSDT` | 策略最新信号 | String | `backup_old_code/a_common/constants.py:129` |
| `strat:pnl:{strategy_id}:{symbol}` | `atlas:v2:strat:pnl:trend_001:BTCUSDT` | 策略盈亏统计 | String | `backup_old_code/a_common/constants.py:132` |
| `strat:registry` | `atlas:v2:strat:registry` | 策略注册表 | String | `backup_old_code/a_common/constants.py:135` |

---

## 8. 指标计算域 (Indicators)

### 8.1 指标数据

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `indicators:` | `indicators:1m:BTCUSDT` | 技术指标数据（Hash类型，RedisConfig定义） | Hash | `infrastructure/config/settings.py:41` |
| `indicators:{period}:{symbol}` | `indicators:1d:BTCUSDT` | 指标数据（新版格式） | String | `strategies/implementations/trend/detector.py:54` |
| `ind:{indicator}:{symbol}:{interval}` | `atlas:v2:ind:EMA:BTCUSDT:1h` | 指标计算结果缓存 | String | `backup_old_code/a_common/constants.py:157` |

### 8.2 指标字段说明

**1m周期指标字段：**
```
timestamp, parent_1m_ts, tick_index, open, high, low, close, volume,
tr_ratio_zscore_10min_1h, tr_ratio_10min_1h, tr_base_10min,
tr_ratio_zscore_60min_5h, tr_ratio_60min_5h, tr_base_60min,
zscore_1h_1m, zscore_14_1m, jerk_signal, pos_norm_60,
price_deviation, price_deviation_horizontal_position,
velocity_1m, velocity_percentile_1h, acceleration_1m,
acc_percentile_1h, power, power_percentile_60d,
strategy_calc_ts, real_time_tr_10min, real_time_tr_60min
```

**1h周期指标字段：**
```
timestamp, parent_1m_ts, tick_index, open, high, low, close, volume,
tr_ratio_zscore_5d_20d, tr_ratio_5d_all, tr_ratio_5d_20d, tr_base_5d,
tr_ratio_zscore_20d_60d, tr_ratio_20d_all, tr_ratio_20d_60d, tr_base_20d,
jerk_signal, pos_norm_20, ma5_close_in_20d_ma5_pos, ma20_close_in_60d_ma20_pos,
ma5_close_in_all_ma5_pos, ma20_close_in_all_ma20_pos,
vel_percentile_d, acc_percentile_d, power, power_percentile_60d,
strategy_calc_ts, real_time_tr_5d, real_time_tr_20d,
real_time_tr_ratio_5d_20d, real_time_tr_ratio_20d_60d
```

**1d周期指标字段：**
```
timestamp, parent_1m_ts, tick_index, open, high, low, volume,
tr_ratio_5d_all, tr_ratio_5d_20d_rank_20d, tr_ratio_5d_20d, tr_base_5d,
tr_ratio_20d_all, tr_ratio_20d_60d_rank_20d, tr_ratio_20d_60d, tr_base_20d,
readable_time, close, jerk_signal, top3_avg_amplitude_pct,
pine_bar_color_20_50, pine_bg_color_20_50,
pine_bar_color_100_200, pine_bg_color_100_200,
pine_bar_color_12_26, pine_bg_color_12_26,
real_time_tr_5d, real_time_tr_20d, ema_100_200_compare,
real_time_tr_ratio_5d_20d, real_time_tr_ratio_20d_60d,
pos_norm_20, ma5_close_in_20d_ma5_pos, ma20_close_in_60d_ma20_pos,
ma5_close_in_all_ma5_pos, ma20_close_in_all_ma20_pos,
vel_percentile_d, acc_percentile_d, power, power_percentile_60d,
strategy_calc_ts
```

---

## 9. 系统运行域 (System Runtime)

### 9.1 分布式锁

| 键名模式 | 完整示例 | 用途 | 类型 | 来源 |
|----------|----------|------|------|------|
| `lock:{resource}` | `lock:order_123` | 分布式锁（全局） | String | `infrastructure/client/redis.py:197` |
| `sys:lock:{resource}` | `atlas:v2:sys:lock:position` | 分布式锁（新版） | String | `backup_old_code/a_common/constants.py:146` |

### 9.2 系统状态

| 键名 | 用途 | 类型 | 来源 |
|------|------|------|------|
| `sys:on_running` | 当前运行的策略标记 | String | `backup_old_code/a_common/constants.py:143` |
| `sys:heartbeat:{service}` | 服务心跳 | String | `backup_old_code/a_common/constants.py:149` |

---

## 10. 交易对规则域 (Symbol Rules)

### 10.1 交易对规则

| 键名 | 用途 | 类型 | 来源 |
|------|------|------|------|
| `symbol_rules` | 主交易对规则 | Hash | `domain/account/symbol_rules.py:34` |
| `symbol_rules:v2:{symbol}` | 交易对规则缓存（新版） | String | `backup_old_code/a_common/model/symbol_rules.py:232` |
| `leverage_brackets` | 杠杆档位缓存 | Hash | `domain/account/symbol_rules.py:35` |
| `commission_info` | 手续费率缓存 | Hash | `domain/account/symbol_rules.py:36` |

---

## 11. 其他业务键

### 11.1 高频交易相关

| 键名 | 用途 | 类型 | 来源 |
|------|------|------|------|
| `high_vol_1h` | 1小时高交易量币种 | ZSet | `infrastructure/config/settings.py:38` |
| `fast_hold_symbols` | 快周期持有币种 | Hash | `infrastructure/config/settings.py:39` |
| `slow_hold_symbols` | 慢周期持有币种 | Hash | `infrastructure/config/settings.py:40` |
| `continuous_amplitude` | 连续振幅 | ZSet | `infrastructure/config/settings.py:42` |
| `pending_trading` | 待交易 | String | `infrastructure/config/settings.py:44` |
| `trading:symbol_config` | 交易品种配置 | Set | `infrastructure/config/settings.py:37` |

---

## 12. Redis数据类型汇总

| 数据类型 | 键数量 | 键名示例 |
|----------|--------|----------|
| String | ~25 | `position:BTCUSDT`, `pst_risk`, `atlas:v2:...` |
| Hash | ~8 | `kline_1m`, `brackets`, `symbol_rules` |
| ZSet | ~2 | `high_vol_1h`, `continuous_amplitude` |
| Set | ~1 | `trading:symbol_config` |

---

## 13. Redis连接配置

| 配置项 | 值 | 来源 |
|--------|-----|------|
| Unix Socket | `/var/run/redis/redis-server.sock` | `infrastructure/client/redis.py:15` |
| TCP Host | `127.0.0.1` | `infrastructure/client/redis.py:48` |
| TCP Port | `6379` | `infrastructure/client/redis.py:49` |
| DB Index | `0` | `infrastructure/config/settings.py:33` |
| 连接方式 | Unix Socket优先，失败5次后回退TCP | `infrastructure/client/redis.py` |

---

## 14. 使用建议

1. **统一前缀**：建议所有新键使用 `atlas:v2:` 前缀
2. **键名规范**：采用 `业务域:子域:具体标识` 三层结构
3. **序列化**：所有值使用JSON序列化（orjson）
4. **过期时间**：根据业务设置合理的TTL
5. **分布式锁**：使用 `lock:` 前缀的键配合Lua脚本实现

---

## 修改记录

| 日期 | 修改人 | 修改内容 |
|------|--------|----------|
| 2026-03-20 | 产品经理 | 初始整理Redis键值配置 |
