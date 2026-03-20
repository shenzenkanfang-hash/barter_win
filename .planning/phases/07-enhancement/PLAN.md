================================================================================
Phase 7: Enhancement 改进
================================================================================

## 阶段目标

完善流水线架构核心模块，实现设计文档中提到的风控两层、PnlManager、市场状态检测等关键功能。

## 待实现任务

### 7.1 风控两层完善
--------------------------------------------------------------------------------
目标: 实现 RiskReChecker 锁内复核

**RiskReChecker (锁内复核)**
- 位置: engine/src/risk_rechecker.rs (新文件)
- 功能:
  - 再次检查资金（防止并发修改）
  - 实时波动率确认
  - 与 RiskPreChecker 配合实现完整风控

**风控流程:**
1. RiskPreChecker (锁外预检) → 通过
2. GlobalLock.acquire() → 获取锁
3. RiskReChecker (锁内复核) → 再次确认
4. 下单执行
5. 释放锁

### 7.2 PnlManager 盈亏管理模块
--------------------------------------------------------------------------------
目标: 实现设计文档 17.3.8 描述的盈亏管理功能

**PnlManager**
- 位置: engine/src/pnl_manager.rs (新文件)
- 功能:
  - 计算已实现盈亏 (realized_pnl)
  - 计算未实现盈亏 (unrealized_pnl)
  - 累计盈利跟踪 (cumulative_profit)
  - 低波动/高波动品种互斥机制
  - rescue_low_volatility_symbols() 解救机制

**关键函数:**
- calculate_realized_pnl()
- calculate_unrealized_pnl()
- check_and_rescue_low_volatility()
- update_cumulative_profit()

### 7.3 MarketStatusDetector 市场状态检测
--------------------------------------------------------------------------------
目标: 实现市场状态检测 (PIN/RANGE/TREND/INVALID)

**MarketStatus**
- 位置: engine/src/market_status.rs (新文件)
- 功能:
  - PIN (插针状态): 极端波动检测
  - RANGE (震荡状态): 低波动、低动能
  - TREND (趋势状态): 有明确方向
  - INVALID (数据无效): 超时/异常

**检测器:**
- MarketStatusDetector: 主检测器，根据多指标判断状态
- 检测优先级: INVALID > PIN > RANGE > TREND

### 7.4 仓位互斥判断
--------------------------------------------------------------------------------
目标: 实现同品种同策略的持仓互斥逻辑

**PositionMutualExclusion**
- 位置: engine/src/position_exclusion.rs (新文件)
- 功能:
  - 同品种 + 同策略: LONG 和 SHORT 互斥
  - 同品种 + 不同策略: 不互斥
  - 跨品种: 可配置控制

### 7.5 日线指标周期支持
--------------------------------------------------------------------------------
目标: 添加长周期 EMA 支持

**实现:**
- 日线 EMA (100, 200): 用于趋势判断
- 日线 RSI (14): 标准 RSI
- 日线 PineColor: 用于判断颜色
- 日线 PricePosition: 价格位置

**修改:**
- indicator/ema.rs: 支持配置不同周期
- channel.rs: 添加日线 KLineSynthesizer 和指标

### 7.6 阈值常量模块
--------------------------------------------------------------------------------
目标: 集中管理所有阈值常量

**ThresholdConstants**
- 位置: engine/src/thresholds.rs (新文件)
- 内容:
  - PROFIT_THRESHOLD = 0.01 (1% 盈利平仓)
  - PRICE_DOWN_THRESHOLD = 0.98 (下跌 2% 对冲)
  - PRICE_UP_THRESHOLD = 1.02 (上涨 2% 对冲/加仓)
  - 波动率阈值 (已在 channel.rs 中)
  - 其他策略阈值

## 验收标准

1. RiskReChecker 实现完成，与 RiskPreChecker 配合形成完整风控
2. PnlManager 实现盈亏计算和累计盈利跟踪
3. MarketStatusDetector 实现四种市场状态检测
4. 仓位互斥判断逻辑正确实现
5. 日线指标周期支持添加
6. 所有新模块通过代码审查

## 文件清单

新增文件:
- engine/src/risk_rechecker.rs
- engine/src/pnl_manager.rs
- engine/src/market_status.rs
- engine/src/position_exclusion.rs
- engine/src/thresholds.rs

修改文件:
- engine/src/lib.rs (导出新模块)
- engine/src/channel.rs (添加日线支持)
- indicator/src/ema.rs (可能需要调整)
