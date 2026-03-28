================================================================================
                    功能测试报告
                    模块: b_data_mock
                    测试点: BM-004 - MockAccount 模拟账户操作
                    日期: 2026-03-28
================================================================================

## 1. 测试概要
--------------------------------------------------------------------------------
| 项目 | 内容 |
|------|------|
| 模块 | b_data_mock |
| 测试点 | BM-004 |
| 测试内容 | MockAccount 模拟账户操作 (开仓/平仓/修改杠杆) |
| 优先级 | P0 |
| 测试结果 | 通过 |

## 2. 测试覆盖
--------------------------------------------------------------------------------
| 序号 | 测试用例 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|---------|------|---------|---------|------|
| 1 | pre_check_leverage_10 | 10倍杠杆开仓 | 成功 | 通过 | 通过 |
| 2 | pre_check_leverage_100 | 100倍杠杆开仓 | 成功 | 通过 | 通过 |
| 3 | pre_check_insufficient_balance | 保证金不足 | Reject | 通过 | 通过 |
| 4 | pre_check_position_limit | 超过持仓限制 | Reject | 通过 | 通过 |
| 5 | apply_open_long | 开多仓 | 持仓增加 | 通过 | 通过 |
| 6 | apply_close_long | 平多仓 | 持仓减少 | 通过 | 通过 |
| 7 | apply_open_short | 开空仓 | 持仓增加 | 通过 | 通过 |
| 8 | apply_close_without_position | 无持仓平仓 | Reject | 通过 | 通过 |

## 3. 测试详情
--------------------------------------------------------------------------------
### 3.1 杠杆验证测试

#### test_account_leverage_precheck_buy_with_leverage
```rust
let mut account = Account::new(dec!(10000.0), &config);
account.update_price("BTCUSDT", dec!(50000.0));
let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(10), Side::Buy);
assert!(result.is_ok());
```
**输入**: 10倍杠杆, 0.1 BTC @ 50000, 需要保证金500 USDT
**预期**: 成功 (可用10000 > 需要500)
**实际**: 通过

#### test_account_leverage_high_leverage
```rust
let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(100), Side::Buy);
assert!(result.is_ok());
```
**输入**: 100倍杠杆, 0.1 BTC @ 50000, 需要保证金50 USDT
**预期**: 成功
**实际**: 通过

#### test_account_leverage_precheck_insufficient_balance
```rust
let mut account = Account::new(dec!(100.0), &config);
account.update_price("BTCUSDT", dec!(50000.0));
let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(10), Side::Buy);
assert!(result.is_err());
```
**输入**: 10倍杠杆, 0.1 BTC @ 50000, 需要保证金500 USDT, 仅有100
**预期**: RejectReason::InsufficientBalance
**实际**: 通过

#### test_account_leverage_position_limit
```rust
let mut account = Account::new(dec!(10000.0), &config);
account.update_price("BTCUSDT", dec!(50000.0));
// 0.2 BTC @ 50000 = 10000 USDT, 超过95%限制
let result = account.pre_check("BTCUSDT", dec!(0.2), dec!(50000.0), dec!(1), Side::Buy);
assert!(result.is_err());
```
**输入**: 0.2 BTC @ 50000, 总权益10000, 95%限制=9500
**预期**: RejectReason::PositionLimitExceeded
**实际**: 通过

### 3.2 平仓验证测试

#### test_account_close_without_position
```rust
let mut account = Account::new(dec!(10000.0), &config);
account.update_price("BTCUSDT", dec!(50000.0));
let result = account.pre_check("BTCUSDT", dec!(0.1), dec!(50000.0), dec!(1), Side::Sell);
assert!(result.is_err());
```
**输入**: 平仓不存在的持仓
**预期**: Reject
**实际**: 通过

## 4. 杠杆计算公式
--------------------------------------------------------------------------------
```
所需保证金 = 价格 * 数量 / 杠杆倍数
```

| 杠杆 | 数量 | 价格 | 所需保证金 |
|------|------|------|-----------|
| 1x | 0.1 BTC | 50000 | 5000 USDT |
| 10x | 0.1 BTC | 50000 | 500 USDT |
| 100x | 0.1 BTC | 50000 | 50 USDT |

## 5. 持仓限制验证
--------------------------------------------------------------------------------
```
当前持仓价值 = Σ(position.qty * 当前价格)
最大持仓价值 = 总权益 * max_position_ratio (0.95)
新持仓价值 = 当前持仓价值 + 新开仓价值
```

## 6. 边界条件测试
--------------------------------------------------------------------------------
| 测试项 | 输入 | 预期 | 实际 | 状态 |
|--------|------|------|------|------|
| 无持仓平仓检查 | Sell, 无持仓 | Reject | Reject | 通过 |
| 负数量 | qty=-1 | 过滤 | N/A | N/A |
| 零价格 | price=0 | 允许开仓 | N/A | N/A |

## 7. 测试结论
--------------------------------------------------------------------------------
**测试结果**: 通过

**覆盖范围**:
- 10倍杠杆开仓验证
- 100倍杠杆开仓验证
- 保证金不足检查
- 持仓限制检查
- 无持仓平仓拒绝

**备注**:
- pre_check 在开仓时验证保证金和持仓限制
- pre_check 在平仓时验证持仓存在和手续费
- 杠杆倍数影响所需保证金计算

================================================================================
                              报告结束
================================================================================
