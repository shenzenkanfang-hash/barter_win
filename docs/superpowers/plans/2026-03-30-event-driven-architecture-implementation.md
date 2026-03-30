# 事件驱动协程自治架构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Barter-Rs 系统从 50ms 串行轮询架构重构为事件驱动协程自治架构，实现组件自运行、指标按需计算、状态中心轻量化、风控全局串行。

**Architecture:** 分5阶段迁移，每阶段可独立验证：
- P1: StateCenter 轻量状态中心
- P2: RiskService + TradeLock 全局串行风控
- P3: IndicatorService 事件触发 + 日线串行
- P4: DataService 数据层自运行
- P5: StrategyService + EngineManager 协程自治

**Tech Stack:** Rust (tokio/async, parking_lot, rust_decimal, chrono, serde), Cargo Workspace

---

## 第一阶段: StateCenter 轻量状态中心

### 概述
在 x_data crate 中新增 StateCenter trait 和实现，作为轻量级状态观测中心。只记录组件生死状态和最后活跃时间，不承载业务数据。

### 文件结构

```
crates/x_data/src/state/
├── mod.rs           # 导出所有 state 相关类型
├── component.rs     # 新增: ComponentState + ComponentStatus
└── center.rs       # 新增: StateCenter trait + StateCenterImpl
```

### Task 1.1: 创建 ComponentState 类型

**Files:**
- Create: `crates/x_data/src/state/component.rs`
- Modify: `crates/x_data/src/state/mod.rs` (导出)
- Modify: `crates/x_data/Cargo.toml` (添加 async-trait 依赖)

- [ ] **Step 1: 添加 Cargo.toml 依赖**

```toml
# crates/x_data/Cargo.toml
[dependencies]
async-trait = { workspace = true }
```

Run: `cargo check -p x_data`
Expected: 成功，无警告

- [ ] **Step 2: 创建 component.rs 文件**

```rust
//! ComponentState - 组件状态数据结构
//!
//! 轻量级状态，只记录生死和最后活跃时间

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 组件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentStatus {
    /// 正常运行
    Running,
    /// 已停止
    Stopped,
    /// 心跳超时（疑似死亡）
    Stale,
}

/// 组件状态记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentState {
    /// 组件唯一标识
    pub component_id: String,
    /// 组件状态
    pub status: ComponentStatus,
    /// 最后活跃时间戳
    pub last_active: DateTime<Utc>,
    /// 可选的简短错误信息
    pub error_msg: Option<String>,
}

impl ComponentState {
    /// 创建新的存活状态
    pub fn new_running(component_id: String) -> Self {
        Self {
            component_id,
            status: ComponentStatus::Running,
            last_active: Utc::now(),
            error_msg: None,
        }
    }

    /// 创建停止状态
    pub fn new_stopped(component_id: String) -> Self {
        Self {
            component_id,
            status: ComponentStatus::Stopped,
            last_active: Utc::now(),
            error_msg: None,
        }
    }

    /// 创建错误状态
    pub fn new_error(component_id: String, error: String) -> Self {
        Self {
            component_id,
            status: ComponentStatus::Stale,
            last_active: Utc::now(),
            error_msg: Some(error),
        }
    }

    /// 更新为存活状态
    pub fn mark_alive(&mut self) {
        self.status = ComponentStatus::Running;
        self.last_active = Utc::now();
        self.error_msg = None;
    }

    /// 检查是否超时
    pub fn is_stale(&self, threshold_secs: i64) -> bool {
        let elapsed = Utc::now() - self.last_active;
        elapsed.num_seconds() > threshold_secs && self.status != ComponentStatus::Stopped
    }
}
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo check -p x_data`
Expected: 编译成功

- [ ] **Step 4: 更新 mod.rs 导出**

```rust
// crates/x_data/src/state/mod.rs
pub mod component;  // 新增

pub use component::{ComponentState, ComponentStatus};
```

- [ ] **Step 5: 提交**

```bash
git add crates/x_data/src/state/component.rs crates/x_data/src/state/mod.rs crates/x_data/Cargo.toml
git commit -m "feat(x_data): 添加 ComponentState 组件状态类型"
```

---

### Task 1.2: 创建 StateCenter Trait 和实现

**Files:**
- Create: `crates/x_data/src/state/center.rs`
- Modify: `crates/x_data/src/state/mod.rs` (导出)

- [ ] **Step 1: 创建 center.rs 文件**

```rust
//! StateCenter - 轻量级状态中心
//!
//! 核心目标: 知道"组件是否活着"
//! 不需要: 实时高频上报、业务数据、状态变更推送

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::component::{ComponentState, ComponentStatus};

/// StateCenter 错误类型
#[derive(Debug, thiserror::Error)]
pub enum StateCenterError {
    #[error("组件未注册: {0}")]
    NotFound(String),
    #[error("内部错误: {0}")]
    Internal(String),
}

/// StateCenter trait
#[async_trait]
pub trait StateCenter: Send + Sync {
    /// 上报存活状态（轻量心跳）
    async fn report_alive(&self, component_id: &str) -> Result<(), StateCenterError>;

    /// 上报错误状态
    async fn report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError>;

    /// 上报停止状态
    async fn report_stopped(&self, component_id: &str) -> Result<(), StateCenterError>;

    /// 查询组件状态
    async fn get(&self, component_id: &str) -> Option<ComponentState>;

    /// 查询所有组件状态
    async fn get_all(&self) -> Vec<ComponentState>;

    /// 获取所有存活的组件（超时阈值内）
    async fn get_alive(&self, timeout_secs: i64) -> Vec<ComponentState>;

    /// 获取所有 Stale 的组件（心跳超时）
    async fn get_stale(&self, threshold_secs: i64) -> Vec<String>;
}

/// StateCenter 实现
pub struct StateCenterImpl {
    /// 组件状态存储: component_id -> ComponentState
    states: RwLock<HashMap<String, ComponentState>>,
    /// 存活超时阈值（秒）
    stale_threshold_secs: i64,
}

impl StateCenterImpl {
    /// 创建新的 StateCenter
    pub fn new(stale_threshold_secs: i64) -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
            stale_threshold_secs,
        }
    }

    /// 创建默认配置的 StateCenter（超时阈值 30 秒）
    pub fn default() -> Self {
        Self::new(30)
    }

    /// 注册新组件
    pub fn register(&self, component_id: String) {
        let mut states = self.states.write();
        if !states.contains_key(&component_id) {
            states.insert(component_id, ComponentState::new_running(component_id));
        }
    }
}

#[async_trait]
impl StateCenter for StateCenterImpl {
    async fn report_alive(&self, component_id: &str) -> Result<(), StateCenterError> {
        let mut states = self.states.write();
        match states.get_mut(component_id) {
            Some(state) => {
                state.mark_alive();
                Ok(())
            }
            None => {
                // 自动注册新组件
                states.insert(
                    component_id.to_string(),
                    ComponentState::new_running(component_id.to_string()),
                );
                Ok(())
            }
        }
    }

    async fn report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError> {
        let mut states = self.states.write();
        match states.get_mut(component_id) {
            Some(state) => {
                state.status = ComponentStatus::Stale;
                state.last_active = Utc::now();
                state.error_msg = Some(error.to_string());
                Ok(())
            }
            None => Err(StateCenterError::NotFound(component_id.to_string())),
        }
    }

    async fn report_stopped(&self, component_id: &str) -> Result<(), StateCenterError> {
        let mut states = self.states.write();
        match states.get_mut(component_id) {
            Some(state) => {
                state.status = ComponentStatus::Stopped;
                state.last_active = Utc::now();
                Ok(())
            }
            None => Err(StateCenterError::NotFound(component_id.to_string())),
        }
    }

    async fn get(&self, component_id: &str) -> Option<ComponentState> {
        let states = self.states.read();
        states.get(component_id).cloned()
    }

    async fn get_all(&self) -> Vec<ComponentState> {
        let states = self.states.read();
        states.values().cloned().collect()
    }

    async fn get_alive(&self, timeout_secs: i64) -> Vec<ComponentState> {
        let states = self.states.read();
        let now = Utc::now();
        states
            .values()
            .filter(|s| {
                if s.status == ComponentStatus::Stopped {
                    return false;
                }
                let elapsed = now - s.last_active;
                elapsed.num_seconds() <= timeout_secs
            })
            .cloned()
            .collect()
    }

    async fn get_stale(&self, threshold_secs: i64) -> Vec<String> {
        let states = self.states.read();
        let now = Utc::now();
        states
            .iter()
            .filter(|(_, s)| {
                if s.status == ComponentStatus::Stopped {
                    return false;
                }
                let elapsed = now - s.last_active;
                elapsed.num_seconds() > threshold_secs
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// 创建 StateCenter 的便捷函数
pub fn create_state_center(stale_threshold_secs: i64) -> Arc<dyn StateCenter> {
    Arc::new(StateCenterImpl::new(stale_threshold_secs))
}
```

- [ ] **Step 2: 更新 mod.rs 导出**

```rust
// crates/x_data/src/state/mod.rs

// ... 现有代码保持不变 ...

// 新增导出
pub use center::{StateCenter, StateCenterImpl, StateCenterError, create_state_center};
```

- [ ] **Step 3: x_data lib.rs 添加导出**

```rust
// crates/x_data/src/lib.rs
// ... 现有代码 ...
pub use state::{StateViewer, StateManager, UnifiedStateView, SystemSnapshot};
pub use state::{StateCenter, StateCenterImpl, StateCenterError, create_state_center};  // 新增
pub use state::component::{ComponentState, ComponentStatus};  // 新增
```

- [ ] **Step 4: 运行测试验证**

Run: `cargo check -p x_data`
Expected: 编译成功，无警告

- [ ] **Step 5: 提交**

```bash
git add crates/x_data/src/state/center.rs crates/x_data/src/state/mod.rs crates/x_data/src/lib.rs
git commit -m "feat(x_data): 添加 StateCenter trait 和实现"
```

---

### Task 1.3: 编写 StateCenter 单元测试

**Files:**
- Create: `crates/x_data/src/state/center_test.rs`

- [ ] **Step 1: 创建测试文件**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_report_alive_auto_register() {
        let center = create_state_center(30);

        // 首次上报会自动注册
        center.report_alive("test-component").await.unwrap();

        let state = center.get("test-component").await;
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.component_id, "test-component");
        assert_eq!(state.status, ComponentStatus::Running);
        assert!(state.error_msg.is_none());
    }

    #[tokio::test]
    async fn test_report_alive_updates_timestamp() {
        let center = create_state_center(30);
        center.report_alive("test-component").await.unwrap();

        // 再次上报更新状态
        center.report_alive("test-component").await.unwrap();

        let state = center.get("test-component").await.unwrap();
        assert_eq!(state.status, ComponentStatus::Running);
    }

    #[tokio::test]
    async fn test_report_error() {
        let center = create_state_center(30);
        center.report_alive("test-component").await.unwrap();

        center.report_error("test-component", "test error").await.unwrap();

        let state = center.get("test-component").await.unwrap();
        assert_eq!(state.status, ComponentStatus::Stale);
        assert_eq!(state.error_msg, Some("test error".to_string()));
    }

    #[tokio::test]
    async fn test_report_stopped() {
        let center = create_state_center(30);
        center.report_alive("test-component").await.unwrap();

        center.report_stopped("test-component").await.unwrap();

        let state = center.get("test-component").await.unwrap();
        assert_eq!(state.status, ComponentStatus::Stopped);
    }

    #[tokio::test]
    async fn test_get_stale_after_timeout() {
        let center = create_state_center(1);  // 1秒超时

        center.report_alive("test-component").await.unwrap();

        // 等待超时
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let stale = center.get_stale(1).await;
        assert!(stale.contains(&"test-component".to_string()));
    }

    #[tokio::test]
    async fn test_get_all() {
        let center = create_state_center(30);
        center.report_alive("component-1").await.unwrap();
        center.report_alive("component-2").await.unwrap();

        let all = center.get_all().await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let center = create_state_center(30);

        let result = center.report_error("nonexistent", "error").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, StateCenterError::NotFound(_)));
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p x_data --lib -- --nocapture`
Expected: 所有测试通过

- [ ] **Step 3: 提交**

```bash
git add crates/x_data/src/state/center_test.rs
git commit -m "test(x_data): 添加 StateCenter 单元测试"
```

---

### Task 1.4: 集成到 main.rs（验证 StateCenter）

**Files:**
- Modify: `src/main.rs` (添加 StateCenter 初始化和心跳上报)

- [ ] **Step 1: 修改 main.rs 引入 StateCenter**

在文件顶部添加导入:
```rust
use x_data::state::{create_state_center, StateCenter};
```

- [ ] **Step 2: 在 create_components() 中创建 StateCenter**

在函数开头添加:
```rust
// 状态中心（第一阶段新增）
let state_center = create_state_center(30);
tracing::info!("[StateCenter] 创建完成，超时阈值 30s");
```

- [ ] **Step 3: 在 SystemComponents 中添加 state_center**

```rust
struct SystemComponents {
    // ... 现有字段 ...
    /// v6.0: 状态中心（第一阶段新增）
    state_center: Arc<dyn StateCenter>,
}
```

- [ ] **Step 4: 在 create_components 返回值中添加 state_center**

```rust
Ok(SystemComponents {
    // ... 现有字段 ...
    state_center,
})
```

- [ ] **Step 5: 在流水线循环中每层上报心跳**

在 `stage_b_data` 末尾添加:
```rust
// 上报到 StateCenter
let _ = components.state_center.report_alive("b_data.kline_stream").await;
```

在 `stage_f_engine` 末尾添加:
```rust
let _ = components.state_center.report_alive("f_engine.gateway").await;
```

在 `stage_d_check` 末尾添加:
```rust
let _ = components.state_center.report_alive("d_checktable.trader").await;
```

在 `stage_e_risk` 末尾添加:
```rust
let _ = components.state_center.report_alive("e_risk_monitor.checker").await;
```

- [ ] **Step 6: 编译验证**

Run: `cargo check`
Expected: 编译成功，无警告

- [ ] **Step 7: 提交**

```bash
git add src/main.rs
git commit -m "feat(main): 集成 StateCenter 作为第一阶段验证"
```

---

## 第二阶段: RiskService + TradeLock 全局串行风控

### 概述
在 e_risk_monitor crate 中新增 RiskService trait 和 TradeLock 全局锁，实现两阶段风控检查。

### 文件结构

```
crates/e_risk_monitor/src/
├── risk_service.rs    # 新增: RiskService trait + TradeLock
```

### Task 2.1: 创建 TradeLock 全局锁

**Files:**
- Create: `crates/e_risk_monitor/src/trade_lock.rs`
- Modify: `crates/e_risk_monitor/src/lib.rs` (导出)

- [ ] **Step 1: 创建 trade_lock.rs**

```rust
//! TradeLock - 全局交易锁
//!
//! 确保同时只有一个协程在执行交易操作

#![forbid(unsafe_code)]

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 锁错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum LockError {
    #[error("锁已被 {0} 持有")]
    AlreadyHeld(String),
    #[error("获取锁超时")]
    Timeout,
    #[error("风控服务降级，禁止所有交易")]
    Degraded,
}

/// 锁持有者 Guard
pub struct TradeLockGuard {
    lock: Arc<TradeLock>,
    strategy_id: String,
    acquired_at: Instant,
}

impl Drop for TradeLockGuard {
    fn drop(&mut self) {
        let held_duration = self.acquired_at.elapsed();

        // 持有时间过长告警
        if held_duration > Duration::from_millis(100) {
            tracing::warn!(
                "[TradeLock] {} 持有锁时间过长: {:?}",
                self.strategy_id,
                held_duration
            );
        }

        self.lock.release(&self.strategy_id);
    }
}

/// 全局交易锁
pub struct TradeLock {
    /// 当前持有锁的策略
    holder: RwLock<Option<String>>,
    /// 锁的版本号（用于乐观锁检测）
    version: AtomicU64,
    /// 是否降级（风控异常时）
    degraded: RwLock<bool>,
}

impl TradeLock {
    /// 创建新的 TradeLock
    pub fn new() -> Self {
        Self {
            holder: RwLock::new(None),
            version: AtomicU64::new(0),
            degraded: RwLock::new(false),
        }
    }

    /// 尝试获取锁
    pub fn try_acquire(&self, strategy_id: &str) -> Result<TradeLockGuard, LockError> {
        // 检查是否降级
        if *self.degraded.read() {
            return Err(LockError::Degraded);
        }

        let mut holder = self.holder.write();
        match holder.as_ref() {
            Some(h) if h != strategy_id => {
                Err(LockError::AlreadyHeld(h.clone()))
            }
            _ => {
                *holder = Some(strategy_id.to_string());
                self.version.fetch_add(1, Ordering::SeqCst);
                Ok(TradeLockGuard {
                    lock: Arc::new(self.clone()),
                    strategy_id: strategy_id.to_string(),
                    acquired_at: Instant::now(),
                })
            }
        }
    }

    /// 释放锁
    pub fn release(&self, strategy_id: &str) {
        let mut holder = self.holder.write();
        if holder.as_ref() == Some(&strategy_id.to_string()) {
            *holder = None;
            self.version.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 设置降级模式（禁止所有交易）
    pub fn set_degraded(&self, degraded: bool) {
        let mut d = self.degraded.write();
        *d = degraded;

        if degraded {
            // 降级时强制释放所有锁
            let mut holder = self.holder.write();
            *holder = None;
            self.version.fetch_add(1, Ordering::SeqCst);
            tracing::warn!("[TradeLock] 进入降级模式，禁止所有交易");
        }
    }

    /// 检查是否降级
    pub fn is_degraded(&self) -> bool {
        *self.degraded.read()
    }

    /// 获取当前持有者
    pub fn holder(&self) -> Option<String> {
        self.holder.read().clone()
    }

    /// 获取锁版本号
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }
}

impl Default for TradeLock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for TradeLock {
    fn clone(&self) -> Self {
        Self {
            holder: RwLock::new(self.holder.read().clone()),
            version: AtomicU64::new(self.version.load(Ordering::SeqCst)),
            degraded: RwLock::new(*self.degraded.read()),
        }
    }
}

/// 创建 TradeLock 的便捷函数
pub fn create_trade_lock() -> Arc<TradeLock> {
    Arc::new(TradeLock::new())
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo check -p e_risk_monitor`
Expected: 编译成功

- [ ] **Step 3: 提交**

```bash
git add crates/e_risk_monitor/src/trade_lock.rs
git commit -m "feat(e_risk): 添加 TradeLock 全局交易锁"
```

---

### Task 2.2: 创建 RiskService Trait 和请求/结果类型

**Files:**
- Create: `crates/e_risk_monitor/src/risk_service.rs`
- Modify: `crates/e_risk_monitor/src/lib.rs` (导出)

- [ ] **Step 1: 创建 risk_service.rs**

```rust
//! RiskService - 风控服务 trait 和类型定义
//!
//! 全局唯一串行执行，两阶段检查

#![forbid(unsafe_code)]

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 交易方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// 风控检查请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckRequest {
    /// 交易品种
    pub symbol: String,
    /// 交易方向
    pub side: Side,
    /// 数量
    pub qty: Decimal,
    /// 价格
    pub price: Decimal,
    /// 策略 ID
    pub strategy_id: String,
}

/// 风控检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckResult {
    /// 是否批准
    pub approved: bool,
    /// 拒绝原因（若被拒绝）
    pub reason: Option<String>,
    /// 风控调整后的数量（如有）
    pub adjusted_qty: Option<Decimal>,
}

impl RiskCheckResult {
    /// 批准
    pub fn approved() -> Self {
        Self {
            approved: true,
            reason: None,
            adjusted_qty: None,
        }
    }

    /// 批准并调整数量
    pub fn approved_with_qty(qty: Decimal) -> Self {
        Self {
            approved: true,
            reason: None,
            adjusted_qty: Some(qty),
        }
    }

    /// 拒绝
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            approved: false,
            reason: Some(reason.into()),
            adjusted_qty: None,
        }
    }
}

/// 风控服务 trait
#[async_trait]
pub trait RiskService: Send + Sync {
    /// 预检（锁获取前）
    /// 快速拒绝明显违规的请求
    async fn pre_check(&self, request: &RiskCheckRequest) -> RiskCheckResult;

    /// 终检（锁获取后）
    /// 更严格的检查，确保全局状态一致性
    async fn final_check(&self, request: &RiskCheckRequest) -> RiskCheckResult;

    /// 获取风控服务名称
    fn name(&self) -> &str;
}
```

- [ ] **Step 2: 创建 RiskService 实现**

继续在 `risk_service.rs` 添加实现:

```rust
// RiskService 实现示例（基于现有 RiskPreChecker）

use crate::risk::common::RiskPreChecker;
use crate::risk::common::OrderCheck;

/// 基于现有 RiskPreChecker 的 RiskService 实现
pub struct RiskServiceImpl {
    inner: Arc<RiskPreChecker>,
    order_check: Arc<OrderCheck>,
    name: String,
}

impl RiskServiceImpl {
    pub fn new(
        inner: Arc<RiskPreChecker>,
        order_check: Arc<OrderCheck>,
    ) -> Self {
        Self {
            inner,
            order_check,
            name: "RiskServiceImpl".to_string(),
        }
    }
}

#[async_trait]
impl RiskService for RiskServiceImpl {
    async fn pre_check(&self, request: &RiskCheckRequest) -> RiskCheckResult {
        // Stage 1: 预检（锁获取前）
        // 1. 账户余额检查
        // 2. 最小交易量检查
        // 3. 价格合理性检查

        // 最小交易量检查
        if request.qty < Decimal::from(1000) {  // TODO: 配置化
            return RiskCheckResult::rejected("qty below minimum");
        }

        // 价格合理性检查（偏离 > 10% 拒绝）
        // 这里需要从外部获取当前价格，暂时用固定值
        let max_price_deviation = dec!(0.1);

        RiskCheckResult::approved()
    }

    async fn final_check(&self, request: &RiskCheckRequest) -> RiskCheckResult {
        // Stage 2: 终检（锁获取后）
        // 1. 总持仓限额检查
        // 2. Symbol 禁止列表检查
        // 3. 下单频率检查

        // 调用现有风控检查
        match self.inner.pre_check(
            &request.symbol,
            dec!(10000),  // TODO: 从账户获取
            request.qty,
            dec!(10000),  // TODO: 从账户获取
        ) {
            Ok(_) => RiskCheckResult::approved(),
            Err(e) => RiskCheckResult::rejected(format!("risk check failed: {}", e)),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}
```

- [ ] **Step 3: 更新 lib.rs 导出**

```rust
// crates/e_risk_monitor/src/lib.rs

// 新增导出
pub mod trade_lock;
pub mod risk_service;

pub use trade_lock::{TradeLock, TradeLockGuard, LockError, create_trade_lock};
pub use risk_service::{RiskService, RiskServiceImpl, Side, RiskCheckRequest, RiskCheckResult};
```

- [ ] **Step 4: 编译验证**

Run: `cargo check -p e_risk_monitor`
Expected: 编译成功，无警告

- [ ] **Step 5: 提交**

```bash
git add crates/e_risk_monitor/src/trade_lock.rs crates/e_risk_monitor/src/risk_service.rs crates/e_risk_monitor/src/lib.rs
git commit -m "feat(e_risk): 添加 RiskService trait 和 RiskServiceImpl 实现"
```

---

### Task 2.3: 编写 TradeLock 和 RiskService 单元测试

**Files:**
- Create: `crates/e_risk_monitor/src/trade_lock_test.rs`
- Create: `crates/e_risk_monitor/src/risk_service_test.rs`

- [ ] **Step 1: 创建 trade_lock_test.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_release() {
        let lock = create_trade_lock();

        // 首次获取成功
        let guard = lock.try_acquire("strategy-1").unwrap();
        assert_eq!(lock.holder(), Some("strategy-1".to_string()));

        // 释放
        drop(guard);
        assert_eq!(lock.holder(), None);
    }

    #[tokio::test]
    async fn test_same_strategy_can_reacquire() {
        let lock = create_trade_lock();

        let guard1 = lock.try_acquire("strategy-1").unwrap();
        drop(guard1);

        // 同一策略可以重新获取
        let guard2 = lock.try_acquire("strategy-1").unwrap();
        assert_eq!(lock.holder(), Some("strategy-1".to_string()));
    }

    #[tokio::test]
    async fn test_different_strategy_blocked() {
        let lock = create_trade_lock();

        let _guard = lock.try_acquire("strategy-1").unwrap();

        // 其他策略无法获取
        let result = lock.try_acquire("strategy-2");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LockError::AlreadyHeld(_)));
    }

    #[tokio::test]
    async fn test_degraded_blocks_all() {
        let lock = create_trade_lock();

        lock.set_degraded(true);

        // 任何策略都无法获取
        let result = lock.try_acquire("strategy-1");
        assert!(matches!(result.unwrap_err(), LockError::Degraded));
    }

    #[tokio::test]
    async fn test_degraded_releases_all_locks() {
        let lock = create_trade_lock();

        let _guard = lock.try_acquire("strategy-1").unwrap();
        assert_eq!(lock.holder(), Some("strategy-1".to_string()));

        lock.set_degraded(true);
        assert_eq!(lock.holder(), None);

        // 无法重新获取
        let result = lock.try_acquire("strategy-1");
        assert!(matches!(result.unwrap_err(), LockError::Degraded));
    }

    #[tokio::test]
    async fn test_version_increments() {
        let lock = create_trade_lock();
        let v0 = lock.version();

        let _guard = lock.try_acquire("strategy-1").unwrap();
        let v1 = lock.version();
        assert!(v1 > v0);
    }

    #[tokio::test]
    async fn test_concurrent_acquire() {
        let lock = create_trade_lock();
        let lock2 = lock.clone();

        let handle1 = tokio::spawn(async move {
            let _guard = lock.try_acquire("strategy-1").unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        });

        let handle2 = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            lock2.try_acquire("strategy-2")
        });

        let result = handle2.await.unwrap();
        // strategy-2 应该在 strategy-1 释放后才能获取
        // 由于 strategy-1 持有 50ms，strategy-2 应该成功
        assert!(result.is_ok() || matches!(result.unwrap_err(), LockError::AlreadyHeld(_)));
    }
}
```

- [ ] **Step 2: 创建 risk_service_test.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_check_result_approved() {
        let result = RiskCheckResult::approved();
        assert!(result.approved);
        assert!(result.reason.is_none());
        assert!(result.adjusted_qty.is_none());
    }

    #[test]
    fn test_risk_check_result_approved_with_qty() {
        let result = RiskCheckResult::approved_with_qty(dec!(0.5));
        assert!(result.approved);
        assert!(result.adjusted_qty == Some(dec!(0.5)));
    }

    #[test]
    fn test_risk_check_result_rejected() {
        let result = RiskCheckResult::rejected("insufficient balance");
        assert!(!result.approved);
        assert_eq!(result.reason, Some("insufficient balance".to_string()));
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p e_risk_monitor --lib -- --nocapture`
Expected: 所有测试通过

- [ ] **Step 4: 提交**

```bash
git add crates/e_risk_monitor/src/trade_lock_test.rs crates/e_risk_monitor/src/risk_service_test.rs
git commit -m "test(e_risk): 添加 TradeLock 和 RiskService 单元测试"
```

---

### Task 2.4: 集成 TradeLock 到策略执行流程

**Files:**
- Modify: `src/main.rs` (添加 TradeLock 初始化)
- Modify: `d_checktable` 或相关策略代码

- [ ] **Step 1: 在 main.rs 创建 TradeLock**

```rust
use e_risk_monitor::trade_lock::create_trade_lock;
use e_risk_monitor::risk_service::RiskServiceImpl;

// 在 create_components() 中
let trade_lock = create_trade_lock();
tracing::info!("[TradeLock] 创建完成");
```

- [ ] **Step 2: 在 SystemComponents 中添加**

```rust
struct SystemComponents {
    // ... 现有字段 ...
    /// TradeLock
    trade_lock: Arc<TradeLock>,
}
```

- [ ] **Step 3: 提交**

```bash
git add src/main.rs
git commit -m "feat(main): 集成 TradeLock 到策略执行流程"
```

---

## 第三阶段: IndicatorService 事件触发 + 日线串行

### 概述
改造 c_data_process crate，实现分钟级指标事件触发计算和日线指标串行批量计算。

### Task 3.1: 创建 IndicatorStore Trait

**Files:**
- Create: `crates/c_data_process/src/traits.rs`
- Modify: `crates/c_data_process/src/lib.rs`

- [ ] **Step 1: 创建 traits.rs**

```rust
//! IndicatorStore - 指标存储 trait
//!
//! 统一访问分钟级和日线级指标

#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use rust_decimal::Decimal;

/// 分钟级指标输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indicator1mOutput {
    /// zscore (14周期)
    pub zscore_14: Option<f64>,
    /// TR 基准值
    pub tr_base: Option<Decimal>,
    /// 价格位置 (0-100)
    pub pos_norm: Option<f64>,
    /// 是否产生信号
    pub signal: bool,
}

/// 日线级指标输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indicator1dOutput {
    /// 日收益率
    pub daily_return: Decimal,
    /// 波动率
    pub volatility: Decimal,
    /// 其他日级指标...
}

/// IndicatorStore trait - 统一访问接口
#[async_trait]
pub trait IndicatorStore: Send + Sync {
    /// 读取分钟级指标
    async fn get_min(&self, symbol: &str) -> Option<Indicator1mOutput>;

    /// 读取日线级指标
    async fn get_day(&self, symbol: &str) -> Option<Indicator1dOutput>;

    /// 读取所有分钟级指标
    async fn get_all_min(&self) -> std::collections::HashMap<String, Indicator1mOutput>;
}

/// 简单内存实现
pub struct InMemoryIndicatorStore {
    min_cache: parking_lot::RwLock<std::collections::HashMap<String, Indicator1mOutput>>,
    day_cache: parking_lot::RwLock<std::collections::HashMap<String, Indicator1dOutput>>,
}

impl InMemoryIndicatorStore {
    pub fn new() -> Self {
        Self {
            min_cache: parking_lot::RwLock::new(std::collections::HashMap::new()),
            day_cache: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn set_min(&self, symbol: String, indicator: Indicator1mOutput) {
        self.min_cache.write().insert(symbol, indicator);
    }

    pub fn set_day(&self, symbol: String, indicator: Indicator1dOutput) {
        self.day_cache.write().insert(symbol, indicator);
    }
}

impl Default for InMemoryIndicatorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IndicatorStore for InMemoryIndicatorStore {
    async fn get_min(&self, symbol: &str) -> Option<Indicator1mOutput> {
        self.min_cache.read().get(symbol).cloned()
    }

    async fn get_day(&self, symbol: &str) -> Option<Indicator1dOutput> {
        self.day_cache.read().get(symbol).cloned()
    }

    async fn get_all_min(&self) -> std::collections::HashMap<String, Indicator1mOutput> {
        self.min_cache.read().clone()
    }
}
```

- [ ] **Step 2: 更新 lib.rs**

```rust
// crates/c_data_process/src/lib.rs
pub mod traits;  // 新增

pub use traits::{
    IndicatorStore,
    Indicator1mOutput,
    Indicator1dOutput,
    InMemoryIndicatorStore,
};
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p c_data_process`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
git add crates/c_data_process/src/traits.rs crates/c_data_process/src/lib.rs
git commit -m "feat(c_data): 添加 IndicatorStore trait"
```

---

## 第四阶段: DataService 数据层自运行

### 概述
改造数据层为独立自运行服务，将数据写入 SharedStore。

### Task 4.1: 创建 SharedStore Trait

**Files:**
- Create: `crates/b_data_mock/src/shared_store.rs` 或 `crates/b_data_source/src/shared_store.rs`
- 定义 KlineWithSeq 和 SharedStore trait

- [ ] **Step 1: 创建 SharedStore 实现**

在 b_data_mock 或 x_data 中创建 SharedStore:

```rust
//! SharedStore - 共享数据存储
//!
//! 各组件共享的 K 线存储，带版本号机制

#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// K 线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub is_closed: bool,
}

/// 带序列号的 K 线
#[derive(Debug, Clone)]
pub struct KlineWithSeq {
    pub kline: Kline,
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
}

/// SharedStore trait
#[async_trait]
pub trait SharedStore: Send + Sync {
    /// 写入 K 线，返回序列号
    async fn write_kline(&self, symbol: &str, kline: Kline) -> u64;

    /// 读取最新 K 线
    async fn get_kline(&self, symbol: &str) -> Option<KlineWithSeq>;

    /// 读取历史 K 线
    async fn get_history(&self, symbol: &str, limit: usize) -> Vec<KlineWithSeq>;

    /// 读取指定序列号之后的 K 线
    async fn get_since(&self, symbol: &str, min_seq: u64) -> Vec<KlineWithSeq>;

    /// 获取所有 symbol
    async fn get_all_symbols(&self) -> Vec<String>;
}

/// SharedStore 实现
pub struct SharedStoreImpl {
    data: RwLock<HashMap<String, Vec<KlineWithSeq>>>,
    latest_seq: AtomicU64,
    latest: RwLock<HashMap<String, KlineWithSeq>>,
}

impl SharedStoreImpl {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            latest_seq: AtomicU64::new(0),
            latest: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for SharedStoreImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SharedStore for SharedStoreImpl {
    async fn write_kline(&self, symbol: &str, kline: Kline) -> u64 {
        let seq = self.latest_seq.fetch_add(1, Ordering::SeqCst);
        let timestamp = Utc::now();

        let kline_with_seq = KlineWithSeq {
            kline,
            seq,
            timestamp,
        };

        // 更新 latest
        {
            let mut latest = self.latest.write();
            latest.insert(symbol.to_string(), kline_with_seq.clone());
        }

        // 更新历史
        {
            let mut data = self.data.write();
            let klines = data.entry(symbol.to_string()).or_insert_with(Vec::new);
            klines.push(kline_with_seq);
        }

        seq
    }

    async fn get_kline(&self, symbol: &str) -> Option<KlineWithSeq> {
        self.latest.read().get(symbol).cloned()
    }

    async fn get_history(&self, symbol: &str, limit: usize) -> Vec<KlineWithSeq> {
        let data = self.data.read();
        let klines = data.get(symbol);
        match klines {
            Some(klines) => {
                let start = klines.len().saturating_sub(limit);
                klines[start..].to_vec()
            }
            None => Vec::new(),
        }
    }

    async fn get_since(&self, symbol: &str, min_seq: u64) -> Vec<KlineWithSeq> {
        let data = self.data.read();
        let klines = data.get(symbol);
        match klines {
            Some(klines) => {
                klines.iter().filter(|k| k.seq > min_seq).cloned().collect()
            }
            None => Vec::new(),
        }
    }

    async fn get_all_symbols(&self) -> Vec<String> {
        let latest = self.latest.read();
        latest.keys().cloned().collect()
    }
}

pub fn create_shared_store() -> Arc<dyn SharedStore> {
    Arc::new(SharedStoreImpl::new())
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/b_data_mock/src/shared_store.rs  # 或其他合适位置
git commit -m "feat(b_data): 添加 SharedStore trait 和实现"
```

---

## 第五阶段: StrategyService + EngineManager 协程自治

### 概述
实现策略协程自循环和 EngineManager 生命周期管理。

### Task 5.1: 创建 StrategyService Trait

**Files:**
- Create: `crates/d_checktable/src/strategy_service.rs`
- Modify: `crates/f_engine/src/lib.rs` (导出)

- [ ] **Step 1: 创建 strategy_service.rs**

```rust
//! StrategyService - 策略服务 trait
//!
//! 策略协程的统一接口

#![forbid(unsafe_code)]

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use x_data::state::{StateCenter, ComponentStatus};

/// StrategyService trait
#[async_trait]
pub trait StrategyService: Send + Sync {
    /// 获取组件 ID
    fn component_id(&self) -> &str;

    /// 获取交易品种
    fn symbol(&self) -> &str;

    /// 获取状态
    fn status(&self) -> ComponentStatus;

    /// 自循环入口
    async fn run(self: Arc<Self>, shutdown_rx: mpsc::Receiver<()>) {
        tracing::info!("[{}] StrategyService 启动", self.component_id());
        self.run_loop(shutdown_rx).await;
        tracing::info!("[{}] StrategyService 停止", self.component_id());
    }

    /// 运行循环实现
    async fn run_loop(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    self.one_cycle().await;
                }
            }
        }
    }

    /// 一次循环
    async fn one_cycle(&self);
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/d_checktable/src/strategy_service.rs
git commit -m "feat(d_checktable): 添加 StrategyService trait"
```

---

### Task 5.2: 创建 EngineManager

**Files:**
- Create: `crates/f_engine/src/engine_manager.rs`
- Modify: `crates/f_engine/src/lib.rs` (导出)

- [ ] **Step 1: 创建 engine_manager.rs**

```rust
//! EngineManager - 协程生命周期管理器
//!
//! 负责策略协程的启动、监控、重启

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tokio::sync::{broadcast, mpsc};

use d_checktable::strategy_service::StrategyService;
use x_data::state::{StateCenter, create_state_center, ComponentStatus};

/// EngineManager 错误类型
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("组件未找到: {0}")]
    NotFound(String),
    #[error("协程启动失败: {0}")]
    SpawnFailed(String),
}

/// 策略句柄
pub struct StrategyHandle {
    pub component_id: String,
    pub symbol: String,
    pub join_handle: tokio::task::JoinHandle<()>,
    pub shutdown_tx: mpsc::Sender<()>,
    pub retry_count: AtomicU64,
    pub active: AtomicBool,
}

/// EngineManager
pub struct EngineManager {
    state_center: Arc<dyn StateCenter>,
    handles: RwLock<HashMap<String, StrategyHandle>>,
    shutdown_tx: broadcast::Sender<()>,
    stale_threshold_secs: i64,
}

impl EngineManager {
    pub fn new(stale_threshold_secs: i64) -> Self {
        Self {
            state_center: create_state_center(stale_threshold_secs),
            handles: RwLock::new(HashMap::new()),
            shutdown_tx: broadcast::Sender::new(1),
            stale_threshold_secs,
        }
    }

    /// 启动策略协程
    pub async fn spawn(&self, service: Arc<dyn StrategyService>) -> Result<(), EngineError> {
        let component_id = service.component_id().to_string();
        let symbol = service.symbol().to_string();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        // 注册到状态中心
        self.state_center.report_alive(&component_id).await.ok();

        let state_center = self.state_center.clone();

        let join_handle = tokio::spawn(async move {
            service.run(shutdown_rx).await;
            state_center.report_stopped(&component_id).await.ok();
        });

        let handle = StrategyHandle {
            component_id: component_id.clone(),
            symbol,
            join_handle,
            shutdown_tx,
            retry_count: AtomicU64::new(0),
            active: AtomicBool::new(true),
        };

        self.handles.write().insert(component_id, handle);
        Ok(())
    }

    /// 获取状态中心
    pub fn state_center(&self) -> Arc<dyn StateCenter> {
        self.state_center.clone()
    }

    /// 优雅关闭所有协程
    pub async fn shutdown_all(&self) {
        let handles: Vec<_> = self.handles.read().values().cloned().collect();

        // 发送 shutdown 信号
        for h in &handles {
            h.active.store(false, Ordering::SeqCst);
            let _ = h.shutdown_tx.send(()).await;
        }

        // 等待所有协程结束
        for h in handles {
            let _ = h.join_handle.await;
        }

        tracing::info!("[EngineManager] 所有协程已关闭");
    }

    /// 运行重启循环
    pub async fn run_restart_loop(&self) {
        loop {
            tokio::select! {
                _ = self.shutdown_tx.subscribe().recv() => break,
                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    let stale = self.state_center.get_stale(self.stale_threshold_secs).await;
                    for component_id in stale {
                        self.handle_stale(&component_id).await;
                    }
                }
            }
        }
    }

    /// 处理 Stale 组件
    async fn handle_stale(&self, component_id: &str) {
        let handle = self.handles.read().get(component_id).cloned();

        if let Some(h) = handle {
            // 指数退避: 1s, 2s, 4s, 8s, 16s, 32s, 60s(max)
            let retry = h.retry_count.load(Ordering::SeqCst);
            let delay = std::cmp::min(60, 2_i64.saturating_pow(retry as u32)) as u64;
            h.retry_count.fetch_add(1, Ordering::SeqCst);

            tracing::warn!(
                "[EngineManager] {} 心跳超时，{}s 后重启 (重试 {})",
                component_id,
                delay,
                retry + 1
            );

            tokio::time::sleep(Duration::from_secs(delay)).await;

            // 检查是否仍然 Stale
            if let Some(state) = self.state_center.get(component_id).await {
                if state.status == ComponentStatus::Stale && h.active.load(Ordering::SeqCst) {
                    self.respawn(component_id).await;
                }
            }
        }
    }

    /// 重新启动组件（预留接口）
    async fn respawn(&self, _component_id: &str) {
        // TODO: 实现重spawn逻辑
        tracing::info!("[EngineManager] respawn called (TODO)");
    }
}
```

- [ ] **Step 2: 更新 lib.rs 导出**

```rust
// crates/f_engine/src/lib.rs
pub mod engine_manager;  // 新增

pub use engine_manager::{EngineManager, EngineError, StrategyHandle};
```

- [ ] **Step 3: 编译验证**

Run: `cargo check`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
git add crates/f_engine/src/engine_manager.rs crates/f_engine/src/lib.rs
git commit -m "feat(f_engine): 添加 EngineManager 协程生命周期管理"
```

---

## 第六阶段: main.rs 重构为纯启动引导

### 概述
将 main.rs 简化为 50 行以内，纯启动引导，无业务逻辑。

### Task 6.1: 重构 main.rs

**Files:**
- Modify: `src/main.rs` (完全重构)

- [ ] **Step 1: 重写 main.rs**

```rust
//! Trading System v6.0 - 事件驱动协程自治架构
//!
//! main.rs 纯启动引导，无业务逻辑
//! 业务逻辑在各服务协程中自运行

use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use a_common::heartbeat::{self as hb, Config as HbConfig};
use c_data_process::traits::{IndicatorStore, InMemoryIndicatorStore};
use e_risk_monitor::risk_service::{RiskService, RiskServiceImpl, RiskCheckRequest};
use e_risk_monitor::risk::common::RiskPreChecker;
use e_risk_monitor::trade_lock::create_trade_lock;
use f_engine::engine_manager::EngineManager;
use x_data::state::{create_state_center, StateCenter};

// ============================================================================
// 常量
// ============================================================================

const INITIAL_BALANCE: rust_decimal::Decimal = rust_decimal_macros::dec!(10000);
const SYMBOL: &str = "HOTUSDT";
const DB_PATH: &str = "D:/RusProject/barter-rs-main/data/trade_records.db";
const DATA_FILE: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv";

// ============================================================================
// 主程序
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化 tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("==============================================");
    tracing::info!("Trading System v6.0 - 事件驱动协程自治架构");
    tracing::info!("==============================================");

    // 2. 初始化心跳监控
    init_heartbeat();

    // 3. 创建共享组件
    let state_center = create_state_center(30);
    let trade_lock = create_trade_lock();
    let indicator_store: Arc<dyn IndicatorStore> = Arc::new(InMemoryIndicatorStore::new());

    tracing::info!("[StateCenter] 超时阈值 30s");
    tracing::info!("[TradeLock] 创建完成");
    tracing::info!("[IndicatorStore] 创建完成");

    // 4. 创建 EngineManager
    let engine = Arc::new(EngineManager::new(30));

    // 5. 启动主监控循环
    run_monitor_loop(engine.state_center()).await;

    // 6. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}

// ============================================================================
// 心跳监控
// ============================================================================

fn init_heartbeat() {
    hb::init(HbConfig {
        stale_threshold: 3,
        report_interval_secs: 300,
        max_file_age_hours: 24,
        max_file_size_mb: 100,
    });
    tracing::info!("[Heartbeat] 监控初始化完成");
}

async fn run_monitor_loop(state_center: Arc<dyn StateCenter>) {
    let mut tick = interval(Duration::from_secs(10));

    tracing::info!("[Monitor] 开始监控...");

    loop {
        tick.tick().await;

        let alive = state_center.get_alive(30).await;
        let all = state_center.get_all().await;

        tracing::info!(
            "[Monitor] 存活: {}/{} 组件",
            alive.len(),
            all.len()
        );
    }
}

async fn print_heartbeat_report() {
    tracing::info!("==============================================");
    tracing::info!("HEARTBEAT REPORT");
    tracing::info!("==============================================");

    let summary = hb::global().summary().await;
    tracing::info!(
        "Total: {}, Active: {}, Reports: {}",
        summary.total_points,
        summary.active_count,
        summary.reports_count
    );
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check`
Expected: 编译成功，无警告

- [ ] **Step 3: 运行测试**

Run: `cargo run`
Expected: 正常启动，显示监控循环

- [ ] **Step 4: 提交**

```bash
git add src/main.rs
git commit -m "feat(main): 重构为纯启动引导 v6.0"
```

---

## 验收清单

### 第一阶段验收
- [ ] StateCenter 编译通过
- [ ] StateCenter 单元测试全通过
- [ ] main.rs 集成 StateCenter 成功
- [ ] cargo check 零警告

### 第二阶段验收
- [ ] TradeLock 编译通过
- [ ] TradeLock 单元测试全通过
- [ ] RiskService trait 定义完整
- [ ] 锁机制生效

### 第三阶段验收
- [ ] IndicatorStore trait 定义完整
- [ ] 编译通过

### 第四阶段验收
- [ ] SharedStore trait 定义完整
- [ ] 序列号机制生效

### 第五阶段验收
- [ ] StrategyService trait 定义完整
- [ ] EngineManager 编译通过
- [ ] 重启循环生效

### 第六阶段验收
- [ ] main.rs < 50 行（不含注释和空行）
- [ ] 无业务流水线逻辑
- [ ] 系统正常启动

---

## 文件变更总清单

### 新增文件
```
crates/x_data/src/state/component.rs          # ComponentState
crates/x_data/src/state/center.rs            # StateCenter
crates/x_data/src/state/center_test.rs       # StateCenter 测试
crates/e_risk_monitor/src/trade_lock.rs       # TradeLock
crates/e_risk_monitor/src/trade_lock_test.rs # TradeLock 测试
crates/e_risk_monitor/src/risk_service.rs     # RiskService
crates/e_risk_monitor/src/risk_service_test.rs
crates/c_data_process/src/traits.rs          # IndicatorStore
crates/d_checktable/src/strategy_service.rs # StrategyService
crates/f_engine/src/engine_manager.rs         # EngineManager
```

### 修改文件
```
crates/x_data/src/state/mod.rs               # 导出
crates/x_data/src/lib.rs                      # 导出
crates/x_data/Cargo.toml                     # 添加依赖
crates/e_risk_monitor/src/lib.rs             # 导出
crates/c_data_process/src/lib.rs             # 导出
crates/f_engine/src/lib.rs                   # 导出
src/main.rs                                  # 重构
```
