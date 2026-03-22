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

## 7.7 Z-Score / TR-Ratio 指标框架
--------------------------------------------------------------------------------
[x] 创建 indicator/src/z_score.rs ✓
[x] 实现 ZScore 结构体 (Welford's 算法) ✓
[x] 实现 ZScoreSignal 信号枚举 ✓
[x] 创建 indicator/src/tr_ratio.rs ✓
[x] 实现 TRRatio 结构体 ✓
[x] 实现 TRRatioSignal 信号枚举 ✓
[x] 更新 indicator/lib.rs 导出新模块 ✓

## 7.8 LocalPositionManager 持仓管理器
--------------------------------------------------------------------------------
[x] 创建 engine/src/position_manager.rs ✓
[x] 实现 LocalPositionManager 结构体 ✓
[x] 实现 open_position() / close_position() ✓
[x] 实现 unrealized_pnl() 未实现盈亏计算 ✓
[x] 更新 engine/lib.rs 导出新模块 ✓

## 7.9 TrendStrategy 趋势策略
--------------------------------------------------------------------------------
[x] 创建 strategy/src/trend_strategy.rs ✓
[x] 实现 TrendStrategy 结构体 ✓
[x] 实现 TrendState 状态机 (Idle/Long/Short) ✓
[x] 实现 TrendSignal 信号枚举 ✓
[x] 实现 check_signal() 基于 EMA/PineColor/RSI ✓
[x] 更新 strategy/lib.rs 导出新模块 ✓

## 7.10 PinStrategy 马丁/插针策略
--------------------------------------------------------------------------------
[x] 创建 strategy/src/pin_strategy.rs ✓
[x] 实现 PinStrategy 结构体 ✓
[x] 实现 PinState 状态机 (Idle/Opening/Holding/Hedging) ✓
[x] 实现 PinSignal 信号枚举 ✓
[x] 实现 check_signal() 基于 Z-Score/TR-Ratio ✓
[x] 更新 strategy/lib.rs 导出新模块 ✓

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
- ZScore/TRRatio: 指标框架 (2026-03-20)
- LocalPositionManager: 持仓管理器 (2026-03-20)
- TrendStrategy: 趋势策略状态机 (2026-03-20)
- PinStrategy: 马丁/插针策略状态机 (2026-03-20)

待调整:
- 指标计算逻辑需根据 D:/量化策略开发/tradingW/backup_old_code/c_data_process/indicator_1m/indicator_calc.py 调整
- 指标计算逻辑需根据 D:/量化策略开发/tradingW/backup_old_code/c_data_process/indicator_1d/pine_scripts.py 调整

================================================================================
v1.1 待办清单
================================================================================

## A. MockBinanceGateway - 模拟币安网关
--------------------------------------------------------------------------------
[ ] 创建 engine/src/mock_binance_gateway.rs
[ ] 实现 MockAccount 模拟账户
[ ] 实现 MockPosition 模拟持仓
[ ] 实现 MockOrder 模拟订单
[ ] 实现 MockMargin 模拟保证金计算
[ ] 实现风控检查（与币安一致）
    - [ ] check_account_balance() 可用余额检查
    - [ ] check_position_limit() 持仓限制检查
    - [ ] check_margin_sufficient() 保证金充足检查
    - [ ] check_forced_liquidation() 强制平仓检查
[ ] 实现立即成交机制（Market Order）
[ ] CSV 输出
    - [ ] trades.csv 交易记录
    - [ ] positions.csv 持仓变化
    - [ ] risk_log.csv 风控日志
    - [ ] account_snapshot.csv 账户快照
    - [ ] indicator_comparison.csv 指标对比

## B. 信号综合层 - 通道退出逻辑
--------------------------------------------------------------------------------
参考:
  - pin_status_detector.py (分钟级): 进入高速/退出高速/马丁策略
  - trend_status_detector.py (日线级): 趋势策略/平仓条件

通道切换逻辑：
  1. 进入高速：15min>=13% 或 1min>=3% → 马丁策略(分钟级)
  2. 退出高速：tr_ratio < 1 → 回到慢速通道
  3. 日线趋势平仓：ma5_close位置 + Pine颜色

[ ] 实现 tr_ratio < 1 退出条件判断
[ ] 实现日线趋势平仓条件(ma5_close + PineColor)
[ ] 实现通道状态变化记录
[ ] 输出 trigger_log.csv

## C. 完整测试用例
--------------------------------------------------------------------------------
[ ] 指标层测试
    - [ ] EMA 增量计算测试
    - [ ] RSI 计算测试
    - [ ] PineColor 判断测试
    - [ ] BigCycleCalculator 测试
[ ] 策略层测试
    - [ ] TrendStrategy 状态机测试
    - [ ] PinStrategy 状态机测试
[ ] 风控层测试
    - [ ] RiskPreChecker 测试
    - [ ] RiskReChecker 测试
    - [ ] AccountPool 测试
[ ] 引擎层测试
    - [ ] VolatilityChannel 通道切换测试
    - [ ] TradingEngine 集成测试
[ ] MockBinanceGateway 测试
    - [ ] 正常交易流程测试
    - [ ] 风控拒绝场景测试
    - [ ] 强制平仓场景测试

## D. 指标对比验证
--------------------------------------------------------------------------------
[ ] 同步输出 Rust 计算的指标值
[ ] 提供 Python 指标对比接口
[ ] 生成 indicator_comparison.csv
[ ] 用户验证准确性

================================================================================
v1.7 分层数据持久化架构
================================================================================

## 架构分层
--------------------------------------------------------------------------------
| 层级 | 存储位置 | 用途 | 状态 |
|------|----------|------|------|
| 1 | 内存数据结构 | 程序运行时使用，最快 | ✅ 已实现 |
| 2 | 内存盘 (E:/shm/) | 备份 + 崩溃快速加载 | ✅ kline_1m/1d_history |
| 3 | 定时同步线程 | 内存盘 → 磁盘备份 | ⏳ 待实现 |
| 4 | SQLite | 仓位、盈亏、账户数据，每次写入加锁 | ⏳ 待实现 |

## 7.11 定时同步线程 (层级3)
--------------------------------------------------------------------------------
[ ] 创建定时同步模块 sync_thread.rs
[ ] 实现定时器线程，定时将内存盘数据同步到磁盘
[ ] 同步策略：每5分钟或达到一定数据量时触发
[ ] 同步内容：
    - [ ] kline_1m_history/*.json → 磁盘备份目录
    - [ ] kline_1d_history/*.json → 磁盘备份目录
[ ] 实现优雅退出（收到信号时完成当前同步再退出）

## 7.12 SQLite 账户数据持久化 (层级4)
--------------------------------------------------------------------------------
[ ] 创建 persistence 模块 (如果不存在)
[ ] 实现 AccountSnapshot SQLite 表
    - [ ] 表结构：timestamp, total_equity, available, unrealized_pnl
    - [ ] 每次账户更新写入 SQLite（加锁）
[ ] 实现 PositionSnapshot SQLite 表
    - [ ] 表结构：timestamp, symbol, side, qty, entry_price, unrealized_pnl
    - [ ] 每次持仓变化写入 SQLite（加锁）
[ ] 实现 TradeRecord SQLite 表
    - [ ] 表结构：timestamp, symbol, side, qty, price, realized_pnl
    - [ ] 每次成交写入 SQLite（加锁）
[ ] 实现重启加载逻辑
    - [ ] 启动时从 SQLite 读取最新账户状态
    - [ ] 启动时从 kline_1m_history/ 读取 K线历史

## 7.13 程序启动加载流程验证
--------------------------------------------------------------------------------
[ ] 验证启动时加载 K线历史数据
[ ] 验证启动时恢复账户/持仓状态
[ ] 验证数据一致性（内存 vs 内存盘 vs SQLite）

================================================================================
