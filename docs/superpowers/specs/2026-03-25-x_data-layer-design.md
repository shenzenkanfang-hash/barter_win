================================================================
x_data 数据抽象层 - 架构重构设计文档
================================================================
Project: barter-rs 量化交易系统
Author: 软件架构师 + Droid
Date: 2026-03-25
Status: 🔄 Phase 1+6 执行中
================================================================

## 1. 背景与目标

### 1.1 问题描述

当前系统存在两类架构问题：

| Issue | 问题 | 影响 |
|-------|------|------|
| ARCH-001 | 模块边界模糊 | a_common 包含业务数据类型（SymbolRulesData、LocalPosition等），与"纯基础设施"定位冲突 |
| ARCH-002 | 状态管理分散 | EngineState、LocalPositionManager、AccountPool 各自管理，无统一视图 |

### 1.2 解决目标

1. 建立 x_data 业务数据抽象层
2. 所有业务数据类型统一管理，消除重复定义
3. 定义 StateManager trait，实现统一状态视图
4. 明确依赖方向：a_common <- x_data <- 业务层（单向依赖，无循环）

================================================================
2. 目标架构
================================================================

### 2.1 分层结构

```
crates/
├── a_common/          → 纯基础设施（不变）
│   ├── error.rs       → EngineError, MarketError, AppError
│   ├── config.rs      → Platform, Paths
│   ├── api/            → RateLimiter, BinanceApiGateway
│   └── ws/             → WebSocket基础设施
│
├── x_data/            → 【新增】业务数据抽象层
│   ├── position/        → 持仓数据类型
│   ├── account/         → 账户数据类型
│   ├── market/          → 市场数据类型
│   ├── trading/         → 交易数据类型
│   └── state/           → 状态管理trait
│
├── b_data_source/     → 依赖 x_data
├── e_risk_monitor/    → 依赖 x_data
└── f_engine/          → 依赖 x_data
```

### 2.2 依赖方向

```
a_common (基础设施)
    ↓
x_data (业务数据)
    ↓
b_data_source / e_risk_monitor / f_engine (业务层)
```

================================================================
3. x_data 目录结构
================================================================

```
crates/x_data/src/
├── lib.rs                     → 统一导出所有子模块
│
├── position/                  → 持仓数据类型
│   ├── mod.rs
│   ├── types.rs              → LocalPosition, Direction, PositionSide
│   └── snapshot.rs           → PositionSnapshot, UnifiedPositionSnapshot, Positions
│
├── account/                   → 账户数据类型
│   ├── mod.rs
│   ├── types.rs              → FundPool, AccountSnapshot
│   └── pool.rs               → FundPoolManager
│
├── market/                    → 市场数据类型
│   ├── mod.rs
│   ├── tick.rs               → Tick
│   ├── kline.rs              → KLine, KlineData
│   ├── orderbook.rs           → OrderBook, OrderBookSnapshot, DepthData, OrderBookLevel
│   └── volatility.rs          → SymbolVolatility, VolatilitySummary
│
├── trading/                   → 交易数据类型
│   ├── mod.rs
│   ├── rules.rs              → SymbolRulesData, ParsedSymbolRules
│   ├── order.rs              → OrderRejectReason, OrderResult, OrderRecord
│   └── futures.rs            → FuturesPosition, FuturesAccount
│
└── state/                     → 状态管理trait
    ├── mod.rs
    └── traits.rs              → StateViewer, StateManager, UnifiedStateView, SystemSnapshot
```

================================================================
4. 数据迁移清单
================================================================

### 4.1 从 a_common 迁出

| 原文件 | 类型 | 新位置 |
|--------|------|--------|
| backup/memory_backup.rs | AccountSnapshot | x_data/account/types.rs |
| backup/memory_backup.rs | PositionSnapshot | x_data/position/snapshot.rs |
| backup/memory_backup.rs | Positions | x_data/position/snapshot.rs |
| models/dto.rs | PositionDirection | x_data/position/types.rs |
| models/market_data.rs | OrderBookLevel | x_data/market/orderbook.rs |
| models/market_data.rs | OrderBookSnapshot | x_data/market/orderbook.rs |
| exchange/mod.rs | OrderRejectReason | x_data/trading/order.rs |
| exchange/mod.rs | OrderResult | x_data/trading/order.rs |

### 4.2 从 b_data_source 迁出

| 原文件 | 类型 | 新位置 |
|--------|------|--------|
| models/types.rs | Tick | x_data/market/tick.rs |
| models/types.rs | KLine | x_data/market/kline.rs |
| symbol_rules/mod.rs | SymbolRulesData | x_data/trading/rules.rs |
| symbol_rules/mod.rs | ParsedSymbolRules | x_data/trading/rules.rs |
| api/position.rs | FuturesPosition | x_data/trading/futures.rs |
| api/account.rs | FuturesAccount | x_data/trading/futures.rs |
| ws/order_books/ws.rs | DepthData | x_data/market/orderbook.rs |
| ws/kline_1m/ws.rs | KlineData | x_data/market/kline.rs |
| ws/volatility/mod.rs | SymbolVolatility | x_data/market/volatility.rs |

### 4.3 从 e_risk_monitor 迁出

| 原文件 | 类型 | 新位置 |
|--------|------|--------|
| position/position_manager.rs | LocalPosition | x_data/position/types.rs |
| persistence/startup_recovery.rs | UnifiedPositionSnapshot | x_data/position/snapshot.rs |
| shared/account_pool.rs | FundPool | x_data/account/types.rs |
| shared/account_pool.rs | FundPoolManager | x_data/account/pool.rs |

### 4.4 保留在原层的类型（不是纯数据）

| 类型 | 留在原层原因 |
|------|-------------|
| EngineState | 含引擎运行逻辑，保留在 f_engine |
| LocalPositionManager | 含业务逻辑（持仓管理方法） |
| AccountPool | 含业务逻辑（资金池操作方法） |
| KLineSynthesizer | 含业务逻辑（K线合成算法） |
| SymbolRuleService | 含业务逻辑（规则获取逻辑） |

================================================================
5. StateManager Trait 设计
================================================================

### 5.1 Trait 定义

```rust
// x_data/src/state/traits.rs

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use thiserror::Error;

use crate::position::types::LocalPosition;
use crate::position::snapshot::{UnifiedPositionSnapshot, PositionSnapshot};
use crate::account::types::AccountSnapshot;
use crate::trading::order::OrderRecord;
use crate::AppError;

/// 状态视图 trait（只读接口）
pub trait StateViewer: Send + Sync {
    fn get_positions(&self) -> Vec<UnifiedPositionSnapshot>;
    fn get_account(&self) -> Option<AccountSnapshot>;
    fn get_open_orders(&self) -> Vec<OrderRecord>;
}

/// 状态管理器 trait（可写接口）
pub trait StateManager: StateViewer {
    fn update_position(&self, symbol: &str, pos: LocalPosition) -> Result<(), AppError>;
    fn remove_position(&self, symbol: &str) -> Result<(), AppError>;
    fn lock_positions(&self) -> RwLockReadGuard<HashMap<String, LocalPosition>>;
}

/// 统一状态视图 - 组合多个 StateManager
pub struct UnifiedStateView {
    position_manager: Arc<dyn StateManager>,
    account_pool: Arc<dyn StateManager>,
}

impl UnifiedStateView {
    pub fn new(
        position_manager: Arc<dyn StateManager>,
        account_pool: Arc<dyn StateManager>,
    ) -> Self {
        Self { position_manager, account_pool }
    }

    /// 原子读取所有状态（带原子锁确保一致性）
    pub fn snapshot(&self) -> SystemSnapshot {
        // 使用 Arc::spawn_blocking 确保跨线程安全
        let positions = self.position_manager.get_positions();
        let account = self.account_pool.get_account();
        SystemSnapshot {
            positions,
            account,
            timestamp: Utc::now(),
        }
    }
}

/// 系统完整快照
pub struct SystemSnapshot {
    pub positions: Vec<UnifiedPositionSnapshot>,
    pub account: Option<AccountSnapshot>,
    pub timestamp: DateTime<Utc>,
}
```

### 5.2 Trait 实现要求

| 实现者 | 实现的 trait | 说明 |
|--------|-------------|------|
| e_risk_monitor::position::LocalPositionManager | StateManager | 持仓状态管理 |
| e_risk_monitor::shared::account_pool::FundPoolManager | StateManager | 账户状态管理 |
| f_engine::core::state::EngineState | StateViewer | 只读引擎状态 |

================================================================
6. Cargo.toml 依赖配置
================================================================

### 6.1 x_data/Cargo.toml

```toml
[package]
name = "x_data"
version = "0.1.0"
edition = "2021"

[dependencies]
# 数值计算
rust_decimal = { workspace = true }

# 时间处理
chrono = { workspace = true, features = ["serde"] }

# 序列化
serde = { workspace = true }
serde_json = { workspace = true }

# 错误处理
thiserror = { workspace = true }

# 同步原语
parking_lot = { workspace = true }

# 依赖 a_common（获取 AppError 等基础设施类型）
a_common = { path = "../a_common" }

[dev-dependencies]
tokio = { workspace = true }
```

### 6.2 各 crate 依赖更新

```toml
# b_data_source/Cargo.toml
[dependencies]
# 删除对 a_common 中数据类型的依赖
x_data = { path = "../x_data" }

# e_risk_monitor/Cargo.toml
[dependencies]
x_data = { path = "../x_data" }

# f_engine/Cargo.toml
[dependencies]
x_data = { path = "../x_data" }
```

### 6.3 Phase 7 re-export 过渡方案

Phase 7 不直接删除 a_common 中的旧类型，而是通过 re-export 保持向后兼容：

```rust
// a_common/src/backup/memory_backup.rs

// 旧类型保留，用于向后兼容
pub use x_data::position::PositionSnapshot;
pub use x_data::account::AccountSnapshot;
```

这样旧代码无需立即修改 import 路径，可逐步迁移。

================================================================
7. 执行阶段
================================================================

| 阶段 | 任务 | 风险 | 产物 | 状态 |
|------|------|------|------|------|
| Phase 1 | 创建 x_data 骨架 + Cargo.toml | 低 | crates/x_data/src/lib.rs | ✅ 完成 |
| Phase 2 | 迁移 position/ 模块 | 中 | 4个类型文件 | ✅ 完成 |
| Phase 3 | 迁移 account/ 模块 | 中 | 3个类型文件 | ✅ 完成 |
| Phase 4 | 迁移 market/ 模块 | 中 | 5个类型文件 | ✅ 完成 |
| Phase 5 | 迁移 trading/ 模块 | 中 | 5个类型文件 | ✅ 完成 |
| Phase 6 | 实现 state/traits.rs | 中 | StateManager trait | ✅ 完成 |
| Phase 7 | 更新 a_common 导出（re-export过渡） | 高 | 使用 pub use x_data:: 保留旧导出 | ⚠️ 暂缓（循环依赖） |
| Phase 8 | 更新 b_data_source 依赖 | 高 | 修改 Cargo.toml + import | ⬜ 待执行 |
| Phase 9 | 更新 e_risk_monitor 依赖 | 高 | 修改 Cargo.toml + import | ⬜ 待执行 |
| Phase 10 | 更新 f_engine 依赖 | 高 | 修改 Cargo.toml + import | ⬜ 待执行 |
| Phase 11 | 编译验证 + 修复 | - | 全部通过 | ⬜ 待执行 |

**注**: Phase 7 因 a_common ↔ x_data 循环依赖暂缓。需要先消除 x_data 对 a_common 的依赖，或将 x_data 迁至独立层级。

================================================================
8. 验收标准
================================================================

- [x] cargo check --all 编译通过 ✅
- [x] x_data 无循环依赖 ✅
- [ ] 所有数据类型只在一处定义（无重复）
- [x] StateManager trait 可被现有类型实现 ✅
- [ ] 不改变任何业务逻辑（只移动类型定义）
- [x] a_common 保留基础设施类型 ✅

================================================================
9. 风险与缓解
================================================================

| 风险 | 级别 | 缓解措施 |
|------|------|----------|
| import 路径大规模修改 | 高 | 分阶段执行，每阶段验证编译 |
| 循环依赖 | 高 | 严格遵循 a_common->x_data->业务层 方向 |
| 遗漏数据类型 | 中 | 迁移后搜索 "pub struct" 确认无遗漏 |
| StateManager trait 设计不合理 | 中 | 先实现简单版本，后续迭代 |

================================================================
10. 回滚方案
================================================================

### 10.1 分阶段回滚能力

| 阶段 | 回滚方式 |
|------|----------|
| Phase 1~6 | 直接删除 x_data 文件夹即可回滚 |
| Phase 7~11 | 恢复 a_common 的类型导出，回退 Cargo.toml 依赖 |

### 10.2 回滚检查清单

- [ ] 删除 x_data 文件夹
- [ ] 恢复 a_common 中被迁移的类型定义
- [ ] 恢复 a_common/Cargo.toml 的依赖配置
- [ ] 恢复 b_data_source/Cargo.toml 的依赖
- [ ] 恢复 e_risk_monitor/Cargo.toml 的依赖
- [ ] 恢复 f_engine/Cargo.toml 的依赖
- [ ] `cargo check --all` 验证编译通过

================================================================
11. 后续优化（可选）
================================================================

完成数据层抽象后，可进一步优化：

1. **ARCH-002 完整实现**：为 LocalPositionManager 和 FundPoolManager 实现 StateManager trait
2. **UnifiedStateView 使用**：在 f_engine 中使用统一状态视图
3. **Builder 模式**：为复杂类型添加 Builder 简化构造

================================================================
附录：迁移类型完整清单
================================================================

| # | 类型 | 来源 | 目标 | 状态 |
|---|------|------|------|------|
| 1 | AccountSnapshot | a_common/backup | x_data/account/types.rs | ⬜ |
| 2 | PositionSnapshot | a_common/backup | x_data/position/snapshot.rs | ⬜ |
| 3 | Positions | a_common/backup | x_data/position/snapshot.rs | ⬜ |
| 4 | PositionDirection | a_common/models/dto.rs | x_data/position/types.rs | ⬜ |
| 5 | OrderBookLevel | a_common/models | x_data/market/orderbook.rs | ⬜ |
| 6 | OrderBookSnapshot | a_common/models | x_data/market/orderbook.rs | ⬜ |
| 7 | OrderRejectReason | a_common/exchange | x_data/trading/order.rs | ⬜ |
| 8 | OrderResult | a_common/exchange | x_data/trading/order.rs | ⬜ |
| 9 | Tick | b_data_source/models | x_data/market/tick.rs | ⬜ |
| 10 | KLine | b_data_source/models | x_data/market/kline.rs | ⬜ |
| 11 | SymbolRulesData | b_data_source | x_data/trading/rules.rs | ⬜ |
| 12 | ParsedSymbolRules | b_data_source | x_data/trading/rules.rs | ⬜ |
| 13 | FuturesPosition | b_data_source/api | x_data/trading/futures.rs | ⬜ |
| 14 | FuturesAccount | b_data_source/api | x_data/trading/futures.rs | ⬜ |
| 15 | DepthData | b_data_source/ws | x_data/market/orderbook.rs | ⬜ |
| 16 | KlineData | b_data_source/ws | x_data/market/kline.rs | ⬜ |
| 17 | SymbolVolatility | b_data_source/ws | x_data/market/volatility.rs | ⬜ |
| 18 | LocalPosition | e_risk_monitor | x_data/position/types.rs | ⬜ |
| 19 | UnifiedPositionSnapshot | e_risk_monitor | x_data/position/snapshot.rs | ⬜ |
| 20 | FundPool | e_risk_monitor | x_data/account/types.rs | ⬜ |
| 21 | FundPoolManager | e_risk_monitor | x_data/account/pool.rs | ⬜ |

图例：⬜ 待迁移 | 🔄 迁移中 | ✅ 已完成

================================================================
End of Design Document
