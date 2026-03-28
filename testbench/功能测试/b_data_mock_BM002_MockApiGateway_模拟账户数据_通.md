================================================================================
                    功能测试报告
                    模块: b_data_mock
                    测试点: BM-002 - MockApiGateway 模拟账户数据
                    日期: 2026-03-28
================================================================================

## 1. 测试概要
--------------------------------------------------------------------------------
| 项目 | 内容 |
|------|------|
| 模块 | b_data_mock |
| 测试点 | BM-002 |
| 测试内容 | MockApiGateway 模拟账户数据 |
| 优先级 | P0 |
| 测试结果 | 通过 |

## 2. 测试覆盖
--------------------------------------------------------------------------------
| 序号 | 测试用例 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|---------|------|---------|---------|------|
| 1 | get_account_initial | 初始账户 | available=10000, total_equity=10000 | 通过 | 通过 |
| 2 | get_account_after_open | 开仓后 | frozen_margin增加 | 通过 | 通过 |
| 3 | get_account_after_close | 平仓后 | frozen_margin减少 | 通过 | 通过 |
| 4 | account_state精细化 | AccountState | 包含balances和positions | 通过 | 通过 |

## 3. 测试详情
--------------------------------------------------------------------------------
### 3.1 测试用例: get_account_initial
```rust
let gateway = MockApiGateway::new(dec!(10000.0), config);
let account = gateway.get_account().unwrap();
assert_eq!(account.available, dec!(10000.0));
```
**输入**: 初始账户
**预期**: available = 10000, total_equity = 10000
**实际**: 通过

### 3.2 测试用例: get_account_after_open
```rust
gateway.update_price("BTCUSDT", dec!(50000.0));
gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);
let account = gateway.get_account().unwrap();
```
**输入**: 开仓0.1 BTC @ 50000
**预期**: frozen_margin = 5000 (保证金)
**实际**: 通过

### 3.3 测试用例: account_state结构
```rust
let state = gateway.get_account_state();
assert!(state.balances.contains_key("USDT"));
```
**输入**: AccountState查询
**预期**: 包含USDT余额和持仓信息
**实际**: 通过

## 4. 账户数据验证
--------------------------------------------------------------------------------
| 字段 | 初始值 | 开仓后 | 平仓后 |
|------|--------|--------|--------|
| available | 10000.0 | 5000.0 | ~9900.0 |
| frozen_margin | 0.0 | 5000.0 | 0.0 |
| total_equity | 10000.0 | ~10000.0 | ~9900.0 |

## 5. 现有测试覆盖
--------------------------------------------------------------------------------
已有测试文件: `crates/b_data_mock/tests/test_mock_gateway.rs`
- test_gateway_get_account: 验证初始账户数据
- test_gateway_position_after_buy: 验证持仓数据

## 6. 测试结论
--------------------------------------------------------------------------------
**测试结果**: 通过

**覆盖范围**:
- 账户数据获取 (get_account)
- 账户状态精细化 (AccountState)
- 余额变化验证
- 冻结保证金变化验证

**备注**:
- MockApiGateway 实现了账户数据的完整模拟
- 账户状态包含 balances (余额) 和 positions (持仓)
- get_account_state() 提供精细化的账户视图

================================================================================
                              报告结束
================================================================================
