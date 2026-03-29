# P0 分钟级 Pin Bar 策略 - 完整逻辑分析

> 文档版本：v0.2.0
> 生成时间：2026-03-30
> 更新：v3.0 完全对齐 Python 原版
> 分析依据：signal.rs / status.rs / trader.rs 源码
> 回放数据：HOTUSDT 2025-10-09 ~ 2025-10-11（3笔交易日志）

---

## 第一步：现状描述

### 1. 触发条件（开仓）

策略使用 **7 个条件组合**，任意满足 >= 4 个即触发信号：

| # | 条件 | 阈值 | 判断内容 |
|---|------|------|---------|
| cond1 | zscore | abs(zscore) > 2 | 14周期价格偏离标准差程度 |
| cond2 | tr_ratio | > 1 | 60min TR / 5h TR，比值越大波动越强 |
| cond3 | pos_norm | 极端值 (>80 或 <20) | 价格在最近60根K线中的相对位置 |
| cond4 | acc_percentile | > 90 | 1小时加速度百分位 |
| cond5 | pine_bg_color | 纯色（非混色） | Pin Bar 背景颜色为纯红或纯绿 |
| cond6 | pine_bar_color | 纯色 | Pin Bar 本身颜色为纯红或纯绿 |
| cond7 | pdh | 价格偏离 != 0 | 水平位置在极端价位（100或0） |

**开仓三要素（必须同时满足）：**

```
tr_base_60min > 15%   当前60根K线平均TR占价格的15%以上（波动性门槛）
AND price_deviation != 0   价格不在中性位（有方向偏离）
AND 条件满足数 >= 4   7个条件中至少满足4个
```

**快慢通道：** High 波动率通道使用标准参数，Medium 通道也可用，Low 通道保守不交易。

**High vs Medium 通道差异：** High 通道阈值更高（>0.15），Medium 通道阈值更低（>0.05），Low 通道完全禁用。

---

### 2. 仓位管理

**三层加仓结构（状态机驱动）：**

| 层级 | 状态转移 | 触发条件 | 数量 |
|------|---------|---------|------|
| 第0层 | Initial → FirstOpen | 满足开仓三要素 | 0.05 (5%) |
| 第1层 | LongFirstOpen/ShortFirstOpen → DoubleAdd | 持有多头时再次出现同向信号 | 0.05 x 1.5 = 0.075 |
| 日线层 | DoubleAdd → DayAllow | 需要日线方向确认 + pin >= 5 | 未实现 |

**最大仓位：** 0.15 (15%)，由 `max_position_qty = 0.15` 硬限制。

**初始开仓比例：** 0.05 (5%)，由 `base_open_qty = 0.05` 决定。

**加仓倍数：** 1.5，即第1层比第0层多 50%。

**加仓触发：** 同向 Pin Bar 信号再次出现（`signal.long_entry` 在持有多头时）。注意：代码中 `LongFirstOpen` 状态下收到 `signal.long_entry` 触发 DoubleAdd，而非等待反向信号。

---

### 3. 平仓条件

**信号驱动平仓（需同时满足）：**

```
pin_conditions >= 4   7个条件至少满足4个
AND pos_norm_60 > 80 (多头)   价格位置达到上极端
   或 pos_norm_60 < 20 (空头)  价格位置达到下极端
```

**极端偏离平仓（单条件，兜底）：**

```
|price_deviation_pct| > 5%   从开仓价偏离超过5%，无条件触发平仓
```

**对冲（Hedge）模式入口：**

```
tr_base_60min < 15%   波动率降到门槛以下
AND price_deviation != 0
AND 6个条件 >= 4（只缺一个条件）
→ 反向开仓锁定收益
```

**对冲退出：** `exit_high_volatility` 信号触发 `HedgeEnter → PosLocked`。

**注意：** 平仓时 `full_close = true`，一次性全部平掉，不是逐层平。

---

### 4. 风控规则

| 规则 | 实现位置 | 行为 |
|------|---------|------|
| 单品种最大仓位 | `max_position_qty = 0.15` | 超过则加仓数量计算为0，跳过 |
| 已有仓位禁止超限加仓 | 状态机 | `LongFirstOpen` 收到同向信号可 DoubleAdd，但无第2次 DoubleAdd 路径 |
| 极端偏离强制平仓 | `use_extreme_exit = |deviation| > 5%` | 偏离超5%即平仓，不管信号方向 |
| 反向信号才平仓 | `decide_action` 中的 is_exit 判断 | mock_main.rs 中实现：持有多头时只有空头信号才触发平仓 |
| 对冲超时退出 | `hedge_enter_time` 记录 | 超时后强制触发退出 |
| 账户信息校验 | `fetch_account_info()` 必须成功 | 无法获取账户信息时拒绝下单 |

**无固定止损/止盈：** v3.0 已修复，现在使用 Python 风格的固定阈值。

---

### 5. 当前参数值（v3.0 Python 对齐版）

```
波动率分层阈值:
  High   > 0.15 (15%)
  Medium > 0.05 (5%)
  Low    <= 0.05

开仓三要素:
  tr_base门槛:      > 15%
  价格偏离:         != 0 (非中性)
  条件数:           >= 4 / 7

仓位参数:
  base_open_qty:    0.05 (5%)
  add_multiplier:    1.5  (第1层 = 0.05 x 1.5 = 0.075)
  max_position_qty: 0.15 (15%)

平仓参数 (v3.0 Python 对齐):
  盈利平仓:         > 1% (entry * 1.01)
  止损平仓:         < 1% (entry * 0.99)
  信号平仓:         signal.long_exit / signal.short_exit
  full_close:       true (全量平仓)

对冲参数 (v3.0 Python 对齐):
  多头对冲:         price < entry * 0.98 (下跌2%) 或 < 0.90 (硬阈值10%)
  空头对冲:         price > entry * 1.02 (上涨2%) 或 > 1.10 (硬阈值10%)

加仓参数 (v3.0 Python 对齐):
  多头加仓:         signal.long_entry AND price > entry * 1.02 (上涨2%)
  多头加仓硬阈值:   price > entry * 1.08 (上涨8%)
  空头加仓:         signal.short_entry AND price < entry * 0.98 (下跌2%)
  空头加仓硬阈值:   price < entry * 0.92 (下跌8%)

执行配置:
  interval_ms:      100 (100毫秒)
  lot_size:         0.001 (最小下单单位)

状态机11状态:
  Initial / LongInitial / ShortInitial
  LongFirstOpen / LongDoubleAdd / LongDayAllow
  ShortFirstOpen / ShortDoubleAdd / ShortDayAllow
  HedgeEnter / PosLocked
```

---

## 第二步：问题诊断

### v3.0 已修复的问题

以下问题已在 Rust v3.0 中修复：

| 问题 | 原状态 | v3.0 修复 |
|------|--------|-----------|
| 无固定止损/止盈 | 全依赖信号驱动 | 添加 1% 固定止盈止损 |
| 加仓无价格条件 | 任意同向信号触发 | 需价格变动 2% 才允许加仓 |
| 对冲阈值错误 | 15% 波动率判断 | 2% 价格偏离判断 |
| 止损线错误 | 5% 极端偏离 | 1% 固定止损 |

---

### 问题 A：加仓逻辑与用户预期可能不符（历史问题，已修复）

**代码实际行为：** 在 `LongFirstOpen` 状态下，再次出现 `signal.long_entry` 信号会触发 DoubleAdd（同向加仓），而非等待反向信号平仓。

**用户预期：** 如果期望的是"反向信号才平仓，同向不再加仓"，则当前 DoubleAdd 路径会导致：
- 在趋势行情中持续加仓直到 max_position
- 无法区分"趋势中继加仓"和"趋势结束信号"

**诊断：** 这不是 bug，是设计选择，但与回放中"PositionAlreadyOpen 拒绝同向信号"的逻辑存在矛盾。回放用的是 mock_main.rs 中的简化版策略，而 trader.rs 中有 DoubleAdd 路径。

---

### 问题 B：无固定止损/止盈（历史问题，已修复）

**原问题：** `stop_loss_price` 和 `take_profit_price` 全为 `None`，完全依赖信号退出。

**v3.0 修复：** 添加 Python 风格的固定止盈止损（1%）。

**注意：** 止盈止损仍通过信号驱动（`full_close = true`），而非交易所止损单。

---

### 问题 C：Martin 网格参数（1-2-4-8）不存在

**实际情况：** 代码中完全没有 Martin 网格（Martingale 1-2-4-8-16）相关实现。

`add_multiplier = 1.5` 只是一个固定乘数，不是 Martin 序列。

**用户提到的"网格参数"在当前代码中不存在。** 如果需要实现，需要明确：
1. 是否真的要用 Martin 网格（风险极高）
2. 还是用固定比例递增（如 1x-1.5x-2x-2.5x-3x）

---

### 问题 D：DayAllow / PosLocked 状态为死代码

**问题：** `PinStatus::DayAllow` 和 `PinStatus::PosLocked` 在 `decide_action` 和 `decide_action_wal` 中均落在 `_ => {}` 分支，永远不会被触发。

**原因：** `DayAllow` 的进入条件（从 DoubleAdd）在代码中没有实现路径，`PosLocked` 只能通过 `HedgeEnter → exit_high_volatility` 触发，但 HedgeEnter 后没有进入 DoubleAdd 的路径。

---

### 问题 E：逐层平仓 vs 全量平仓混淆

**代码行为：** 平仓时 `full_close = true`，一次性全部平仓。

**模糊地带：** 当有多层仓位（FirstOpen + DoubleAdd）时，是否应该先平 DoubleAdd 层，保留 FirstOpen 层？还是必须全部平？当前实现是全部平。

**回放数据佐证：** 3笔交易全部从 Initial → 直接 Close，没有 DoubleAdd 层出现。

---

## 第三步：理想状态讨论

### 讨论点 1：加仓策略选择

**方案 A（当前代码）：** 同向信号触发 DoubleAdd，仓位可累加至 0.15

**方案 B（反向平仓）：** 持有多头时，只有空头信号才平仓，不允许 DoubleAdd

**方案 C（时间/价格间隔加仓）：** 同向信号出现后，需等待 N 分钟或价格变动 X% 才允许 DoubleAdd

**推荐：** 方案 C，添加 interval_ms（已有 100ms）限制加仓频率，防止信号过密时连续加仓。

---

### 讨论点 2：止损/止盈机制

**方案 A：** 固定止损 + 固定止盈（如 +/- 2% / +/- 5%），由策略层构造 `StrategySignal` 时填入 `stop_loss_price` 和 `take_profit_price`

**方案 B：** 追踪止损（Trailing Stop），随价格有利方向移动止损位

**推荐：** 方案 A + 方案 B 组合，固定止损作为硬底线，信号驱动退出作为主动止盈。

---

### 讨论点 3：Martin 网格实现

**是否真的需要 Martin 网格？**

Martin 网格（亏损后翻倍加仓）在 HOTUSDT 这类高波动小币种中风险极高：
- HOTUSDT 在回放期间波动率 432%（tr_base_60min: 432.36%）
- 一次错误方向加仓可能导致资金快速耗尽

**替代方案：** 使用固定比例递增（Fibonacci 或等差数列：0.05 -> 0.075 -> 0.10 -> 0.125 -> 0.15），而非翻倍。

---

### 讨论点 4：平仓粒度

**全量平仓 vs 逐层平仓**

当前：full_close = true

**可选：**
- 逐层平：先平 DoubleAdd（0.075），保留 FirstOpen（0.05）观望
- 全量平：信号出现立即全部平仓

**推荐：** 全量平仓更简单，减少"留尾巴"导致的状态复杂性。

---

## 待确认事项

用户需确认以下选择后，再生成修改后的测试版本：

```
[ ] 加仓策略：方案A / 方案B / 方案C
[ ] 止损止盈：固定止损 / 追踪止损 / 组合方案
[ ] 网格参数：需要实现 / 不需要实现（固定递增）
[ ] 平仓粒度：全量平 / 逐层平
```

---

---

## v3.0 Python 对齐修改记录

### 修改的文件
- `crates/d_checktable/src/h_15m/trader.rs`
- `crates/d_checktable/src/h_15m/mod.rs`

### 新增配置：`ThresholdConfig`

从 Python 原版 `pin_main.py` 1:1 移植的阈值配置：

```rust
pub struct ThresholdConfig {
    // 盈利平仓：1%
    profit_threshold: dec!(0.01),
    // 多头对冲：下跌2%
    price_down_threshold: dec!(0.98),
    // 空头对冲：上涨2%
    price_up_threshold: dec!(1.02),
    // 多头对冲硬阈值：下跌10%
    price_down_hard_threshold: dec!(0.90),
    // 空头对冲硬阈值：上涨10%
    price_up_hard_threshold: dec!(1.10),
    // 多头加仓：上涨2%
    long_add_threshold: dec!(1.02),
    // 多头加仓硬阈值：上涨8%
    long_add_hard_threshold: dec!(1.08),
    // 空头加仓：下跌2%
    short_add_threshold: dec!(0.98),
    // 空头加仓硬阈值：下跌8%
    short_add_hard_threshold: dec!(0.92),
    // 止损线：亏损1%
    stop_loss_threshold: dec!(0.99),
}
```

### 修改的决策逻辑

#### 平仓逻辑（已对齐 Python）

| Python 原版 | Rust v3.0 实现 |
|------------|----------------|
| 盈利平仓：`close > entry * 1.01` | `profit_take_price_long = entry * 1.01` |
| 止损平仓：`close < entry * 0.99` | `stop_loss_price_long = entry * 0.99` |
| 空头盈利平仓：`close < entry * 0.99` | `profit_take_price_short = entry * 0.99` |
| 空头止损平仓：`close > entry * 1.01` | `stop_loss_price_short = entry * 1.01` |

#### 加仓逻辑（已对齐 Python）

| Python 原版 | Rust v3.0 实现 |
|------------|----------------|
| 多头加仓：`signal.long_entry AND close > entry * 1.02` | `signal.long_entry && price > entry * 1.02` |
| 多头加仓硬阈值：`close > entry * 1.08` | `price > entry * 1.08` |
| 空头加仓：`signal.short_entry AND close < entry * 0.98` | `signal.short_entry && price < entry * 0.98` |
| 空头加仓硬阈值：`close < entry * 0.92` | `price < entry * 0.92` |

#### 对冲逻辑（已对齐 Python）

| Python 原版 | Rust v3.0 实现 |
|------------|----------------|
| 多头对冲：`signal.long_hedge AND close < entry * 0.98` | `signal.long_hedge && price < entry * 0.98` |
| 多头对冲硬阈值：`close < entry * 0.90` | `price < entry * 0.90` |
| 空头对冲：`signal.short_hedge AND close > entry * 1.02` | `signal.short_hedge && price > entry * 1.02` |
| 空头对冲硬阈值：`close > entry * 1.10` | `price > entry * 1.10` |

### 删除的旧逻辑

- ~~5% 极端偏离平仓~~（改为 Python 风格的 1% 固定止损）

---

*本文档由 Claude Code 基于 signal.rs / status.rs / trader.rs 源码分析生成*
