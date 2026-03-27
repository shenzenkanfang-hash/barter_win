================================================================
barter-rs 量化交易系统 - 八层架构全景图
================================================================

系统全景图描述了这个 Rust 量化交易系统的完整面貌，从物理结构
到设计哲学，共八个层面。每个层面都从特定视角呈现系统的某个维度。

================================================================
第一层：物理结构
================================================================

项目采用多 crate 工作空间结构，物理布局直接映射架构层级。

根目录包含核心配置文件：Cargo.toml 定义工作空间成员和依赖版本，
workspace.dependencies 中声明全局依赖版本，如 tokio = "1"、
rust_decimal = "1.36"、rusqlite = { version = "0.32", features = ["bundled"] }。
根目录还包含 CLAUDE.md 作为 AI 行为规则约束，README.md 作为项目入口。

crates 目录下按从底向上的依赖顺序排列九个 crate：

x_data 位于最底层，仅包含数据类型定义，位于 crates/x_data/src/ 目录，
包含 market/kline.rs、market/tick.rs、trading/order.rs 等模块，
定义 Tick、Kline、Order、Position 等核心结构体，不依赖任何其他 crate。

a_common 是基础设施层，位于 crates/a_common/src/ 目录，包含 api/（BinanceApiGateway、
BinanceWsConnector）、ws/（WebSocket 连接）、config/（Platform、VolatilityConfig）、
models/（MarketData、Dto）、volatility/、backup/ 等模块，为所有上层提供工具能力。

b_data_source 位于 crates/b_data_source/src/ 目录，包含 api/（DataFeeder、SymbolRegistry）、
store/（MemoryStore、VolatilityManager）、history/（历史数据回放）等模块，
负责数据注入和存储。核心文件 data_feeder.rs 中的 push_tick() 函数是数据注入入口。

c_data_process 位于 crates/c_data_process/src/ 目录，负责指标计算和信号生成。
目录结构包含 indicator/、signal/、strategy/ 等子目录。

d_checktable 位于 crates/d_checktable/src/ 目录，负责并发检查验证。
包含 check_15m.rs、check_daily.rs 等检查模块。

e_risk_monitor 位于 crates/e_risk_monitor/src/ 目录，负责风控和持仓管理。
包含 risk_checker.rs、position_manager.rs 等核心模块。

f_engine 是交易引擎，位于 crates/f_engine/src/ 目录，采用七子目录结构：
core/（engine.rs、pipeline.rs）、risk/（risk.rs、order_check.rs）、
order/（order.rs、gateway.rs）、position/（position_manager.rs）、
persistence/（sqlite_persistence.rs、memory_backup.rs）、
channel/（channel.rs、mode.rs）、shared/（account_pool.rs、check_table.rs）。

g_test 位于 crates/g_test/src/ 目录，包含集成测试和单元测试。

h_sandbox 是沙盒层，位于 crates/h_sandbox/src/ 目录，包含 simulator/（Account、OrderEngine）、
gateway/（ShadowBinanceGateway 拦截器）、historical_replay/（ReplayController、
ShardCache）等模块。最重要的入口是 bin/sandbox_full_production.rs。

物理结构的设计原则是：每一层的 crate 只依赖下层的 crate，不允许反向依赖。
这种结构通过 Cargo.toml 中的 path 依赖声明实现，如 a_common = { path = "crates/a_common" }。

================================================================
第二层：逻辑架构
================================================================

系统由八个逻辑层级构成，每层有明确的职责边界和对外接口。

最底层是工具层（x_data + a_common），为所有上层提供通用能力。x_data 定义
业务数据类型（Tick、Order、Position、Signal），a_common 提供平台适配（Platform::detect()
根据操作系统选择存储路径）、配置管理（Config、VolatilityConfig）和日志系统。

工具层之上是数据层（b_data_source），负责与外部市场数据源交互。DataFeeder 通过
push_tick() 函数接收原始 Tick，写入 MemoryStore。数据层维护两个核心状态：
latest_ticks（最新报价）和 klines（各周期 K 线）。VolatilityManager 在 store 模块
的 volatility.rs 中实现，计算波动率指标。

数据层之上是信号层（c_data_process），将原始数据转化为交易信号。信号层读取数据层
提供的 K 线和指标，通过策略逻辑生成 Signal。策略可以是日线策略、分钟策略或 Tick 策略。
信号层采用并行计算，多个指标同时处理，通过 #[async] async/await 语法实现。

信号层通过检查层（d_checktable）进行并发验证。检查项包括交易时间、K 线区间、挂单限制等。
check_15m.rs 和 check_daily.rs 分别实现 15 分钟和日线级别的检查。检查结果通过
Result<CheckResult, CheckError> 类型返回，检查失败则直接中止流程。

检查层通过风控层（e_risk_monitor）进行串行审核。风控层查询 Account 状态检查资金充足性，
查询 Position 状态检查持仓限额，查询 Order 状态检查是否有反向持仓。风控是最后一道关口，
必须串行执行以避免并发导致的风控失效。RiskChecker 结构体定义在 risk_monitor 模块中。

风控层通过引擎层（f_engine）协调整个执行流程。core/engine.rs 中的 TradingEngine 是核心，
接收 Signal 后依次调用检查层、风控层、订单执行层。引擎还管理 SQLite 持久化（persistence/
sqlite_persistence.rs）和内存备份（persistence/memory_backup.rs）。

最外层是沙盒层（h_sandbox），模拟外部市场环境。沙盒的核心是 Simulator 模块，定义在
simulator/mod.rs 中，包含 Account、OrderEngine、RiskChecker 三个子组件。沙盒通过
ShadowBinanceGateway 拦截器（gateway/interceptor.rs）拦截交易所 API 调用，返回模拟结果。

层与层之间的调用关系遵循严格的依赖顺序。数据层不调用任何上层，信号层只调用数据层，
检查层和风控层接收信号层输出，引擎层调用所有其他层，沙盒层包裹整个系统。

================================================================
第三层：数据流动
================================================================

市场数据从进入系统到触发订单，经历完整的生命周期和多次形态变化。

第一阶段是数据注入。原始 Tick 通过 DataFeeder.push_tick() 进入系统。Tick 来源有两种：
WebSocket 实时推送（通过 a_common/ws/binance_ws.rs 连接）和历史数据回放（通过
b_data_source/history/ 模块）。push_tick() 函数签名是 async fn push_tick(&self, tick: Tick)，
接收 Tick 结构体，包含 symbol、price、quantity、timestamp 等字段。

第二阶段是 K 线合成。data_feeder.rs 调用 MemoryStore.update_with_tick() 更新 K 线。
K 线按周期分组存储，每分钟更新一次当前 K 线，已闭合 K 线进入历史存储。合成是增量式的，
不重复计算已完成的 K 线。

第三阶段是指标计算。klines 更新后触发 VolatilityManager.calculate() 计算波动率。
calculate() 函数签名是 pub fn calculate(&mut self, kline: &Kline) -> Decimal，
输入当前 K 线，输出波动率值。计算采用 EMA 指数移动平均，周期参数可配置（默认 20）。
结果存储在 store 的 volatility 字段中。

第四阶段是信号生成。c_data_process 的策略模块读取 K 线和波动率，执行策略逻辑。
策略可以是简单的双均线交叉，也可以是复杂的多指标组合。输出是 Signal 结构体，
包含 direction（买入/卖出）、symbol、price、quantity、reason 等字段。

第五阶段是检查验证。Signal 进入 d_checktable 的检查队列。check_15m() 验证当前是否
处于可交易时间（排除收盘前 15 分钟），check_daily() 验证当日交易次数未超限。
检查并行执行，通过 tokio::join! 宏同时运行多个检查项。

第六阶段是风控审核。检查通过后进入 e_risk_monitor 的风控队列。RiskChecker.pre_check()
验证账户余额充足（balance >= required_margin * quantity），position_manager.check_limit()
验证持仓未超限（position.quantity <= max_position），order_checker.check_reverse()
验证无反向持仓。风控串行执行，确保检查结果不会被并发覆盖。

第七阶段是订单执行。风控通过后进入 f_engine 的订单执行流程。gateway/ 模块调用
ExchangeGateway.place_order()，在沙盒模式下返回 ShadowBinanceGateway 的模拟结果。
订单状态通过 Order 结构体管理，包含 pending、filled、failed 三种状态。

数据在各阶段的存储位置：原始 Tick 存储在 DataFeeder.latest_ticks（HashMap<Symbol, Tick>），
K 线存储在 MemoryStore.klines（HashMap<Symbol, Vec<Kline>>），波动率存储在
MemoryStore.volatility（HashMap<Symbol, Decimal>），持仓存储在 Account.positions
（HashMap<Symbol, Position>）。所有共享数据通过 crate::b_data_source::store 模块的
MemoryStore 结构体管理，该结构体在沙盒初始化时创建为单例。

================================================================
第四层：执行模型
================================================================

系统采用并行执行模型，数据注入和引擎处理同时运行，通过共享存储同步。

主入口是 crates/h_sandbox/src/bin/sandbox_full_production.rs 中的 main() 函数。
main() 解析命令行参数（--symbol HOTUSDT、--start、--end），创建 SandboxContext，
然后启动两个并发任务：ReplayController 和 TradingEngine。

ReplayController 运行在 tokio::spawn() 创建的任务中，负责历史数据回放。
核心函数是 replay_loop()，在 historical_replay/replay_controller.rs 中实现。
循环逻辑是：读取下一条历史数据 → 转换为 Tick → 调用 DataFeeder.push_tick() →
等待下一个时间间隔。等待使用 tokio::time::interval() 或 tokio::time::sleep() 实现。

TradingEngine 也运行在独立任务中，负责实时处理。核心循环在 core/engine.rs 的
run() 函数中，逻辑是：读取最新市场数据 → 计算策略信号 → 执行检查和风控 → 提交订单。
循环使用 loop { ... } 配合 tokio::select! 宏，同时监听多个事件源（数据到达、定时触发、订单回调）。

两个任务通过共享的 MemoryStore 实例同步。DataFeeder 和 TradingEngine 持有同一个
Arc<MarketDataStore> 引用。DataFeeder 写入数据时，TradingEngine 下次读取就能看到更新。
这种设计避免了复杂的跨任务通信，但要求对 store 的访问通过 parking_lot::RwLock 保护。

时间协调通过时间片让出机制实现。Engine 在每个处理周期结束时调用 tokio::task::yield_now()
让出执行权，允许其他任务（如数据注入）获得 CPU。这种机制模拟了真实市场的时间特性，
因为市场数据是连续到达的，不是按需拉取的。

异步执行使用 Rust 的 async/await 语法和 tokio 运行时。所有 IO 操作（文件读写、网络请求）
都是异步的，通过 .await 关键字挂起当前任务而非阻塞线程。spawn() 宏创建独立任务，
join! 宏等待多个任务同时完成，select! 宏监听多个异步事件。

================================================================
第五层：接口契约
================================================================

各层之间通过显式的接口约定通信，接口定义在对应模块的 lib.rs 中导出。

数据层接口定义在 crates/b_data_source/src/api/data_feeder.rs。核心接口：
- pub async fn push_tick(&self, tick: Tick) -> Result<(), DataError>：注入新 Tick
- pub fn get_kline(&self, symbol: &str, period: Period) -> Option<Kline>：查询当前 K 线
- pub fn get_volatility(&self, symbol: &str) -> Option<Decimal>：查询波动率
- pub fn subscribe(&self, symbol: &str) -> impl Stream<Item = Tick>：订阅实时数据

数据层返回的都是 Result 或 Option，不返回默认值。调用方必须处理数据缺失的情况。

信号层接口定义在 crates/c_data_process/src/strategy/mod.rs。核心接口：
- pub async fn generate_signal(&self, market_data: &MarketData) -> Result<Signal, SignalError>
- pub fn set_parameters(&mut self, params: StrategyParams)：设置策略参数

检查层接口定义在 crates/d_checktable/src/mod.rs。核心接口：
- pub async fn check_signal(&self, signal: &Signal, context: &CheckContext)
  -> Result<CheckResult, CheckError>
- pub fn check_15m(&self) -> Result<bool, CheckError>：15 分钟周期检查
- pub fn check_daily(&self) -> Result<bool, CheckError>：日线周期检查

检查层接口是轻量级的，设计为快速执行，返回布尔值加失败原因。

风控层接口定义在 crates/e_risk_monitor/src/risk/mod.rs。核心接口：
- pub async fn pre_check(&self, order: &OrderRequest) -> Result<RiskResult, RiskError>
- pub fn check_balance(&self, required: Decimal) -> Result<bool, RiskError>：资金检查
- pub fn check_position_limit(&self, symbol: &str, quantity: Decimal) -> Result<bool, RiskError>：持仓检查

风控层会修改账户状态（交易执行后），接口带有副作用。

引擎层接口定义在 crates/f_engine/src/core/mod.rs。核心接口：
- pub async fn process_signal(&self, signal: Signal) -> Result<OrderResult, EngineError>
- pub fn get_status(&self) -> EngineStatus：查询引擎状态
- pub async fn start(&mut self) -> Result<(), EngineError>：启动引擎
- pub async fn stop(&mut self) -> Result<(), EngineError>：停止引擎

引擎是最复杂的接口，协调所有其他层。process_signal() 执行完整的处理流水线。

沙盒层接口定义在 crates/h_sandbox/src/lib.rs。核心接口：
- pub struct SandboxContext { pub store: Arc<MarketDataStore>, pub account: Account, ... }
- pub fn new(config: SandboxConfig) -> SandboxContext：创建沙盒上下文
- pub async fn run(&mut self) -> Result<(), SandboxError>：运行沙盒

沙盒接口是对外的最终封装，隐藏了内部组件的复杂性。

接口稳定性原则：底层接口（数据层）倾向于稳定，因为改变影响面太大；
上层接口（引擎层、沙盒层）可能随功能增加而扩展。接口通过 async trait 或
直接函数定义实现，使用 Result<T, E> 处理错误传播。

================================================================
第六层：状态管理
================================================================

系统采用集中式存储和单例模式管理共享状态，确保数据一致性。

核心存储是 MemoryStore 结构体，定义在 crates/b_data_source/src/store/memory_store.rs。
结构体包含以下字段：
- pub latest_ticks: FnvHashMap<Symbol, Tick>：最新报价
- pub klines_1m: FnvHashMap<Symbol, Vec<Kline>>：1 分钟 K 线
- pub klines_15m: FnvHashMap<Symbol, Vec<Kline>>：15 分钟 K 线
- pub klines_1d: FnvHashMap<Symbol, Vec<Kline>>：日 K 线
- pub volatility: FnvHashMap<Symbol, Decimal>：波动率
- pub last_update: FnvHashMap<Symbol, DateTime<Utc>>：最后更新时间

使用 FnvHashMap 而非标准 HashMap，因为 FnV 哈希函数对短键更高效，适合交易场景。
所有字段通过 RwLock 保护，允许并发读取和独占写入：parking_lot::RwLock<Self>。

存储实例在沙盒初始化时创建为单例。代码位于 crates/h_sandbox/src/simulator/mod.rs：
```rust
let store = Arc::new(MarketDataStore::new());
let data_feeder = DataFeeder::new(store.clone());
let engine = TradingEngine::new(store);
```

DataFeeder、TradingEngine、RiskChecker 都持有同一个 store 引用，通过 Arc<T> 共享。
这种设计确保所有组件看到同一份数据，无需额外同步。

账户状态存储在 Account 结构体中，定义在 crates/h_sandbox/src/simulator/account.rs。
核心字段：
- pub balance: Decimal：可用资金
- pub positions: FnvHashMap<Symbol, Position>：持仓映射
- pub orders: Vec<Order>：订单历史

账户状态也通过 RwLock 保护，风控检查和订单执行都需要修改。

状态一致性由存储层保证。所有写操作直接作用于同一个数据源，写入立即生效。
并发写通过 RwLock 的内部机制保护，读取总是看到最新数据。事务性通过
SQLite 持久化保证（persistence/sqlite_persistence.rs），重大事件（如成交）
会写入数据库。

存储分布：市场状态集中在 MemoryStore（数据层），账户状态集中在 Account（沙盒层），
决策状态在 Engine 内部（f_engine/core/engine.rs）。状态不会跨层复制，
组件通过引用访问共享存储，而非复制数据。

================================================================
第七层：边界处理
================================================================

系统设计多层防御处理边界情况，遵循"错误暴露而非掩盖"的原则。

数据缺失时，系统返回错误而非默认值。调用 get_kline() 时，如果 K 线不存在，
返回 Option<Kline> 的 None，调用方必须处理。代码示例：
```rust
let kline = store.get_kline(&symbol, Period::Min1).ok_or(Error::KlineNotFound)?;
```
不会使用 unwrap_or_default() 构造假数据，不会使用 unwrap() 引发 panic，
而是显式返回错误让调用方处理。

计算失败时，错误向上传播。VolatilityManager.calculate() 如果遇到除零或其他计算错误，
返回 Result<Decimal, CalcError>。调用方可以选择中止交易或使用备用逻辑。
不会返回错误的默认值（如 0 或 1），因为错误的指标可能导致错误的交易决策。

组件故障隔离通过错误边界实现。每个组件的错误通过 Result 类型传播，不会扩散到其他组件。
模块边界定义清晰的错误类型：DataError、SignalError、CheckError、RiskError、EngineError。
这些错误类型定义在各自的模块中，通过 thiserror 派生实现。

输入假设检验在数据入口处进行。DataFeeder.push_tick() 验证 Tick 的有效性：
price 必须是正数、timestamp 必须是有效的 ISO8601 格式、symbol 必须是已注册的交易对。
检验失败返回 DataError::InvalidTick，错误被记录并中止处理。

沙盒边界遵循"注入数据不处理业务"原则。ShadowBinanceGateway 拦截器只返回模拟的
API 响应，不执行任何策略逻辑。如果沙盒注入的数据导致系统错误，那是真实系统的 bug，
沙盒不会帮助修复。代码位于 gateway/interceptor.rs：
```rust
impl BinanceGateway for ShadowBinanceGateway {
    async fn place_order(&self, request: OrderRequest) -> Result<Order, GatewayError> {
        // 直接返回模拟结果，不做风控检查
        Ok(self.simulator.execute(request).await)
    }
}
```

超时和重试逻辑在网络层处理。BinanceApiGateway（a_common/src/api/binance_api.rs）
使用 reqwest 客户端，设置超时：reqwest::ClientBuilder::new()
    .timeout(Duration::from_secs(10))
    .build()。失败时返回错误，不自动重试，由上层决定是否重试。

================================================================
第八层：设计原则
================================================================

系统设计体现了几个核心思想，每个思想都有相应的权衡。

并行优先思想。系统采用并行架构，多组件同时运行，通过共享存储同步。选择并行的原因是
交易系统对吞吐量要求高，串行架构无法满足多交易对、多策略同时处理的需求。实现方式：
tokio::spawn() 创建多个任务，tokio::join!() 等待任务完成，tokio::select!() 监听多事件。
权衡是复杂度高，需要处理状态同步和潜在的竞态条件。

共享存储思想。组件通过共享 MemoryStore 实例交换数据，而非通过消息队列。选择共享存储的
原因是延迟敏感，消息队列的额外开销不可接受。实现方式是 Arc<RwLock<T>> 模式，
所有组件持有 Arc 引用，通过 RwLock 保护并发访问。权衡是组件之间耦合度高，
存储成为性能瓶颈，组件不能完全独立演进。

沙盒注入思想。沙盒层只注入原始数据，不执行任何业务逻辑。选择这种设计的原因是确保
测试环境与生产环境一致，测试通过的问题在生产中也应通过。实现方式是 ShadowBinanceGateway
拦截器返回模拟结果，不调用真实交易所 API。权衡是测试速度受限于生产逻辑执行速度，
无法通过跳过计算来加速测试。

错误暴露思想。系统遇到问题时错误向上传播，不捕获返回默认值。选择错误暴露的原因是
交易系统对正确性要求高于可用性，宁可停止交易也不能错误交易。实现方式是所有函数返回
Result<T, E>，调用方必须处理错误。权衡是系统可能因边界情况频繁中止，需要完善的监控告警。

单例模式思想。存储实例在系统初始化时创建，全局唯一。选择单例的原因是确保数据一致性，
避免多实例导致的状态分裂。实现方式是在 main() 或 SandboxContext::new() 中创建一次，
通过 Arc 传递给所有组件。权衡是单例在测试时需要 mock，增加了测试复杂度。

关键权衡。当前实现的限制：并行架构使状态管理复杂，共享存储使组件耦合紧密，沙盒模式
使测试速度受限，错误暴露策略使系统可能频繁中止。这些限制是设计的代价，在当前阶段
是可接受的。未来可能通过改进存储架构（引入 Redis 缓存）、解耦组件（引入消息队列）、
优化沙盒（增加快速模式）、改进错误处理（增加重试和降级）来缓解这些限制，但核心思想不会改变。

================================================================
总结
================================================================

系统的八层全景展示了从物理结构到设计哲学的完整面貌。物理结构通过多 crate 工作空间
实现依赖分层，逻辑架构通过六层职责划分实现关注点分离，数据流动通过形态变化实现
信息转化，执行模型通过并行任务实现高吞吐，接口契约通过显式约定实现松耦合，
状态管理通过单例存储实现数据一致，边界处理通过错误传播实现问题暴露，设计原则通过
核心思想实现架构统一。

八层之间相互关联：物理结构是逻辑架构的基础，逻辑架构决定数据流动方向，执行模型
依赖状态管理，状态管理受接口契约约束，边界处理验证设计原则是否生效。

这套全景图是理解系统、修改系统、测试系统的基础。任何修改都应考虑对各层的影响，
确保修改与设计原则一致。