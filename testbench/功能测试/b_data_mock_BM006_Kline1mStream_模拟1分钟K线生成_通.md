================================================================================
                    功能测试报告
                    模块: b_data_mock
                    测试点: BM-006 - Kline1mStream (mock) 模拟1分钟K线生成
                    日期: 2026-03-28
================================================================================

## 1. 测试概要
--------------------------------------------------------------------------------
| 项目 | 内容 |
|------|------|
| 模块 | b_data_mock |
| 测试点 | BM-006 |
| 测试内容 | Kline1mStream (mock) 模拟1分钟K线生成 |
| 优先级 | P0 |
| 测试结果 | 通过 |

## 2. 测试覆盖
--------------------------------------------------------------------------------
| 序号 | 测试用例 | 输入 | 预期结果 | 实际结果 | 状态 |
|------|---------|------|---------|---------|------|
| 1 | next_message | 单根K线 | 返回JSON消息 | 通过 | 通过 |
| 2 | generates_60_subs | 单根K线 | 生成60个子K线 | 通过 | 通过 |
| 3 | multi_kline | 2根K线 | 生成120个子K线 | 通过 | 通过 |
| 4 | empty_klines | 空K线列表 | 返回None | 通过 | 通过 |
| 5 | message_content | JSON消息 | 包含交易对/周期 | 通过 | 通过 |

## 3. 测试详情
--------------------------------------------------------------------------------
### 3.1 测试用例: next_message
```rust
let kline = create_test_kline("BTCUSDT", dec!(50000.0), dec!(51000.0), ...);
let klines = vec![kline];
let boxed = Box::new(klines.into_iter());
let mut stream = Kline1mStream::from_klines("BTCUSDT".to_string(), boxed);
let msg = stream.next_message();
assert!(msg.is_some());
let json_str = msg.unwrap();
assert!(json_str.contains("BTCUSDT"));
assert!(json_str.contains("1m"));
```
**输入**: 单根K线 (O:50000, C:51000, H:51500, L:49500)
**预期**: 返回包含BTCUSDT和1m的JSON消息
**实际**: 通过

### 3.2 测试用例: generates_60_subs
```rust
let kline = create_test_kline(...);
let klines = vec![kline];
let boxed = Box::new(klines.into_iter());
let mut stream = Kline1mStream::from_klines("BTCUSDT".to_string(), boxed);
let mut count = 0;
while stream.next_message().is_some() {
    count += 1;
}
assert_eq!(count, 60);
```
**输入**: 单根1分钟K线
**预期**: 生成60个子K线
**实际**: 通过 (count = 60)

### 3.3 测试用例: multi_kline
```rust
let klines = vec![
    create_test_kline("ETHUSDT", 50000, 51000, 51500, 49500),
    create_test_kline("ETHUSDT", 51000, 52000, 52500, 50500),
];
let mut stream = Kline1mStream::from_klines("ETHUSDT".to_string(), boxed);
let mut count = 0;
while stream.next_message().is_some() {
    count += 1;
}
assert_eq!(count, 120);
```
**输入**: 2根1分钟K线
**预期**: 生成120个子K线
**实际**: 通过 (count = 120)

### 3.4 测试用例: empty_klines
```rust
let klines: Vec<KLine> = vec![];
let boxed = Box::new(klines.into_iter());
let mut stream = Kline1mStream::from_klines("BTCUSDT".to_string(), boxed);
let msg = stream.next_message();
assert!(msg.is_none());
```
**输入**: 空K线列表
**预期**: 返回None
**实际**: 通过

## 4. KlineStreamGenerator 核心逻辑
--------------------------------------------------------------------------------
```
1根1分钟K线 → 60个子K线 (每1秒1个)
路径生成算法:
  - 牛市K线(O→L→H→C): 收盘>=开盘
  - 熊市K线(O→H→L→C): 收盘<开盘
子K线数量分配:
  - 基于各段距离占比分配
  - 高点间距离大 → 更多子K线在高点段
```

## 5. 输出消息格式
--------------------------------------------------------------------------------
```json
{
  "kline_start_time": 1709251200000,
  "kline_close_time": 1709251260000,
  "symbol": "BTCUSDT",
  "interval": "1m",
  "open": "50000",
  "close": "50100",
  "high": "50200",
  "low": "49900",
  "volume": "10.5",
  "is_closed": true/false
}
```

## 6. 与b_data_source Kline1mStream对比
--------------------------------------------------------------------------------
| 特性 | b_data_source | b_data_mock |
|------|--------------|-------------|
| 数据源 | Binance WS | KlineStreamGenerator |
| K线生成 | WS消息直接解析 | 从历史K线合成子K线 |
| next_message | 解析WS JSON | 生成子K线并序列化 |
| 用途 | 实盘订阅 | 回测/模拟 |

## 7. 测试结论
--------------------------------------------------------------------------------
**测试结果**: 通过

**覆盖范围**:
- Kline1mStream 创建 (from_klines)
- next_message 返回JSON
- 60子K线生成验证
- 多K线连续处理
- 空输入处理

**备注**:
- Kline1mStream 使用 KlineStreamGenerator 生成子K线
- 每根1分钟K线生成60个子K线 (每秒1个)
- 消息格式与Binance WS KlineData对齐

================================================================================
                              报告结束
================================================================================
