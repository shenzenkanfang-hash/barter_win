# Heartbeat 监控系统缺口分析与增强方案

> 生成时间：2026-03-29
> 状态：待评审
> 负责人：系统架构分析

---

## 1. 当前设计覆盖分析

### 1.1 有效覆盖 ✅

| 方面 | 覆盖情况 | 说明 |
|------|---------|------|
| **组件连通性** | ✅ 已覆盖 | 5 个核心组件按序报到，能检测"谁还活着" |
| **基础延迟监控** | ✅ 已覆盖 | 数据产生 → 各组件处理的时延 |
| **失联检测** | ✅ 已覆盖 | 超时未报到识别 |
| **序列连续性** | ⚠️ 部分 | 通过 heartbeat_sequence 检测丢包/跳序 |

### 1.2 当前架构

```
Mock 数据源 (b_data_mock)
    ↓
KlineStreamGenerator
    ↓
┌─────────────────────────────────────────┐
│           Heartbeat 报到序列              │
│                                         │
│  BS-001 → CP-001 → DT-001 →           │
│  ER-001 → FE-001                       │
│                                         │
│  ✅ 进程内通信                          │
│  ✅ 内存共享                            │
│  ❌ 网络层未测试                        │
│  ❌ 数据完整性未验证                    │
└─────────────────────────────────────────┘
    ↓
HeartbeatReporter
    ↓
JSON Report / Console Output
```

---

## 2. 关键缺口 🚨

### 2.1 网络层连通性未测试

```
当前状态: 所有组件在同进程内，通过 channel/内存通信

真实场景:
  ├── WebSocket 断连
  ├── 自动重连
  ├── 消息堆积
  ├── TCP 半开连接
  └── 限流 (1200 req/min on Binance)
```

**风险**: 网络故障时系统行为未知

### 2.2 数据完整性未验证

```rust
// 当前只报"到了"，没验"对没对"

缺失检查:
  ├── K 线序列号是否连续
  │       (出现 1,2,3,5,6 漏了 4)
  ├── OHLCV 数据合法性
  │       (open <= high >= low 等)
  ├── 时间戳单调性
  │       (乱序数据检测)
  └── 成交量非负
```

**风险**: 错误数据可能导致交易决策失误

### 2.3 背压与流控缺失

```
当前: futures::stream::iter 无限快消费

真实场景:
  ├── 交易所限流 (1200 req/min)
  ├── 消费慢于生产时的队列堆积
  ├── 内存溢出风险
  └── 反压传播
```

**风险**: 数据洪流时系统崩溃

### 2.4 故障注入不足

| 故障类型 | 当前测试 | 需要补充 |
|---------|---------|---------|
| 组件崩溃 | ❌ 无 | 模拟 panic/重启 |
| 网络抖动 | ❌ 无 | 延迟注入 100-500ms |
| 数据损坏 | ❌ 无 | 随机篡改 OHLCV |
| 时钟漂移 | ❌ 无 | NTP 失效场景 |
| 数据库断开 | ❌ 无 | 持久化层故障 |

### 2.5 交易链路未闭环

```
当前流程:
  数据 → 信号 → 风控 → 引擎 (报到结束)
              ↑
  缺失:       │
  ├── 实际下单
  ├── 成交回报
  ├── 持仓更新
  └── 风控校验 (持仓校验)
              ↑
              └─────────────────────────────↓
```

---

## 3. 增强方案路线图

### Phase 1: 数据层加固 (立即)

**目标**: 确保数据质量，防止脏数据进入系统

```rust
// 新增: crates/b_data_mock/src/validator/kline_validator.rs

use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DataIntegrityError {
    #[error("K线序列缺口: 期望 {expected}, 收到 {actual}")]
    SequenceGap { expected: u64, actual: u64 },

    #[error("OHLCV数据非法: {0}")]
    InvalidOHLCV(String),

    #[error("时间戳乱序: 当前 {current:?} < 上一个 {previous:?}")]
    TimestampReorder { current: DateTime<Utc>, previous: DateTime<Utc> },
}

pub struct KlineValidator {
    last_seq: AtomicU64,
    last_timestamp: std::sync::Mutex<Option<DateTime<Utc>>>,
    gap_count: AtomicU64,
    corrupt_count: AtomicU64,
}

impl KlineValidator {
    /// 验证 K 线数据完整性
    pub fn validate(&self, kline: &KLine) -> Result<(), DataIntegrityError> {
        // 1. 序列连续性检查
        let expected = self.last_seq.load(Ordering::SeqCst) + 1;
        if kline.seq != expected {
            self.gap_count.fetch_add(1, Ordering::SeqCst);
            return Err(DataIntegrityError::SequenceGap {
                expected,
                actual: kline.seq,
            });
        }

        // 2. OHLCV 合法性检查
        if kline.open > kline.high || kline.low > kline.high {
            self.corrupt_count.fetch_add(1, Ordering::SeqCst);
            return Err(DataIntegrityError::InvalidOHLCV(
                format!("open={}, high={}, low={}, close={}",
                    kline.open, kline.high, kline.low, kline.close)
            ));
        }

        // 3. 时间戳单调性检查
        let mut last_ts = self.last_timestamp.lock().unwrap();
        if let Some(prev) = *last_ts {
            if kline.timestamp < prev {
                return Err(DataIntegrityError::TimestampReorder {
                    current: kline.timestamp,
                    previous: prev,
                });
            }
        }

        // 更新状态
        self.last_seq.store(kline.seq, Ordering::SeqCst);
        *last_ts = Some(kline.timestamp);

        Ok(())
    }

    /// 获取统计信息
    pub fn stats(&self) -> ValidatorStats {
        ValidatorStats {
            last_seq: self.last_seq.load(Ordering::SeqCst),
            gap_count: self.gap_count.load(Ordering::SeqCst),
            corrupt_count: self.corrupt_count.load(Ordering::SeqCst),
        }
    }
}
```

**交付物**:
- [ ] `KlineValidator` 结构体
- [ ] 序列连续性检查
- [ ] OHLCV 合法性检查
- [ ] 时间戳单调性检查
- [ ] 统计指标暴露

### Phase 2: 故障注入框架

**目标**: 模拟真实故障场景，验证系统韧性

```rust
// 新增: crates/b_data_mock/src/chaos/chaos_layer.rs

use rand::Rng;
use std::time::Duration;

/// 故障注入配置
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// 延迟注入概率 (0.0 - 1.0)
    pub delay_prob: f64,
    /// 延迟范围 (ms)
    pub delay_range_ms: (u64, u64),
    /// 丢包概率 (0.0 - 1.0)
    pub drop_prob: f64,
    /// 数据损坏概率 (0.0 - 1.0)
    pub corrupt_prob: f64,
    /// 组件崩溃概率 (0.0 - 1.0)
    pub crash_prob: f64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            delay_prob: 0.1,      // 10% 概率注入延迟
            delay_range_ms: (100, 500),
            drop_prob: 0.01,      // 1% 丢包率
            corrupt_prob: 0.001,  // 0.1% 数据损坏
            crash_prob: 0.0,      // 默认不模拟崩溃
        }
    }
}

/// 故障注入器
pub struct ChaosInjector {
    config: ChaosConfig,
    rng: rand::ThreadRng,
}

impl ChaosInjector {
    pub fn new(config: ChaosConfig) -> Self {
        Self {
            config,
            rng: rand::thread_rng(),
        }
    }

    /// 注入故障，返回需要延迟的时间
    pub fn inject(&mut self) -> Option<Duration> {
        // 检查是否需要丢包
        if self.rng.gen::<f64>() < self.config.drop_prob {
            return None; // 丢包
        }

        // 检查是否需要延迟
        if self.rng.gen::<f64>() < self.config.delay_prob {
            let (min, max) = self.config.delay_range_ms;
            let delay_ms = self.rng.gen_range(min..=max);
            return Some(Duration::from_millis(delay_ms));
        }

        None
    }

    /// 损坏数据
    pub fn corrupt(&mut self, kline: &mut KLine) {
        if self.rng.gen::<f64>() < self.config.corrupt_prob {
            // 随机篡改价格
            kline.close = kline.close * Decimal::from(0.5 + self.rng.gen::<f64>());
        }
    }
}

/// 故障类型枚举
#[derive(Debug, Clone)]
pub enum ChaosEvent {
    Delay(Duration),
    Drop,
    Corrupt,
    Crash,
}

impl std::fmt::Display for ChaosEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChaosEvent::Delay(d) => write!(f, "Delay({}ms)", d.as_millis()),
            ChaosEvent::Drop => write!(f, "Drop"),
            ChaosEvent::Corrupt => write!(f, "Corrupt"),
            ChaosEvent::Crash => write!(f, "Crash"),
        }
    }
}
```

**交付物**:
- [ ] `ChaosConfig` 配置结构
- [ ] `ChaosInjector` 故障注入器
- [ ] 延迟注入
- [ ] 丢包模拟
- [ ] 数据损坏
- [ ] 组件崩溃模拟

### Phase 3: 全链路压测

**目标**: 模拟真实交易所行为，验证系统极限

```rust
// 新增: crates/b_data_mock/src/simulator/mock_exchange.rs

use std::time::Duration;

/// 模拟交易所配置
#[derive(Debug, Clone)]
pub struct MockExchangeConfig {
    /// 订单延迟 (ms)
    pub order_latency_ms: (u64, u64),
    /// 成交率 (0.0 - 1.0)
    pub fill_rate: f64,
    /// 限流阈值 (req/min)
    pub rate_limit: u32,
    /// 市场波动率
    pub volatility: f64,
}

pub struct MockExchange {
    config: MockExchangeConfig,
    request_count: u32,
    last_reset: std::time::Instant,
}

impl MockExchange {
    pub fn new(config: MockExchangeConfig) -> Self {
        Self {
            config,
            request_count: 0,
            last_reset: std::time::Instant::now(),
        }
    }

    /// 检查是否触发限流
    pub fn check_rate_limit(&mut self) -> Result<(), RateLimitError> {
        // 每分钟重置计数器
        if self.last_reset.elapsed() > Duration::from_secs(60) {
            self.request_count = 0;
            self.last_reset = std::time::Instant::now();
        }

        self.request_count += 1;

        if self.request_count > self.config.rate_limit {
            return Err(RateLimitError::Exceeded {
                limit: self.config.rate_limit,
                current: self.request_count,
            });
        }

        Ok(())
    }

    /// 模拟订单执行
    pub async fn place_order(&mut self, order: &OrderRequest)
        -> Result<OrderResult, ExchangeError> {
        // 检查限流
        self.check_rate_limit()?;

        // 模拟延迟
        let (min, max) = self.config.order_latency_ms;
        let delay = Duration::from_millis(rand::thread_rng().gen_range(min..=max));
        tokio::time::sleep(delay).await;

        // 模拟成交/拒绝
        if rand::thread_rng().gen::<f64>() < self.config.fill_rate {
            Ok(OrderResult::Filled {
                order_id: order.order_id.clone(),
                filled_qty: order.quantity,
                filled_price: order.price.unwrap_or_default(),
                fee: Decimal::ZERO,
            })
        } else {
            Ok(OrderResult::Rejected {
                reason: "Simulated rejection".to_string(),
            })
        }
    }
}

#[derive(Error, Debug)]
pub enum RateLimitError {
    #[error("触发限流: 限制 {limit} req/min, 当前 {current}")]
    Exceeded { limit: u32, current: u32 },
}

#[derive(Error, Debug)]
pub enum ExchangeError {
    #[error("限流: {0}")]
    RateLimit(#[from] RateLimitError),

    #[error("网络错误: {0}")]
    Network(String),

    #[error("交易所错误: {0}")]
    Exchange(String),
}
```

**交付物**:
- [ ] `MockExchange` 模拟交易所
- [ ] 订单延迟模拟
- [ ] 成交率控制
- [ ] 限流模拟
- [ ] 市场波动模拟

---

## 4. 快速验证清单

在投入生产前，建议补充以下测试：

| 测试项 | 方法 | 通过标准 | 优先级 |
|--------|------|---------|--------|
| 网络分区恢复 | 断开 WS 5秒后恢复 | 自动重连，无数据丢失 | P0 |
| 乱序 K 线处理 | 发送 seq=5,3,4,6 | 正确排序或丢弃 | P0 |
| K 线序列缺口 | 发送 1,2,4,5 (缺3) | 检测并告警 | P0 |
| 内存泄漏 | 运行 24 小时 | RSS 增长 < 10% | P1 |
| 极限 TPS | 10x 正常速率灌入 | 不 OOM，延迟 < 1s | P1 |
| 时钟回拨 | 系统时间 -10s | 正确处理，不崩溃 | P1 |
| 订单限流 | 1300 req/min | 正确拒绝，触发告警 | P1 |
| 数据损坏 | 随机篡改 OHLCV | 检测并丢弃 | P2 |
| 组件崩溃 | 模拟 panic | 自动恢复，继续运行 | P2 |
| 并发订单 | 100 并发下单 | 无死锁，结果正确 | P2 |

### 4.1 P0 测试用例

```rust
// 测试: K 线序列缺口检测

#[tokio::test]
async fn test_kline_sequence_gap_detection() {
    let validator = Arc::new(KlineValidator::new());

    // 发送正常序列
    for seq in 1..=5 {
        let kline = create_kline(seq, DateTime::from_timestamp(seq, 0).unwrap());
        validator.validate(&kline).unwrap();
    }

    // 发送缺口序列
    let kline = create_kline(7, DateTime::from_timestamp(7, 0).unwrap());
    let result = validator.validate(&kline);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DataIntegrityError::SequenceGap { expected: 6, actual: 7 }));

    // 验证统计
    let stats = validator.stats();
    assert_eq!(stats.gap_count, 1);
}

// 测试: 网络分区恢复

#[tokio::test]
async fn test_network_partition_recovery() {
    let mut mock_ws = MockWebSocket::new();

    // 模拟断开
    mock_ws.disconnect().await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    // 模拟恢复
    mock_ws.reconnect().await;

    // 验证重连成功
    assert!(mock_ws.is_connected());

    // 验证数据连续性
    let seq_after = mock_ws.last_seq();
    assert!(seq_after > 0); // 应该有数据继续
}
```

---

## 5. 结论与建议

### 5.1 当前状态评估

```
┌─────────────────────────────────────────────────────────────┐
│                    系统健康状态评估                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  当前设计 = 组件"心跳体检"  ✅                              │
│                                                             │
│  生产就绪 = 需要:                                          │
│    ├── "压力测试"     ❌                                  │
│    ├── "故障演练"     ❌                                  │
│    └── "全链路对账"   ❌                                  │
│                                                             │
│  差距评估: 约 60% 完成度                                    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 建议优先级

| 阶段 | 任务 | 估计工时 | 价值 |
|------|------|---------|------|
| **P0** | KlineValidator 数据验证 | 1 天 | 防止脏数据 |
| **P0** | 网络分区恢复测试 | 1 天 | 验证韧性 |
| **P1** | ChaosInjector 故障注入 | 2 天 | 系统压测 |
| **P1** | MockExchange 模拟交易 | 2 天 | 全链路测试 |
| **P2** | 24h 稳定性测试 | 1 天 | 内存泄漏 |
| **P2** | 并发压力测试 | 1 天 | 死锁检测 |

### 5.3 下一步行动

```
Phase 1 (本周):
  ☐ 实现 KlineValidator
  ☐ 集成到 mock_main.rs
  ☐ 编写 P0 测试用例

Phase 2 (下周):
  ☐ 实现 ChaosInjector
  ☐ 实现 MockExchange
  ☐ 编写故障注入测试

Phase 3 (两周内):
  ☐ 完整 P0/P1 测试套件
  ☐ 性能基准测试
  ☐ 生产就绪评审
```

---

## 6. 参考资料

- [Binance WebSocket 限流文档](https://developers.binance.com/docs/websocket_api)
- [Chaos Engineering 最佳实践](https://principlesofchaos.org/)
- [Rust 异步测试框架](https://tokio.rs/tokio/testing)

---

*本文档由 Claude Code 自动生成，基于系统缺口分析*
*版本: v0.1.0 | 状态: 待评审*
