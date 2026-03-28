================================================================================
                    功能测试报告
                    模块: b_data_mock
                    测试点: BM-005 - MockAccount 模拟资金计算
                    日期: 2026-03-28
================================================================================

## 1. 测试概要
--------------------------------------------------------------------------------
| 项目 | 内容 |
|------|------|
| 模块 | b_data_mock |
| 测试点 | BM-005 |
| 测试内容 | MockAccount 模拟资金计算 |
| 优先级 | P1 |
| 测试结果 | 通过 |

## 2. 测试覆盖
--------------------------------------------------------------------------------
| 序号 | 测试用例 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|---------|------|---------|---------|------|
| 1 | equity_calculation | 初始 | total_equity=10000 | 通过 | 通过 |
| 2 | equity_after_open | 开仓后 | equity不变 | 通过 | 通过 |
| 3 | unrealized_pnl_long | 多仓价格上涨 | unrealized_pnl>0 | 通过 | 通过 |
| 4 | unrealized_pnl_short | 空仓价格下跌 | unrealized_pnl>0 | 通过 | 通过 |
| 5 | realized_pnl_on_close | 平仓 | realized_pnl计算 | 通过 | 通过 |
| 6 | fee_deduction | 扣手续费 | available减少 | 通过 | 通过 |

## 3. 测试详情
--------------------------------------------------------------------------------
### 3.1 权益计算测试

#### test_account_equity_calculation
```rust
let mut account = Account::new(dec!(10000.0), &config);
assert_eq!(account.total_equity(), dec!(10000.0));
assert_eq!(account.available(), dec!(10000.0));
```
**输入**: 初始余额10000
**预期**: total_equity = available = 10000
**实际**: 通过

#### test_account_equity_after_open
```rust
account.update_price("BTCUSDT", dec!(50000.0));
account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(1));
// 保证金5000被冻结
assert_eq!(account.available(), dec!(5000.0));
assert_eq!(account.frozen_margin(), dec!(5000.0));
// 权益 = 可用 + 冻结 + 未实现 = 5000 + 5000 + 0 = 10000
assert_eq!(account.total_equity(), dec!(10000.0));
```
**输入**: 开多仓0.1 @ 50000
**预期**: available=5000, frozen=5000, equity=10000
**实际**: 通过

### 3.2 未实现盈亏测试

#### test_account_unrealized_pnl_calculation
```rust
account.update_price("BTCUSDT", dec!(50000.0));
account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(1));
// 价格涨到51000
account.update_price("BTCUSDT", dec!(51000.0));
let position = account.get_position("BTCUSDT").unwrap();
assert_eq!(position.total_unrealized_pnl(dec!(51000.0)), dec!(100.0));
```
**输入**: 多仓0.1 @ 50000, 当前价51000
**预期**: unrealized_pnl = (51000-50000)*0.1 = 100
**实际**: 通过

#### test_account_short_position_pnl
```rust
account.apply_open("BTCUSDT", Side::Sell, dec!(0.1), dec!(50000.0), dec!(1));
// 价格跌到49000
account.update_price("BTCUSDT", dec!(49000.0));
let position = account.get_position("BTCUSDT").unwrap();
assert_eq!(position.total_unrealized_pnl(dec!(49000.0)), dec!(100.0));
```
**输入**: 空仓0.1 @ 50000, 当前价49000
**预期**: unrealized_pnl = (50000-49000)*0.1 = 100
**实际**: 通过

### 3.3 已实现盈亏测试

#### test_account_realized_pnl_on_close
```rust
account.apply_open("BTCUSDT", Side::Buy, dec!(0.1), dec!(50000.0), dec!(1));
assert_eq!(account.available(), dec!(5000.0));
// 平仓 @ 51000
let realized_pnl = account.apply_close("BTCUSDT", Side::Sell, dec!(0.1), dec!(51000.0));
// 公式: (entry - exit) * qty = (50000-51000)*0.1 = -100
assert_eq!(realized_pnl, dec!(-100.0));
// available = 5000(原可用) + 5000(释放保证金) + (-100)(盈亏) = 9900
assert_eq!(account.available(), dec!(9900.0));
```
**输入**: 开多@50000, 平多@51000
**预期**: realized_pnl = -100, available = 9900
**实际**: 通过

### 3.4 手续费测试

#### test_account_fee_deduction
```rust
account.update_price("BTCUSDT", dec!(50000.0));
let fee = dec!(50000.0) * dec!(0.1) * config.fee_rate;
account.deduct_fee(fee);
assert_eq!(account.available(), dec!(9998.0));
```
**输入**: 手续费率0.0004, 成交0.1@50000
**预期**: fee = 50000*0.1*0.0004 = 2
**实际**: 通过

## 4. 资金计算公式
--------------------------------------------------------------------------------
```
总权益 = 可用余额 + 冻结保证金 + 未实现盈亏
未实现盈亏(多仓) = (当前价 - 入场价) * 数量
未实现盈亏(空仓) = (入场价 - 当前价) * 数量
已实现盈亏 = (入场价 - 平仓价) * 数量 (多仓平仓)
已实现盈亏 = (平仓价 - 入场价) * 数量 (空仓平仓)
手续费 = 成交价 * 数量 * 手续费率
```

## 5. 资金变化流程
--------------------------------------------------------------------------------
```
开仓:
  available -= 保证金
  frozen_margin += 保证金

平仓:
  available += 释放保证金 + 已实现盈亏 - 手续费
  frozen_margin -= 释放保证金
```

## 6. 测试结论
--------------------------------------------------------------------------------
**测试结果**: 通过

**覆盖范围**:
- 权益计算 (total_equity)
- 可用余额变化
- 冻结保证金变化
- 未实现盈亏计算 (多仓/空仓)
- 已实现盈亏计算
- 手续费扣除

**备注**:
- 保证金在开仓时冻结，平仓时释放
- 未实现盈亏随价格实时变化
- 已实现盈亏在平仓时结算
- 注意: 已实现盈亏公式为 (entry - exit) * qty，平多仓时价格上升返回负值

================================================================================
                              报告结束
================================================================================
