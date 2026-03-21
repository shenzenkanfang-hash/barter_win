================================================================================
量化交易系统 - Rust 重构项目
================================================================================

项目目标
================================================================================
核心是先有再改 先实现在优化
基于 Go 量化交易系统迁移到 Rust，采用 Barter-rs 风格架构的高性能高可用系统。

编译器配置:
- cargo.exe: C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe
- rustc.exe: C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe
- 构建前需设置环境变量: export RUSTC="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/rustc.exe"

有不清楚的先以项目的跑通为主 自己选择 只要不和 约定有大的冲突即可

核心目标:
- 多周期策略并行运行（日线 + 分钟级 + Tick级）
- 高波动时自动切换到高频秒级模式
- 混合持仓模式: 资金池共享，策略持仓独立计算
- 高性能: 1-2 品种秒级/Tick级计算，采用增量计算模式

================================================================================
设计文档（最高指导）
================================================================================

所有开发必须严格遵循以下文档（按优先级）:

1. docs/2026-03-20-trading-system-rust-design.md
   - 架构设计（四层架构）
   - 锁与并发控制
   - 持仓管理
   - 核心业务函数调用链

2. docs/indicator-logic.md
   - 三层指标体系: TR、Pine颜色、价格位置
   - 日线指标 vs 分钟级指标
   - Pine颜色判断逻辑

3. docs/architecture-reference.md
   - Rust 技术栈选择
   - 代码组织规范
   - 需要避免的坑

================================================================================
架构原则（强制）
================================================================================

1. 高频路径无锁
   - Tick接收、指标更新、策略判断全部无锁
   - 锁仅用于下单和资金更新
   - 锁外预检所有风控条件

2. 增量计算 O(1)
   - EMA、SMA、MACD 等指标必须增量计算
   - K线增量更新当前K线
   - 订单簿 O(log N) 更新

3. 三层指标体系
   - TR (True Range): 波动率突破判断
   - Pine颜色: 趋势信号 (MACD + EMA10/20 + RSI)
   - 价格位置: 周期极值判断 (close-low)/(high-low)

4. 混合持仓模式
   - 资金池 RwLock 保护（低频）
   - 策略持仓独立计算（无锁）

================================================================================
模块结构
================================================================================

crates/
├── account/          # 账户层: 资金池、持仓、错误类型
├── market/           # 市场数据层: WebSocket、K线合成、订单簿
├── indicator/        # 指标层: EMA、RSI、Pine颜色、价格位置
├── strategy/         # 策略层: 日线策略、分钟策略、Tick策略
└── engine/           # 引擎层: 风控、订单执行、模式切换

src/
└── main.rs          # 程序入口，tracing 初始化

================================================================================
技术栈
================================================================================

| 组件 | 技术 | 说明 |
|------|------|------|
| Runtime | Tokio | 异步 IO，多线程任务调度 |
| 状态管理 | FnvHashMap | O(1) 查找 |
| 同步原语 | parking_lot | 比 std RwLock 更高效 |
| 数值计算 | rust_decimal | 金融计算避免浮点精度问题 |
| 时间处理 | chrono | DateTime<Utc> |
| 错误处理 | thiserror | 清晰的错误类型层次 |
| 日志 | tracing | 结构化日志 info!/warn!/error! |
| 序列化 | serde | Serialize/Deserialize |

================================================================================
代码规范（强制）
================================================================================

1. 所有 lib.rs 顶部必须添加:
   #![forbid(unsafe_code)]

2. 派生宏顺序:
   #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]

3. 错误类型模式 (使用 thiserror):
   #[derive(Debug, Clone, Eq, PartialEq, Error)]
   pub enum MyError {
       #[error("描述: {0}")]
       MyVariant(String),
   }

4. 避免的问题 (来自 barter-rs 教训):
   - 禁止使用 panic!()，全部返回 Result
   - 禁止在高频路径加锁
   - 禁止过多 clone()，优先使用引用

================================================================================
当前进度
================================================================================

| Phase | 状态 | 说明 |
|-------|------|------|
| Phase 1: Foundation | 完成 | TradingError, Order, Position |
| Phase 2: Market Data | 完成 | Tick, KLine, KLineSynthesizer, MarketConnector |
| Phase 3: Indicator | 完成 | EMA, RSI, PineColor, PricePosition |
| Phase 4: Strategy | 完成 | Strategy trait, Signal, TradingMode |
| Phase 5: Engine | 完成 | RiskPreChecker, OrderExecutor, ModeSwitcher |
| Phase 6: Integration | 完成 | TradingEngine, main.rs |
| Phase 7: Enhancement | 完成 | RiskReChecker, PnlManager, MarketStatusDetector等 |
| Phase A: 线程安全修复 | 完成 | parking_lot::RwLock 保护 |
| Phase A.5: FundPool合并 | 完成 | AccountPool/PnlManager/OrderCheck/StrategyPool线程安全 |

================================================================================
角色定位
================================================================================

当前角色（固定）: 产品经理

根据系统 CLAUDE.md 规则，产品经理是固定角色，不随任务状态变化。
启动时加载: C:/Users/char/.claude/roles/product/产品经理.txt

产品经理职责:
- 主持项目开发流程
- 协调各角色（架构师、开发者、测试工程师）
- 管理阶段进度
- 与用户通信的唯一入口

子代理派发规则:
- 分析阶段: claude -p "读取工作流程优化器角色，执行analyze任务"
- 设计阶段: claude -p "读取软件架构师角色，执行design任务"
- 执行阶段: claude -p "读取开发者角色，执行execute任务"
- 验证阶段: claude -p "读取测试工程师角色，执行verify任务"
- 评审阶段: claude -p "读取代码评审角色，执行validate任务"

================================================================================
编译活动规则
================================================================================

根据系统 CLAUDE.md 规则:
- 开发阶段禁止编译: 不执行 cargo build/check/test
- 功能优先: 先完成所有功能代码实现
- 编译归属测试工程师: verify 阶段由测试工程师执行编译验证
- 自动提交: 每次修改或创建文件后自动 git commit

================================================================================
## 第二部分：Rust 量化交易系统项目规则

### 项目信息

- **项目目录**: `D:\量化策略开发\回测策略\`
- **编译器配置**:
  - cargo.exe: `C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe`
  - rustc.exe: `C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe`

### 设计文档（最高指导）

所有开发必须严格遵循以下文档（按优先级）:

1. `docs\2026-03-20-trading-system-rust-design.md`
   - 架构设计（四层架构）
   - 锁与并发控制
   - 持仓管理
   - 核心业务函数调用链

2. `docs\indicator-logic.md`
   - 三层指标体系: TR、Pine颜色、价格位置
   - 日线指标 vs 分钟级指标
   - Pine颜色判断逻辑

3. `docs\architecture-reference.md`
   - Rust 技术栈选择
   - 代码组织规范
   - 需要避免的坑

### 编译活动规则（当前阶段）

**编译活动已暂停**，完成所有功能代码后再统一编译验证。

| 规则 | 说明 |
|------|------|
| 禁止主动编译 | 不执行 `cargo build`、`cargo check`、`cargo test` |
| 禁止自动修复 | 不主动尝试修复编译错误 |
| 功能优先 | 先完成所有功能代码实现 |
| 统一编译 | 所有功能完成后一次性编译验证 |

### 架构原则（强制）

1. **高频路径无锁**
   - Tick接收、指标更新、策略判断全部无锁
   - 锁仅用于下单和资金更新
   - 锁外预检所有风控条件

2. **增量计算 O(1)**
   - EMA、SMA、MACD 等指标必须增量计算
   - K线增量更新当前K线

3. **三层指标体系**
   - TR (True Range): 波动率突破判断
   - Pine颜色: 趋势信号 (MACD + EMA10/20 + RSI)
   - 价格位置: 周期极值判断

4. **混合持仓模式**
   - 资金池 RwLock 保护（低频）
   - 策略持仓独立计算（无锁）

### 模块结构

```
crates/
├── account/          # 账户层: 资金池、持仓、错误类型
├── market/           # 市场数据层: WebSocket、K线合成、订单簿
├── indicator/        # 指标层: EMA、RSI、Pine颜色、价格位置
├── strategy/         # 策略层: 日线策略、分钟策略、Tick策略
└── engine/           # 引擎层: 风控、订单执行、模式切换

src/
└── main.rs          # 程序入口，tracing 初始化
```

### 技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| Runtime | Tokio | 异步 IO，多线程任务调度 |
| 状态管理 | FnvHashMap | O(1) 查找 |
| 同步原语 | parking_lot | 比 std RwLock 更高效 |
| 数值计算 | rust_decimal | 金融计算避免浮点精度问题 |
| 时间处理 | chrono | DateTime<Utc> |
| 错误处理 | thiserror | 清晰的错误类型层次 |
| 日志 | tracing | 结构化日志 |
| 序列化 | serde | Serialize/Deserialize |

### 代码规范（强制）

1. 所有 `lib.rs` 顶部必须添加:
   ```rust
   #![forbid(unsafe_code)]
   ```

2. 派生宏顺序:
   ```rust
   #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
   ```

3. 错误类型模式 (使用 thiserror):
   ```rust
   #[derive(Debug, Clone, Eq, PartialEq, Error)]
   pub enum MyError {
       #[error("描述: {0}")]
       MyVariant(String),
   }
   ```

4. 避免的问题:
   - 禁止使用 `panic!()`，全部返回 Result
   - 禁止在高频路径加锁
   - 禁止过多 `clone()`，优先使用引用

### 当前进度

| Phase | 状态 | 说明 |
|-------|------|------|
| Phase 1: Foundation | 完成 | TradingError, Order, Position, FundPool |
| Phase 2: Market Data | 完成 | Tick, KLine, KLineSynthesizer |
| Phase 3: Indicator | 完成 | EMA, RSI, PineColor, PricePosition |
| Phase 4: Strategy | 完成 | Strategy trait, Signal, TradingMode |
| Phase 5: Engine | 完成 | RiskPreChecker, OrderExecutor, ModeSwitcher |
| Phase 6: Integration | 进行中 | TradingEngine, main.rs, 类型转换 |

================================================================================
服务器部署流程（长期测试环境）
================================================================================

**服务器**: quant@172.18.57.21 (Ubuntu 24.04, 1.6GB RAM + 2GB Swap)
**编译服务器**: char@192.168.1.17 (Windows 本地，资源充足)  123456

### 铁律：禁止直接在服务器修改代码

**所有代码改动必须在本地 Windows 完成，打包上传到服务器！**

| 允许的操作 | 禁止的操作 |
|-----------|-----------|
| 本地修改代码 | 直接在服务器 vim/edits |
| 本地编译测试 | 服务器上修改后编译 |
| 打包上传 | 服务器 git pull/push |
| 服务器仅用于：运行、监控、查看日志 | 服务器代码变更后上传 |

### 部署步骤

**1. 在 192.168.1.17 (编译服务器) 编译**
```bash
# 本地编译 (资源充足，可以多线程)
cargo build --release
```

**2. 打包上传到服务器**
```bash
tar --exclude='.git' --exclude='*.pdb' -czvf barter-rs.tar.gz target/release/
scp barter-rs.tar.gz quant@172.18.57.21:/home/quant/
```

**3. 服务器解压运行**
```bash
ssh quant@172.18.57.21
tar -xzvf barter-rs.tar.gz
./target/release/data-monitor  # 或其他程序
```

### 服务器内存优化（如需）
```bash
sudo systemctl stop cloudmonitor packagekit tuned unattended-upgrades docker
sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile
```

### 更新部署（代码修改后）
```bash
# 1. 在 192.168.1.17 编译
cargo build --release

# 2. 打包 release 目录
tar --exclude='.git' --exclude='*.pdb' -czvf barter-rs.tar.gz target/release/

# 3. 上传到服务器
scp barter-rs.tar.gz quant@172.18.57.21:/home/quant/

# 4. 服务器解压覆盖
ssh quant@172.18.57.21 "tar -xzvf barter-rs.tar.gz"
```

### 首次部署检查清单
- [ ] 创建 swap (2GB)
- [ ] 安装 Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [ ] 安装依赖: `sudo apt install build-essential pkg-config libssl-dev git`
- [ ] 打包上传 → 编译 → 验证运行

---
