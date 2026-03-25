================================================================
barter-rs 量化系统 - 剩余问题专项改进方案
================================================================
Project: barter-rs 量化交易系统
Author: Claude Code
Date: 2026-03-25
Status: Pending Execution
================================================================

【强制规则】
1. 严格对应文档编号，不新增、不遗漏
2. 100%不修改交易逻辑、风控规则、策略信号
3. 架构类改进必须低侵入、可回滚
4. 每个方案必须包含完整实现步骤和代码

================================================================

## 待修复清单 (8项)

| 编号 | 问题名称 | 优先级 | 难度 |
|------|----------|--------|------|
| TD-002 | 重复的持仓数据结构（部分修复） | P1 | 中 |
| PERF-001 | 内存备份频繁序列化 | P2 | 中 |
| PERF-002 | SQLite写入阻塞主线程 | P2 | 中 |
| FRAG-003 | 交易所API限流处理 | P3 | 高 |
| FRAG-004 | 回滚机制完整性（缺并发测试） | P3 | 低 |
| ARCH-001 | 模块边界模糊 | P3 | 高 |
| ARCH-002 | 状态管理分散 | P3 | 高 |
| ARCH-003 | 错误类型不统一 | P3 | 中 |

================================================================
TD-002 | 重复的持仓数据结构（部分修复）
================================================================

### 当前现状
- 已添加 UnifiedPositionSnapshot::from_memory_backup() 和 from_hard_disk_backup() 构造函数
- LocalPosition（运行时单方向持仓）和 PositionSnapshot（持久化）仍为独立结构
- 两处持仓结构字段映射仍有重复代码

### 改进目标
- 消除 UnifiedPositionSnapshot 构建时的重复字段映射
- 提供统一的持仓数据转换层

### 具体实现步骤

1. 在 startup_recovery.rs 中，为 UnifiedPositionSnapshot 实现 From<MemPositionSnapshot> trait

2. 简化 get_positions() 方法中的映射闭包

### 核心代码片段

```rust
// crates/e_risk_monitor/src/persistence/startup_recovery.rs

// 为 MemPositionSnapshot 实现转换 trait
impl From<MemPositionSnapshot> for UnifiedPositionSnapshot {
    fn from(pos: MemPositionSnapshot) -> Self {
        let updated_at = DateTime::parse_from_rfc3339(&pos.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        Self {
            symbol: pos.symbol,
            long_qty: pos.long_qty,
            long_avg_price: pos.long_avg_price,
            short_qty: pos.short_qty,
            short_avg_price: pos.short_avg_price,
            updated_at,
            source: RecoveryPriority::MemoryDisk,
            checksum: calculate_checksum(&pos),
        }
    }
}

// 简化后的 get_positions 方法
fn get_positions(&self) -> Result<Vec<UnifiedPositionSnapshot>, EngineError> {
    // ... 加载逻辑 ...
    let snapshots: Vec<UnifiedPositionSnapshot> = mem_positions
        .positions
        .into_iter()
        .map(UnifiedPositionSnapshot::from)
        .collect();
    Ok(snapshots)
}
```

### 检查验收标准
- [ ] cargo check --all 编译通过
- [ ] UnifiedPositionSnapshot::from(pos) 能正确转换 MemPositionSnapshot
- [ ] 不改变任何业务逻辑

### 风险说明
- 仅重构转换代码，无业务逻辑变更
- 风险等级：低

================================================================
PERF-001 | 内存备份频繁序列化
================================================================

### 当前现状
- 每次 save_xxx() 调用都执行完整的 JSON 序列化
- append_trade() 每次都检查文件大小

### 改进目标
- 引入缓冲写入机制，减少序列化次数
- 批量检查文件大小

### 具体实现步骤

1. 在 MemoryBackup 结构体中添加缓冲字段：
   - write_buffer: HashMap<String, Vec<u8>>
   - last_flush: HashMap<String, Instant>
   - BUFFER_FLUSH_INTERVAL_SECS: u64 = 5

2. 修改 save_xxx() 方法，将数据写入缓冲而非直接序列化

3. 实现 flush_buffer() 方法，定期将缓冲写入磁盘

4. 修改 append_trade()，每 10 次调用才检查一次文件大小

### 核心代码片段

```rust
// crates/a_common/src/backup/memory_backup.rs

pub struct MemoryBackup {
    // ... 现有字段 ...
    /// 写入缓冲（symbol -> 待写入的 JSON 数据）
    write_buffer: HashMap<String, Vec<u8>>,
    /// 上次刷新时间
    last_flush: HashMap<String, Instant>,
}

/// 刷新间隔（秒）
const BUFFER_FLUSH_INTERVAL_SECS: u64 = 5;

/// 检查文件大小的调用间隔
const FILE_SIZE_CHECK_INTERVAL: usize = 10;

impl MemoryBackup {
    /// 刷新缓冲（内部使用）
    async fn flush_buffer(&mut self, symbol: &str) -> Result<(), EngineError> {
        if let Some(data) = self.write_buffer.get(symbol) {
            if !data.is_empty() {
                let path = format!("{}/{}.json", self.tmpfs_dir, symbol);
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .await
                    .map_err(|e| EngineError::MemoryBackup(format!("打开文件失败: {}", e)))?;
                file.write_all(data).await
                    .map_err(|e| EngineError::MemoryBackup(format!("写入缓冲失败: {}", e)))?;
                self.write_buffer.get_mut(symbol).unwrap().clear();
            }
        }
        self.last_flush.insert(symbol.to_string(), Instant::now());
        Ok(())
    }

    /// 缓冲写入（替代直接序列化）
    pub async fn save_with_buffer(&mut self, symbol: &str, data: &impl Serialize) -> Result<(), EngineError> {
        let json = serde_json::to_vec(data)
            .map_err(|e| EngineError::MemoryBackup(format!("序列化失败: {}", e)))?;

        // 追加到缓冲
        let buffer = self.write_buffer.entry(symbol.to_string()).or_insert_with(Vec::new);
        buffer.extend_from_slice(&json);
        buffer.push(b'\n');

        // 检查是否需要刷新
        let now = Instant::now();
        let should_flush = self.last_flush
            .get(symbol)
            .map(|t| now.duration_since(*t).as_secs() >= BUFFER_FLUSH_INTERVAL_SECS)
            .unwrap_or(true);

        if should_flush {
            self.flush_buffer(symbol).await?;
        }

        Ok(())
    }
}
```

### 检查验收标准
- [ ] cargo check --all 编译通过
- [ ] 新增缓冲字段不破坏现有数据结构
- [ ] 缓冲刷新间隔可配置

### 风险说明
- 仅改变写入时机和方式，不改变数据内容
- 可能增加内存使用（缓冲未刷新前）
- 风险等级：中

================================================================
PERF-002 | SQLite写入阻塞主线程
================================================================

### 当前现状
- SQLite 写入操作是同步的，可能阻塞交易线程

### 改进目标
- 使用 tokio::spawn 异步执行写入，不阻塞主线程

### 具体实现步骤

1. 在 SqlitePersistence 结构体中添加任务追踪字段

2. 将 save_order_sync 包装为异步版本

3. 使用 spawn_write_task() 将写入任务发送到后台

### 核心代码片段

```rust
// crates/e_risk_monitor/src/persistence/sqlite_persistence.rs

use tokio::sync::mpsc;

pub struct SqlitePersistence {
    // ... 现有字段 ...
    /// 写入通道（用于异步写入）
    write_tx: Option<mpsc::Sender<WriteTask>>,
}

pub enum WriteTask {
    Order(OrderRecord),
    Position(PositionSnapshot),
}

impl SqlitePersistence {
    /// 异步保存订单（不阻塞主线程）
    pub async fn save_order_async(&self, order: &Order) -> Result<(), EngineError> {
        let record = order.into();
        let tx = self.write_tx.as_ref()
            .ok_or_else(|| EngineError::Other("写入通道未初始化".to_string()))?;

        tx.send(WriteTask::Order(record)).await
            .map_err(|e| EngineError::Other(format!("发送写入任务失败: {}", e)))?;
        Ok(())
    }

    /// 启动写入工作线程
    pub fn start_write_worker(&mut self) {
        let db_path = self.db_path.clone();
        let (tx, mut rx) = mpsc::channel::<WriteTask>(100);

        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                match task {
                    WriteTask::Order(order) => {
                        if let Err(e) = insert_order(&db_path, &order).await {
                            tracing::error!("异步写入订单失败: {}", e);
                        }
                    }
                    WriteTask::Position(pos) => {
                        if let Err(e) = insert_position(&db_path, &pos).await {
                            tracing::error!("异步写入持仓失败: {}", e);
                        }
                    }
                }
            }
        });

        self.write_tx = Some(tx);
    }
}
```

### 检查验收标准
- [ ] cargo check --all 编译通过
- [ ] 异步写入不阻塞主线程
- [ ] 写入失败时有错误日志

### 风险说明
- 异步写入可能导致写入顺序不确定（可接受，订单有独立ID）
- 需要处理写入队列满的情况
- 风险等级：中

================================================================
FRAG-003 | 交易所API限流处理
================================================================

### 当前现状
- RateLimiter 使用固定阈值（80%）等待
- 无请求优先级机制

### 改进目标
- 引入优先级队列，高优先级请求优先处理
- 限流时智能调整请求模式

### 具体实现步骤

1. 定义请求优先级枚举

2. 在 RateLimiter 中添加优先级队列

3. 实现 try_acquire_with_priority() 方法

### 核心代码片段

```rust
// crates/a_common/src/api/binance_api.rs

/// API 请求优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    /// 高优先级：下单、撤单
    High = 2,
    /// 中优先级：账户查询
    Medium = 1,
    /// 低优先级：行情、历史的
    Low = 0,
}

pub struct RateLimiter {
    // ... 现有字段 ...
    /// 优先级队列（高 -> 低）
    priority_queues: [Vec<Instant>; 3],
}

impl RateLimiter {
    /// 创建默认限速器
    pub fn new() -> Self {
        Self {
            // 现有字段
            priority_queues: [Vec::new(), Vec::new(), Vec::new()],
        }
    }

    /// 尝试获取限流令牌（带优先级）
    pub async fn try_acquire_with_priority(&mut self, priority: RequestPriority) -> bool {
        let idx = match priority {
            RequestPriority::High => 0,
            RequestPriority::Medium => 1,
            RequestPriority::Low => 2,
        };

        // 检查高优先级请求是否正在等待
        for i in 0..idx {
            if !self.priority_queues[i].is_empty() {
                // 有更高优先级请求在等待，低优先级请求让行
                return false;
            }
        }

        // 执行现有限流检查...
        self.acquire_common().await
    }
}
```

### 检查验收标准
- [ ] cargo check --all 编译通过
- [ ] 高优先级请求不被低优先级阻塞
- [ ] 不改变现有限流逻辑

### 风险说明
- 引入优先级可能改变请求执行顺序
- 需要确保不会饿死低优先级请求
- 风险等级：高

================================================================
FRAG-004 | 回滚机制完整性（缺并发测试）
================================================================

### 当前现状
- 已有基础测试: test_rollback_manager_order_failed, test_rollback_manager_partial_fill
- 缺少并发回滚测试

### 改进目标
- 添加并发回滚场景测试
- 验证并发安全性

### 具体实现步骤

1. 在 tests.rs 中添加并发回滚测试用例

2. 使用 Arc<Mutex<_>> 模拟并发场景

### 核心代码片段

```rust
// crates/f_engine/src/core/tests.rs (rollback_tests 模块内)

#[test]
fn test_rollback_manager_concurrent() {
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let fund_pool = Arc::new(FundPoolManager::new(dec!(100000), dec!(200000)));
        let manager = Arc::new(RollbackManager::new(fund_pool.clone()));

        // 并发执行 10 次回滚
        let mut handles = vec![];
        for i in 0..10 {
            let m = manager.clone();
            let handle = tokio::spawn(async move {
                m.rollback_order(ChannelType::HighSpeed, dec!(1000 * i as i64))
            });
            handles.push(handle);
        }

        // 等待所有回滚完成
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.success, "并发回滚应该成功");
        }

        // 验证最终状态
        assert_eq!(fund_pool.available(ChannelType::HighSpeed), dec!(100000));
    });
}
```

### 检查验收标准
- [ ] cargo test rollback_tests 通过
- [ ] 并发回滚不导致数据竞争
- [ ] 所有回滚结果一致

### 风险说明
- 仅添加测试代码，不改变回滚逻辑
- 风险等级：低

================================================================
ARCH-001 | 模块边界模糊
================================================================

### 当前现状
- a_common 包含 config/Paths 等基础设施
- b_data_source 等业务层直接使用 a_common 类型

### 改进目标
- 明确 a_common = 纯基础设施（无业务类型依赖）
- 业务类型应在上层 crate 定义

### 具体实现步骤

1. 审计 a_common 的所有导出，识别业务类型

2. 将业务相关类型移至对应业务 crate

3. a_common 只保留：Error, Config, Paths, sanitize 等基础设施

### 核心代码片段

```rust
// 审计清单 - 需要移出 a_common 的类型：
// - SymbolRulesData -> 应在 b_data_source 或 e_risk_monitor
// - LocalPosition -> 应在 e_risk_monitor
// - PositionSnapshot -> 应在 e_risk_monitor

// 建议的新结构：
// crates/a_common/src/
//   - error.rs (EngineError, MarketError)
//   - config.rs (Paths, Platform)
//   - util/sanitize.rs (脱敏工具)

// crates/b_data_source/src/
//   - types/symbol_rules.rs (SymbolRulesFetcher, SymbolRulesData)

// crates/e_risk_monitor/src/
//   - position/types.rs (LocalPosition, Direction)
//   - persistence/types.rs (PositionSnapshot, UnifiedPositionSnapshot)
```

### 检查验收标准
- [ ] a_common 不再依赖任何业务 crate
- [ ] cargo check --all 编译通过
- [ ] 无循环依赖

### 风险说明
- 涉及大量文件移动和 import 修改
- 可能影响多个 crate 的公开 API
- 风险等级：高

================================================================
ARCH-002 | 状态管理分散
================================================================

### 当前现状
- EngineState, LocalPositionManager, AccountPool 各自管理状态
- 无统一的状态视图

### 改进目标
- 定义统一的状态管理 trait
- 提供一致性读取接口

### 具体实现步骤

1. 在 f_engine 中定义 StateManager trait

2. 为各状态管理器实现该 trait

3. 提供统一的 StateViewer

### 核心代码片段

```rust
// crates/f_engine/src/core/state.rs

/// 状态视图 trait（只读接口）
pub trait StateViewer: Send + Sync {
    fn get_positions(&self) -> Vec<UnifiedPositionSnapshot>;
    fn get_account(&self) -> Option<AccountSnapshot>;
    fn get_open_orders(&self) -> Vec<OrderRecord>;
}

/// 状态管理器 trait（可写接口）
pub trait StateManager: StateViewer {
    fn update_position(&self, symbol: &str, pos: LocalPosition) -> Result<(), EngineError>;
    fn remove_position(&self, symbol: &str) -> Result<(), EngineError>;
    fn lock_positions(&self) -> impl Deref<Target = HashMap<String, LocalPosition>>;
}

/// 统一状态视图
pub struct UnifiedStateView {
    position_manager: Arc<dyn StateManager>,
    account_pool: Arc<dyn StateManager>,
}

impl UnifiedStateView {
    /// 原子读取所有状态
    pub fn snapshot(&self) -> SystemSnapshot {
        SystemSnapshot {
            positions: self.position_manager.get_positions(),
            account: self.account_pool.get_account(),
            timestamp: Utc::now(),
        }
    }
}
```

### 检查验收标准
- [ ] cargo check --all 编译通过
- [ ] StateManager trait 可被现有类型实现
- [ ] 不改变现有状态管理逻辑

### 风险说明
- trait 设计可能影响后续扩展
- 需要确保现有代码兼容
- 风险等级：高

================================================================
ARCH-003 | 错误类型不统一
================================================================

### 当前现状
- MarketError 定义在 a_common
- EngineError 也定义在 a_common
- 各子模块还有自己的错误类型

### 改进目标
- 建立统一的错误层次体系
- EngineError 和 MarketError 作为子变体

### 具体实现步骤

1. 定义统一错误枚举 AppError

2. 为 EngineError 和 MarketError 实现 From 转换

3. 逐步将子模块错误统一到 AppError

### 核心代码片段

```rust
// crates/a_common/src/error.rs

/// 统一应用错误类型
#[derive(Debug, Clone, Error)]
pub enum AppError {
    // === 引擎错误 ===
    #[error("[Engine] 风控检查失败: {0}")]
    RiskCheckFailed(String),
    #[error("[Engine] 订单执行失败: {0}")]
    OrderExecutionFailed(String),
    #[error("[Engine] 资金不足: {0}")]
    InsufficientFund(String),

    // === 市场数据错误 ===
    #[error("[Market] WebSocket连接失败: {0}")]
    WebSocketConnectionFailed(String),
    #[error("[Market] WebSocket错误: {0}")]
    WebSocketError(String),
    #[error("[Market] 订阅失败: {0}")]
    SubscribeFailed(String),

    // === 数据错误 ===
    #[error("[Data] 序列化错误: {0}")]
    SerializeError(String),
    #[error("[Data] 解析错误: {0}")]
    ParseError(String),

    // === 基础设施错误 ===
    #[error("[Infra] 内存备份错误: {0}")]
    MemoryBackup(String),
    #[error("[Infra] 网络错误: {0}")]
    Network(String),
}

// From<EngineError> 实现
impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::RiskCheckFailed(msg) => AppError::RiskCheckFailed(msg),
            EngineError::OrderExecutionFailed(msg) => AppError::OrderExecutionFailed(msg),
            EngineError::InsufficientFund(msg) => AppError::InsufficientFund(msg),
            // ... 其他变体映射
            EngineError::Other(msg) => AppError::Other(msg),
        }
    }
}

// From<MarketError> 实现
impl From<MarketError> for AppError {
    fn from(e: MarketError) -> Self {
        match e {
            MarketError::WebSocketConnectionFailed(msg) => AppError::WebSocketConnectionFailed(msg),
            MarketError::WebSocketError(msg) => AppError::WebSocketError(msg),
            // ... 其他变体映射
            MarketError::NetworkError(msg) => AppError::Network(msg),
        }
    }
}
```

### 检查验收标准
- [ ] cargo check --all 编译通过
- [ ] 现有 EngineError/MarketError 可转换为 AppError
- [ ] 错误消息格式统一

### 风险说明
- 改变错误类型可能影响调用方错误处理
- 需要大量 From 实现
- 风险等级：中

================================================================
执行顺序建议
================================================================

推荐分3批执行：

【第1批 - 低风险快速修复】
1. FRAG-004 (回滚并发测试)
2. TD-002 (消除重复映射)

【第2批 - 中风险性能优化】
3. PERF-002 (SQLite异步写入)
4. PERF-001 (缓冲写入) - 需要测试验证
5. ARCH-003 (错误类型统一)

【第3批 - 高风险架构重构】
6. FRAG-003 (智能限流)
7. ARCH-001 (模块边界)
8. ARCH-002 (状态管理)

================================================================
End of REMAINING_FIX_PLAN.md
