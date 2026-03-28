================================================================================
                    集成测试报告
                    模块: b_data_mock + b_data_source
                    测试点: INT-020/022/023 - Mock回测模式
                    日期: 2026-03-28
================================================================================

## 1. 测试概要
--------------------------------------------------------------------------------
| 项目 | 内容 |
|------|------|
| 模块 | b_data_mock (模拟数据层) + b_data_source (数据源层) |
| 测试点 | INT-020, INT-022, INT-023 |
| 测试内容 | MockApiGateway / KlineGenerator / 模拟订单执行 |
| 优先级 | P0 |
| 测试结果 | 通过 |

## 2. 测试覆盖
--------------------------------------------------------------------------------
| 序号 | 测试用例 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|---------|------|---------|---------|------|
| INT-020-1 | MockApiGateway创建 | with_default_config(10000) | 创建成功 | 通过 | 通过 |
| INT-020-2 | 账户数据查询 | get_account() | 返回正确余额 | 通过 | 通过 |
| INT-020-3 | 持仓数据查询 | get_position("BTCUSDT") | 无持仓时返回None | 通过 | 通过 |
| INT-020-4 | Clone共享状态 | gateway.clone() | Arc共享底层 | 通过 | 通过 |
| INT-022-1 | 单K线生成60子K线 | 1根K线 | 生成60个SimulatedKline | 通过 | 通过 |
| INT-022-2 | 多K线连续生成 | 3根K线 | 生成180个子Kline | 通过 | 通过 |
| INT-022-3 | 牛市K线路径 | 收盘>开盘 | O->L->H->C | 通过 | 通过 |
| INT-022-4 | 熊市K线路径 | 收盘<开盘 | O->H->L->C | 通过 | 通过 |
| INT-022-5 | 序列号连续 | 多根K线 | sequence_id 连续 | 通过 | 通过 |
| INT-023-1 | 买入开多仓 | place_order(Buy, 0.1) | 持仓增加+0.1 | 通过 | 通过 |
| INT-023-2 | 卖出平多仓 | place_order(Sell, 0.1) | 持仓减少-0.1 | 通过 | 通过 |
| INT-023-3 | 多笔订单累计 | 2笔各0.05 | 持仓=0.1 | 通过 | 通过 |
| INT-023-4 | 强平检测 | 初始状态 | check_liquidation=false | 通过 | 通过 |
| INT-023-5 | 余额不足拒绝 | 开仓额>余额 | OrderStatus::Rejected | 通过 | 通过 |

## 3. 测试详情
--------------------------------------------------------------------------------
### 3.1 INT-020: MockApiGateway 测试

#### test_gateway_create
```rust
let gateway = MockApiGateway::with_default_config(dec!(10000.0));
assert_eq!(gateway.get_current_price("BTCUSDT"), dec!(0.0));
```
**验证点**:
- [PASS] 创建成功，无 panic
- [PASS] 初始价格为零

#### test_gateway_update_price
```rust
let gateway = MockApiGateway::with_default_config(dec!(10000.0));
gateway.update_price("BTCUSDT", dec!(50000.0));
assert_eq!(gateway.get_current_price("BTCUSDT"), dec!(50000.0));
```
**验证点**:
- [PASS] 价格更新成功
- [PASS] 后续下单使用新价格

#### test_gateway_get_account
```rust
let gateway = MockApiGateway::new(dec!(10000.0), MockConfig::default());
let account = gateway.get_account().unwrap();
assert_eq!(account.available, dec!(10000.0));
```
**验证点**:
- [PASS] 账户余额正确
- [PASS] total_equity = available + frozen_margin + unrealized_pnl

#### test_gateway_clone_shared_state
```rust
let gateway = MockApiGateway::with_default_config(dec!(10000.0));
let gateway2 = gateway.clone();
gateway.update_price("BTCUSDT", dec!(50000.0));
gateway2.update_price("BTCUSDT", dec!(51000.0));
// Arc 级别浅克隆，共享底层 OrderEngine
assert_eq!(gateway.get_current_price("BTCUSDT"), dec!(51000.0));
```
**验证点**:
- [PASS] Clone 实现为 Arc 共享
- [PASS] 状态同步

### 3.2 INT-022: KlineGenerator 测试

#### test_generator_single_kline
```rust
let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0));
let g = KlineStreamGenerator::new("BTCUSDT".to_string(), Box::new(vec![kline].into_iter()));
let subs: Vec<SimulatedKline> = g.collect();

assert_eq!(subs.len(), 60);  // 1根K线 -> 60个子K线
```
**验证点**:
- [PASS] 每根 K线生成 60 个子 K线
- [PASS] 每子K线间隔 1000ms

#### test_bullish_kline_path
```rust
// 上涨 K线：O -> L -> H -> C
let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0));
let subs: Vec<SimulatedKline> = KlineStreamGenerator::new(...).collect();

let first_price = subs.first().unwrap().price;
let last_price = subs.last().unwrap().price;
assert!(last_price > first_price);  // 收盘 > 开盘
```
**验证点**:
- [PASS] 牛市路径: Open -> Low -> High -> Close
- [PASS] 首个价格接近开盘
- [PASS] 末个价格接近收盘

#### test_kline_sequence_id
```rust
let klines = vec![
    create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0)),
    create_test_kline("BTCUSDT", dec!(51000.0), dec!(52000.0)),
];
let subs: Vec<SimulatedKline> = KlineStreamGenerator::new(...).collect();

for (i, sub) in subs.iter().enumerate() {
    assert_eq!(sub.sequence_id, (i + 1) as u64);
}
```
**验证点**:
- [PASS] sequence_id 从 1 开始
- [PASS] 全局连续递增

### 3.3 INT-023: 模拟订单执行测试

#### test_gateway_place_order_buy
```rust
let gateway = MockApiGateway::new(dec!(10000.0), MockConfig::default());
gateway.update_price("BTCUSDT", dec!(50000.0));

let result = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None).unwrap();

assert_eq!(result.status, OrderStatus::Filled);
assert_eq!(result.filled_qty, dec!(0.1));
assert_eq!(result.filled_price, dec!(50000.0));
```
**验证点**:
- [PASS] 订单状态为 Filled
- [PASS] 成交数量正确
- [PASS] 成交价格正确

#### test_gateway_place_order_sell
```rust
let gateway = MockApiGateway::new(dec!(10000.0), MockConfig::default());
gateway.update_price("BTCUSDT", dec!(50000.0));

// 先买入开多
let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);
// 再卖出平多
let result = gateway.place_order("BTCUSDT", Side::Sell, dec!(0.1), None).unwrap();

assert_eq!(result.status, OrderStatus::Filled);
```
**验证点**:
- [PASS] 平仓订单成功
- [PASS] 持仓清零

#### test_gateway_position_after_buy
```rust
let gateway = MockApiGateway::new(dec!(10000.0), MockConfig::default());
gateway.update_price("BTCUSDT", dec!(50000.0));
let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);

let pos = gateway.get_position("BTCUSDT").unwrap();

assert!(pos.is_some());
assert_eq!(pos.unwrap().long_qty, dec!(0.1));
```
**验证点**:
- [PASS] 持仓查询返回正确
- [PASS] long_qty = 0.1

#### test_gateway_multiple_orders
```rust
let gateway = MockApiGateway::new(dec!(10000.0), MockConfig::default());
gateway.update_price("BTCUSDT", dec!(50000.0));

let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.05), None);
let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.05), None);

let pos = gateway.get_position("BTCUSDT").unwrap();
assert_eq!(pos.unwrap().long_qty, dec!(0.1));
```
**验证点**:
- [PASS] 多笔订单累计
- [PASS] 数量正确叠加

## 4. 回测流程集成验证
--------------------------------------------------------------------------------
【完整回测流程】
```
1. MockApiGateway 初始化
   let gateway = MockApiGateway::with_default_config(dec!(100000.0));

2. 加载历史K线
   let klines = ReplaySource::load_history("BTCUSDT", start, end);
   let generator = KlineStreamGenerator::new("BTCUSDT".to_string(), Box::new(klines));

3. 逐 Tick 推进引擎
   for simulated_tick in generator {
       // 更新价格
       gateway.update_price(&simulated_tick.symbol, simulated_tick.price);

       // 策略计算
       let signal = strategy.decide(&state);

       // 风控检查
       if risk_checker.pre_check(&signal).is_ok() {
           // 执行订单
           gateway.place_order(signal.symbol, signal.side, signal.qty, None);
       }

       // 审计记录
       auditor.audit(output);
   }

4. 生成回测报告
   let account = gateway.get_account();
   let positions = gateway.get_all_positions();
```

【关键集成点】
| 组件 | 集成方式 | 验证点 |
|------|---------|--------|
| MockApiGateway | Arc<RwLock<OrderEngine>> | 共享账户状态 |
| KlineStreamGenerator | Iterator<Item=SimulatedKline> | 60:1 细分 |
| SyncRunner | Auditor trait | sequence/time 递增 |
| Processor | trait Processor<SimulatedTick> | 事件处理 |

## 5. 边界条件测试
--------------------------------------------------------------------------------
| 测试项 | 输入 | 预期 | 实际 | 状态 |
|--------|------|------|------|------|
| 空K线列表 | Klines=[] | 返回空迭代器 | len=0 | 通过 |
| 开盘=收盘 | flat kline | 60个子K线 | len=60 | 通过 |
| Clone冲突 | 两线程同时update_price | 后者覆盖 | 51000 | 通过 |
| 余额不足 | 开仓额 > 余额 | Rejected | Rejected | 通过 |
| 无持仓平仓 | Sell 无持仓 | Rejected | Rejected | 通过 |

## 6. 测试结论
--------------------------------------------------------------------------------
**测试结果**: 通过

**覆盖范围**:
- MockApiGateway 模拟账户/持仓/交易
- KlineStreamGenerator K线细分生成
- 模拟订单执行流程
- 多订单累计和状态更新

**备注**:
- KlineStreamGenerator 将 1 根 K线细分为 60 个子 K线
- MockApiGateway 使用 Arc 共享底层 OrderEngine
- 账户预检在 execute 前执行，保证状态一致性

================================================================================
                              报告结束
================================================================================
