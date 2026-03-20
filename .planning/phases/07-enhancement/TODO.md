================================================================================
Phase 7 Enhancement 待办清单
================================================================================

## 7.1 风控两层完善
--------------------------------------------------------------------------------
[x] 创建 engine/src/risk_rechecker.rs ✓
[x] 实现 RiskReChecker 结构体 ✓
[x] 实现 re_check() 锁内复核方法 ✓
[x] 实现 check_volatility_realtime() 实时波动率检查 ✓
[x] 更新 lib.rs 导出新模块 ✓

## 7.2 PnlManager 盈亏管理模块
--------------------------------------------------------------------------------
[x] 创建 engine/src/pnl_manager.rs ✓
[x] 实现 PnlManager 结构体 ✓
[x] 实现 calculate_realized_pnl() 已实现盈亏计算 ✓
[x] 实现 calculate_unrealized_pnl() 未实现盈亏计算 ✓
[x] 实现 update_cumulative_profit() 累计盈利更新 ✓
[x] 实现 check_and_rescue_low_volatility() 低波动解救 ✓
[x] 更新 lib.rs 导出新模块 ✓

## 7.3 MarketStatusDetector 市场状态检测
--------------------------------------------------------------------------------
[x] 创建 engine/src/market_status.rs ✓
[x] 定义 MarketStatus 枚举 (PIN/RANGE/TREND/INVALID) ✓
[x] 实现 MarketStatusDetector 结构体 ✓
[x] 实现 detect() 方法判断市场状态 ✓
[x] 实现 PinIntensity 等级 (WEAK/MODERATE/STRONG) ✓
[x] 更新 lib.rs 导出新模块 ✓

## 7.4 仓位互斥判断
--------------------------------------------------------------------------------
[x] 创建 engine/src/position_exclusion.rs ✓
[x] 实现 PositionExclusionChecker 结构体 ✓
[x] 实现 check_long_short_mutex() 多空互斥检查 ✓
[x] 实现 has_position() 检查持仓存在 ✓
[x] 实现 cross_symbol_mutex 跨品种互斥检查 ✓
[x] 更新 lib.rs 导出新模块 ✓

## 7.5 日线指标周期支持
--------------------------------------------------------------------------------
[x] 修改 engine/src/channel.rs ✓
[x] 添加 kline_1d K线合成器 ✓
[x] 添加 ema_100, ema_200 日线 EMA ✓
[x] 添加 rsi_daily 日线 RSI ✓
[x] 添加 price_position_daily 日线价格位置 ✓
[x] 修改 on_tick() 支持日线更新 ✓

## 7.6 阈值常量模块
--------------------------------------------------------------------------------
[x] 创建 engine/src/thresholds.rs ✓
[x] 定义 THRESHOLD 常量结构体 ✓
[x] 添加所有策略阈值常量 ✓
[x] 实现 Default 提供默认值 ✓
[x] 更新 lib.rs 导出新模块 ✓

================================================================================
完成记录
================================================================================

v0.9 阶段新增模块:
- RiskReChecker: 风控锁内复核 (2026-03-20)
- PnlManager: 盈亏管理模块 (2026-03-20)
- MarketStatusDetector: 市场状态检测 (2026-03-20)
- PositionExclusionChecker: 仓位互斥检查 (2026-03-20)
- ThresholdConstants: 阈值常量集中管理 (2026-03-20)
- 日线指标支持: channel.rs 添加日线 EMA/RSI/PricePosition (2026-03-20)
- OrderCheck: 订单风控检查器 (2026-03-20)

================================================================================
