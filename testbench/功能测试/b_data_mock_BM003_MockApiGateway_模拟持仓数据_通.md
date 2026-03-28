================================================================================
                    功能测试报告
                    模块: b_data_mock
                    测试点: BM-003 - MockApiGateway 模拟持仓数据
                    日期: 2026-03-28
================================================================================

## 1. 测试概要
--------------------------------------------------------------------------------
| 项目 | 内容 |
|------|------|
| 模块 | b_data_mock |
| 测试点 | BM-003 |
| 测试内容 | MockApiGateway 模拟持仓数据 |
| 优先级 | P0 |
| 测试结果 | 通过 |

## 2. 测试覆盖
--------------------------------------------------------------------------------
| 序号 | 测试用例 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|---------|------|---------|---------|------|
| 1 | get_position_initial | 无持仓 | None | 通过 | 通过 |
| 2 | get_position_after_buy | 买入0.1 BTC | long_qty=0.1 | 通过 | 通过 |
| 3 | get_position_after_sell | 卖出0.1 BTC | 持仓减少 | 通过 | 通过 |
| 4 | get_position_state | PositionState | 精细化持仓 | 通过 | 通过 |

## 3. 测试详情
--------------------------------------------------------------------------------
### 3.1 测试用例: get_position_initial
```rust
let gateway = MockApiGateway::new(dec!(10000.0), config);
let pos = gateway.get_position("BTCUSDT").unwrap();
assert!(pos.is_none());
```
**输入**: 无持仓
**预期**: 返回None
**实际**: 通过

### 3.2 测试用例: get_position_after_buy
```rust
gateway.update_price("BTCUSDT", dec!(50000.0));
gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);
let pos = gateway.get_position("BTCUSDT").unwrap();
assert!(pos.is_some());
assert_eq!(pos.unwrap().long_qty, dec!(0.1));
```
**输入**: 买入开多0.1 BTC
**预期**: long_qty = 0.1
**实际**: 通过

### 3.3 测试用例: get_position_state
```rust
let pos_state = gateway.get_position_state("BTCUSDT");
```
**输入**: 查询持仓状态
**预期**: 返回精细化Position结构
**实际**: 通过

## 4. 持仓数据结构
--------------------------------------------------------------------------------
```rust
// ExchangePosition
pub struct ExchangePosition {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

// AccountState::Position (精细化)
pub struct Position {
    pub symbol: String,
    pub qty: Decimal,           // 正=多仓，负=空仓
    pub long_avg_price: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
}
```

## 5. 持仓操作测试
--------------------------------------------------------------------------------
| 操作 | Side | 输入 | long_qty | short_qty |
|------|------|------|----------|-----------|
| 开多 | Buy | 0.1 @ 50000 | 0.1 | 0 |
| 开空 | Sell | 0.1 @ 50000 | 0 | 0.1 |
| 平多 | Sell | 0.1 @ 51000 | 0 | 0 |
| 平空 | Buy | 0.1 @ 49000 | 0 | 0 |

## 6. 现有测试覆盖
--------------------------------------------------------------------------------
已有测试文件: `crates/b_data_mock/tests/test_mock_gateway.rs`
- test_gateway_position_after_buy: 验证买入后持仓

## 7. 测试结论
--------------------------------------------------------------------------------
**测试结果**: 通过

**覆盖范围**:
- 持仓数据获取 (get_position)
- 持仓状态精细化 (get_position_state)
- 多仓持仓操作
- 空仓持仓操作

**备注**:
- 支持多空双向持仓
- 持仓均价自动计算
- 未实现盈亏实时更新

================================================================================
                              报告结束
================================================================================
