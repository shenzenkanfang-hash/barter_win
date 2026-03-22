# e_strategy 目录重构设计

## 1. 背景

e_strategy 当前模块职责混乱，部分代码（order/、symbol/）与业务层边界不清，需要按分层架构重组。

## 2. 分层架构核心规则

| 层级 | 性质 | 说明 |
|------|------|------|
| a_common, b_data_source | 工具结构层 | 可被直接引用 |
| c/d/e/f/h 业务层 | 层间隔离 | 只能数据交换 + 锁控制并发 |

**业务层间禁止直接函数引用，只能通过：**
1. 数据交换（消息/trait 对象）
2. 锁控制并发（parking_lot::RwLock）

## 3. 重组后架构

```
crates/
├── a_common/          # 工具层：API/WS/Error/备份
├── b_data_source/      # 工具层：K线合成/Tick/Order提交
│   └── order/         # [新增] 纯提交订单逻辑
├── c_data_process/     # 业务层：指标计算
├── d_risk_monitor/     # 业务层：风控
├── e_strategy/         # 业务层：策略逻辑
│   ├── channel/        # 通道逻辑
│   ├── strategy/       # 策略实现（traits/types/pin/trend）
│   └── shared/         # 策略层共享（check_table）
├── f_engine/           # 业务层：引擎调度
│   ├── core/           # TradingEngine/Pipeline/StrategyPool
│   └── order/          # ExchangeGateway trait
├── h_sandbox/          # 业务层：影子账户（MockBinanceGateway）
└── g_test/            # 测试层
```

## 4. 文件迁移清单

### 4.1 core/ → f_engine/core/

| 文件 | 说明 |
|------|------|
| engine.rs | TradingEngine 主循环 |
| pipeline.rs | 流水线处理器 |
| pipeline_form.rs | 流水线配置 |
| strategy_pool.rs | 策略资金分配 |

### 4.2 channel/ → e_strategy/channel/

| 文件 | 说明 |
|------|------|
| channel.rs | VolatilityChannel 波动率通道 |
| mode.rs | ModeSwitcher 交易模式切换 |

### 4.3 order/ 拆分

| 文件 | 归属 | 说明 |
|------|------|------|
| gateway.rs | f_engine/order/ | ExchangeGateway trait 接口 |
| order.rs | b_data_source/order/ | 纯提交订单逻辑 |
| mock_binance_gateway.rs | h_sandbox/ | 影子账户完整实现 |

### 4.4 strategy/ → e_strategy/strategy/

| 文件 | 说明 |
|------|------|
| traits.rs | Strategy trait 接口 |
| types.rs | OrderRequest, Side 等类型 |
| pin_strategy.rs | Pine颜色策略 |
| trend_strategy.rs | 趋势策略 |

### 4.5 shared/ → e_strategy/shared/

| 文件 | 说明 |
|------|------|
| check_table.rs | CheckTable 策略判断结果记录 |

### 4.6 symbol/ → 删除

统一使用 a_common 的 SymbolRulesData，消除重复。

## 5. 依赖关系更新

### 5.1 新 f_engine/Cargo.toml

```toml
[dependencies]
b_data_source = { path = "../b_data_source" }
d_risk_monitor = { path = "../d_risk_monitor" }
c_data_process = { path = "../c_data_process" }
a_common = { path = "../a_common" }
# ... 其他依赖
```

### 5.2 新 h_sandbox/Cargo.toml

```toml
[dependencies]
a_common = { path = "../a_common" }
b_data_source = { path = "../b_data_source" }
# ... 其他依赖
```

### 5.3 e_strategy/lib.rs 更新导出

```rust
pub use channel::{channel::*, mode::*};
pub use strategy::{traits::*, types::*, pin_strategy::*, trend_strategy::*};
pub use shared::check_table::*;
```

## 6. 实施步骤

1. 创建 `f_engine/` crate 结构
2. 创建 `h_sandbox/` crate 结构
3. 创建 `b_data_source/order/` 目录
4. 移动 core/ → f_engine/core/
5. 移动 order/gateway.rs → f_engine/order/
6. 移动 order/order.rs → b_data_source/order/
7. 移动 order/mock_binance_gateway.rs → h_sandbox/
8. channel/ → e_strategy/channel/（已在正确位置）
9. strategy/ → e_strategy/strategy/（已在正确位置）
10. shared/ → e_strategy/shared/（已在正确位置）
11. 删除 e_strategy/src/symbol/
12. 更新 Cargo.toml workspace members
13. 更新所有 import 路径
14. 编译验证

## 7. 业务层间交互示例

业务层间通过 trait 对象和数据交换交互，禁止直接函数调用：

```rust
// f_engine 通过 trait 对象调用 d_risk_monitor
pub struct TradingEngine {
    risk_checker: Arc<dyn RiskCheckerTrait>,  // trait 对象
    order_executor: Arc<dyn OrderExecutorTrait>,
}

// d_risk_monitor 通过 Channel 接收数据
pub struct RiskChecker {
    receiver: flume::Receiver<RiskCheckRequest>,
}
```

## 8. 影子账户隔离

h_sandbox 完全独立，不参与正常交易流程，仅用于：
- 回测模拟
- 策略验证
- 风险模型测试
