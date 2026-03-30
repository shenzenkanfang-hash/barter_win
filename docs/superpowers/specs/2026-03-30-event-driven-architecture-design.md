================================================================================
                    事件驱动协程自治架构设计方案（修订版）
================================================================================
生成时间: 2026-03-30
范围: main.rs 重构 + 全系统解耦
版本: v2.0 (基于审核反馈修订)

================================================================================
一、核心需求确认
================================================================================

经审核后确认的三大核心需求：

需求1: 指标层计算模式
  - 分钟级指标: 事件触发计算（被订阅才触发，非自循环）
  - 日线级指标: 串行计算（时效性要求低，可批量处理）

需求2: 状态中心
  - 核心目标: 知道"组件是否活着"
  - 不需要实时高频上报
  - 轻量级实现

需求3: 风控
  - 全局唯一串行执行
  - 两阶段检查:
    Stage 1: 获取交易锁之前检查（预检）
    Stage 2: 获取锁之后、实际交易之前检查（终检）

================================================================================
二、整体架构
================================================================================

2.1 架构原则

原则1: 组件自运行
  - DataService 自循环写 SharedStore
  - 策略协程自循环（拉数据 -> 拉指标 -> 决策 -> 报状态）
  - 引擎层只管生命周期

原则2: 指标按需计算
  - 分钟级指标由策略协程主动触发计算
  - 日线级指标串行批量处理
  - 不主动轮询，不预计算

原则3: 状态中心轻量化
  - 只记录组件存活状态 + 最后活跃时间
  - 心跳式上报（可配置间隔，默认 10s）
  - 不承载高频业务数据

原则4: 风控全局串行
  - 全系统唯一风控执行点
  - 两阶段锁机制确保安全
  - 策略只发起请求，不直接下单

2.2 整体架构图

    +-------------------------------------------------------------------+
    |                          main.rs                                   |
    |              (纯启动引导: 创建组件 -> spawn -> 监控)                |
    +-------------------------------------------------------------------+
                                    |
        +---------------------------+---------------------------+
        |                           |                           |
        v                           v                           v
    +-----------+           +------------------+        +----------------+
    | DataSvc   |           | EngineManager    |        | StateCenter   |
    | [spawn]   |           |   [spawn]        |        |  (轻量)        |
    |           |           |                  |        |                |
    | Kline1mSvc  write_kline()     |        |  report_live()  |
    |  自循环写  -------------> SharedStore   |  get_component() |
    |  SharedStore                   |        |                |
    +-----------+           +------------------+        +----------------+

                                    |
                                    | read_kline() / watch_kline()
                                    | request_indicators() (按需触发)
                                    | request_risk_check() -> 串行风控
                                    |
                    +---------------+---------------+---------------+
                    |               |               |               |
                    v               v               v               v
            +------------+  +------------+  +------------+  +------------+
            | H15m策略  |  | H1m策略   |  | 风控服务   |  | 日线服务   |
            | [spawn]   |  | [spawn]   |  | (串行)     |  | (串行)     |
            |           |  |           |  |            |  |            |
            | 自循环    |  | 自循环    |  | 全局唯一   |  | 批量计算   |
            | 拉数据    |  | 拉数据    |  | 两阶段锁   |  | 日级指标   |
            | 按需触发c |  | 按需触发c |  |            |  |            |
            | 发起风控  |  | 发起风控  |  |            |  |            |
            | 报状态    |  | 报状态    |  |            |  |            |
            +------------+  +------------+  +------------+  +------------+

2.3 数据流向

    DataSvc --写--> SharedStore --读--> H15m策略
                         |                 |
                         | read_kline()    request_indicators() 触发计算
                         |                         |
                         +-----------> IndicatorSvc 计算后缓存到 MinIndicatorStore
                                            |
                                            | read_indicators()
                                            |
    策略决策 --request--> 风控服务(串行) --批准/拒绝--> 策略执行下单
                         |
                         | 两阶段锁
                         |  Stage1: pre_check(交易锁获取前)
                         |  Stage2: final_check(锁获取后，交易前)

    各组件 --心跳--> StateCenter --查询--> EngineManager

================================================================================
三、StateCenter 详细设计
================================================================================

3.1 设计目标

核心目标: 知道"组件是否活着"
不需要: 实时高频上报、业务数据、状态变更推送

3.2 组件状态类型

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ComponentState {
        /// 组件唯一标识
        pub component_id: String,
        /// 组件状态
        pub status: ComponentStatus,
        /// 最后活跃时间戳
        pub last_active: DateTime<Utc>,
        /// 可选的简短错误信息（出错时才填）
        pub error_msg: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ComponentStatus {
        /// 正常运行
        Running,
        /// 已停止
        Stopped,
        /// 心跳超时（疑似死亡）
        Stale,
    }

注: 移除了 metadata，不再存储业务数据。StateCenter 只管生死，不管业务。

3.3 StateCenter trait 接口

    #[async_trait::async_trait]
    pub trait StateCenter: Send + Sync {
        /// 上报存活状态（轻量心跳）
        async fn report_alive(&self, component_id: &str) -> Result<(), StateCenterError>;

        /// 上报错误状态
        async fn report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError>;

        /// 查询组件状态
        async fn get(&self, component_id: &str) -> Option<ComponentState>;

        /// 查询所有组件状态
        async fn get_all(&self) -> Vec<ComponentState>;

        /// 获取所有存活的组件
        async fn get_alive(&self, timeout_secs: i64) -> Vec<ComponentState>;

        /// 获取所有 Stale 的组件（心跳超时）
        async fn get_stale(&self, threshold_secs: i64) -> Vec<ComponentState>;
    }

3.4 实现要点

    pub struct StateCenterImpl {
        /// 组件状态存储: component_id -> ComponentState
        states: RwLock<HashMap<String, ComponentState>>,
        /// 存活超时阈值（秒）
        stale_threshold_secs: i64,
    }

实现要点:
  - 所有操作都是 RwLock + HashMap，无 channel，无订阅
  - 策略协程在每次自循环结束时调用 report_alive()
  - EngineManager 定期调用 get_stale() 检测异常组件
  - 心跳间隔可配置（默认 10s），避免频繁调用

3.5 上报时机

    各组件自循环中，最后一步调用 report_alive():
      - DataSvc: 每次 write_kline 完成后
      - H15m策略: 每次决策完成后（无论是否下单）
      - H1m策略: 同上
      - 日线服务: 每批计算完成后
      - 风控服务: 每次检查完成后

================================================================================
四、指标层详细设计
================================================================================

4.1 设计原则

- 分钟级指标: 事件触发（策略协程需要时主动触发计算）
- 日线级指标: 串行批量计算（时效性要求低）
- 不主动轮询，不预计算所有指标

4.2 分钟级指标: 事件触发模式

    #[async_trait::async_trait]
    pub trait MinIndicatorService: Send + Sync {
        /// 策略协程按需触发计算
        async fn compute(&self, symbol: &str, kline: &Kline) -> Indicator1mOutput;

        /// 读取最新计算结果（如果有）
        async fn get_latest(&self, symbol: &str) -> Option<Indicator1mOutput>;
    }

调用流程:

    H15m策略协程自循环:
      1. 读取 K 线 from SharedStore
      2. 调用 min_indicator_svc.compute(symbol, kline)  <- 事件触发
      3. 收到指标结果
      4. 做决策
      5. 发起风控请求
      6. 报状态

注意: 指标计算是同步的，不单独 spawn 协程。策略协程直接 await 计算结果。

4.3 日线级指标: 串行批量模式

    pub struct DayIndicatorService {
        /// 共享存储（读取 1m K 线）
        shared_store: Arc<SharedStore>,
        /// 日线缓存
        cache: RwLock<HashMap<String, Indicator1dOutput>>,
        /// 最后更新时间的排序索引（用于批量处理）
        last_update: RwLock<BTreeMap<DateTime<Utc>, String>>,

        /// 串行锁（确保同一时刻只有一个计算任务）
        compute_lock: tokio::sync::Mutex<()>,
    }

自循环逻辑（低频批量）:

    impl DayIndicatorService {
        pub async fn run(self: Arc<Self>) {
            loop {
                tokio::select! {
                    _ = self.shutdown_rx.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(300)) => {  // 5分钟一次
                        self.compute_batch().await;
                        self.report_alive().await;
                    }
                }
            }
        }

        /// 串行批量计算所有 symbol 的日线指标
        async fn compute_batch(&self) {
            let _lock = self.compute_lock.lock().await;

            let symbols = self.shared_store.get_all_symbols().await;
            for symbol in symbols {
                if let Some(indicators) = self.compute_for_symbol(&symbol).await {
                    self.cache.write().insert(symbol, indicators);
                }
            }
        }
    }

4.4 IndicatorStore 合并

分钟级和日线级指标共用同一 trait:

    #[async_trait::async_trait]
    pub trait IndicatorStore: Send + Sync {
        /// 读取分钟级指标（最新一次计算结果）
        async fn get_min(&self, symbol: &str) -> Option<Indicator1mOutput>;

        /// 读取日线级指标
        async fn get_day(&self, symbol: &str) -> Option<Indicator1dOutput>;
    }

MinIndicatorService 和 DayIndicatorService 都实现该 trait。
策略协程通过 Arc<dyn IndicatorStore> 统一访问。

================================================================================
五、风控层详细设计
================================================================================

5.1 设计目标

- 全局唯一串行执行（所有策略的下单请求都经过同一个风控执行点）
- 两阶段检查确保安全
- 策略只发起请求，不直接下单

5.2 两阶段检查机制

    策略协程                    风控服务(全局唯一)
         |                              |
         | request_risk_check(pre)      |  Stage 1: 预检
         |  (携带: symbol, qty, price)  |  pre_check()
         | ----------------------------->|  (交易锁获取前)
         |                              |
         |   <- Approved / Rejected     |
         |                              |
         | [若 Approved]                 |
         | acquire_trade_lock()          |  获取全局交易锁
         |                              |  （确保只有一个协程在交易）
         |                              |
         | request_risk_check(final)    |  Stage 2: 终检
         |  (携带: symbol, qty, price)  |  final_check()
         | ----------------------------->|  (交易锁获取后)
         |                              |
         |   <- Approved / Rejected     |
         |                              |
         | [若 Approved]                 |
         | execute_order()              |  实际下单
         | release_trade_lock()          |
         |                              |
         | report_alive()               |  上报存活

为什么两阶段?
  - Stage 1（预检）: 快速拒绝明显违规的请求，避免无效加锁
  - Stage 2（终检）: 锁获取后再检查（防止检查后其他协程改变全局状态）

5.3 风控服务 trait

    #[async_trait::async_trait]
    pub trait RiskService: Send + Sync {
        /// 预检（锁获取前）
        async fn pre_check(&self, request: &RiskCheckRequest) -> RiskCheckResult;

        /// 终检（锁获取后）
        async fn final_check(&self, request: &RiskCheckRequest) -> RiskCheckResult;
    }

    #[derive(Debug, Clone)]
    pub struct RiskCheckRequest {
        pub symbol: String,
        pub side: Side,              // Buy / Sell
        pub qty: Decimal,
        pub price: Decimal,
        pub strategy_id: String,     // 哪个策略发起的
    }

    #[derive(Debug, Clone)]
    pub struct RiskCheckResult {
        pub approved: bool,
        pub reason: Option<String>,  // 拒绝原因
        pub adjusted_qty: Option<Decimal>, // 风控调整后的数量（如有）
    }

5.4 全局锁机制

    pub struct TradeLock {
        /// 当前持有锁的策略
        holder: RwLock<Option<String>>,
        /// 锁的版本号（用于乐观锁检测）
        version: AtomicU64,
    }

    impl TradeLock {
        /// 尝试获取锁
        pub async fn acquire(&self, strategy_id: &str) -> Result<TradeLockGuard, LockError> {
            let mut holder = self.holder.write();
            match holder.as_ref() {
                Some(h) if h != strategy_id => {
                    Err(LockError::AlreadyHeld(h.clone()))
                }
                _ => {
                    *holder = Some(strategy_id.to_string());
                    self.version.fetch_add(1, Ordering::SeqCst);
                    Ok(TradeLockGuard {
                        lock: self.clone(),
                        strategy_id: strategy_id.to_string(),
                    })
                }
            }
        }

        /// 释放锁（Guard Drop 时自动调用）
        pub fn release(&self, strategy_id: &str) {
            let mut holder = self.holder.write();
            if holder.as_ref() == Some(&strategy_id.to_string()) {
                *holder = None;
                self.version.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

5.5 策略协程中的风控调用

    impl H15mStrategyService {
        async fn run_loop(&self) {
            loop {
                // 1. 读取 K 线
                let kline = self.shared_store.get_kline(&self.symbol).await;

                // 2. 触发指标计算
                let indicators = self.min_indicator.compute(&self.symbol, &kline).await;

                // 3. 策略决策
                let decision = self.trader.decide(&indicators).await;

                if let Some(order) = decision.to_order() {
                    let request = RiskCheckRequest {
                        symbol: self.symbol.clone(),
                        side: order.side,
                        qty: order.qty,
                        price: order.price,
                        strategy_id: self.component_id.clone(),
                    };

                    // Stage 1: 预检
                    if self.risk_service.pre_check(&request).await.approved {
                        // 尝试获取锁
                        match self.trade_lock.acquire(&self.component_id).await {
                            Ok(_guard) => {
                                // Stage 2: 终检
                                if self.risk_service.final_check(&request).await.approved {
                                    // 执行订单
                                    self.gateway.place_order(order).await;
                                }
                                // guard.drop() 自动释放锁
                            }
                            Err(_) => {
                                // 锁被占用，等待下一次循环
                            }
                        }
                    }
                }

                // 4. 上报存活
                self.state_center.report_alive(&self.component_id).await;
            }
        }
    }

5.6 风控检查内容

Stage 1 预检（快速，拒绝明显违规）:
  - 账户余额是否足够
  - 数量是否 > 最小交易量
  - 价格是否合理（偏离当前价 < 10%）

Stage 2 终检（锁获取后，更严格）:
  - 当前总持仓是否超过限额
  - 该 symbol 是否在禁止交易列表中
  - 当日下单次数是否超限
  - 最新行情是否剧烈波动（暂停交易阈值）

================================================================================
六、策略协程详细设计
================================================================================

6.1 StrategyService trait

    #[async_trait::async_trait]
    pub trait StrategyService: Send + Sync {
        fn component_id(&self) -> &str;
        fn symbol(&self) -> &str;
        fn status(&self) -> ComponentStatus;

        /// 自循环入口
        async fn run(self: Arc<Self>, shutdown_rx: mpsc::Receiver<()>) {
            // 默认自循环实现
        }
    }

6.2 H15mStrategyService 结构

    pub struct H15mStrategyService {
        pub symbol: String,
        pub component_id: String,
        pub shared_store: Arc<SharedStore>,
        pub indicator_store: Arc<dyn IndicatorStore>,
        pub trader: Arc<Trader>,
        pub risk_service: Arc<dyn RiskService>,
        pub trade_lock: Arc<TradeLock>,
        pub gateway: Arc<MockApiGateway>,
        pub state_center: Arc<dyn StateCenter>,
        pub heartbeat_token: HeartbeatToken,
    }

6.3 自循环逻辑

    impl StrategyService for H15mStrategyService {
        async fn run(self: Arc<Self>, shutdown_rx: mpsc::Receiver<()>) {
            tracing::info!("[{}] H15mStrategyService 启动", self.component_id);

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.recv() => {
                        self.state_center.report_alive(&self.component_id).await;
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        self.run_one_cycle().await;
                    }
                }
            }

            tracing::info!("[{}] H15mStrategyService 停止", self.component_id);
        }
    }

    impl H15mStrategyService {
        async fn run_one_cycle(&self) {
            // 1. 读取 K 线
            let kline = match self.shared_store.get_kline(&self.symbol).await {
                Some(k) => k,
                None => return,  // 无数据，空转
            };

            // 2. 触发指标计算（事件驱动）
            let indicators = self.indicator_store.get_min(&self.symbol).await;

            // 3. 策略决策
            let decision = self.trader.decide(indicators.as_ref()).await;

            // 4. 决策分支处理
            if let Some(order) = decision.to_order() {
                self.execute_with_risk_control(order).await;
            }

            // 5. 上报存活（每次循环结束）
            let _ = self.state_center.report_alive(&self.component_id).await;
        }

        async fn execute_with_risk_control(&self, order: Order) {
            let request = RiskCheckRequest {
                symbol: self.symbol.clone(),
                side: order.side.clone(),
                qty: order.qty,
                price: order.price,
                strategy_id: self.component_id.clone(),
            };

            // Stage 1: 预检
            let pre_result = self.risk_service.pre_check(&request).await;
            if !pre_result.approved {
                tracing::debug!("[{}] 预检拒绝: {:?}", self.component_id, pre_result.reason);
                return;
            }

            // 获取锁
            let guard = match self.trade_lock.acquire(&self.component_id).await {
                Ok(g) => g,
                Err(LockError::AlreadyHeld(_)) => {
                    // 锁被占用，跳过本次
                    return;
                }
                Err(LockError::Degraded) => {
                    // 风控服务降级，禁止所有交易
                    self.state_center.report_error(&self.component_id, "RiskService degraded").await;
                    return;
                }
            };

            // Stage 2: 终检
            let final_result = self.risk_service.final_check(&request).await;
            if !final_result.approved {
                tracing::debug!("[{}] 终检拒绝: {:?}", self.component_id, final_result.reason);
                return;
            }

            // 执行订单
            let qty = final_result.adjusted_qty.unwrap_or(order.qty);
            let _ = self.gateway.place_order(order.with_qty(qty)).await;
        }
    }

================================================================================
七、SharedStore 详细设计
================================================================================

7.1 保留版本号机制

    #[derive(Debug, Clone)]
    pub struct KlineWithSeq {
        pub kline: Kline,
        pub seq: u64,          // 序列号，每次写入递增
        pub timestamp: DateTime<Utc>,
    }

    /// Store 返回时附带序列号
    #[derive(Debug, Clone)]
    pub struct StoreOutput<T> {
        pub data: T,
        pub seq: u64,
    }

7.2 SharedStore trait

    #[async_trait::async_trait]
    pub trait SharedStore: Send + Sync {
        /// 写入 K 线（同步，无版本号问题）
        async fn write_kline(&self, symbol: &str, kline: Kline) -> u64;  // 返回 seq

        /// 读取最新 K 线（附带序列号）
        async fn get_kline(&self, symbol: &str) -> Option<KlineWithSeq>;

        /// 读取历史 K 线
        async fn get_history(&self, symbol: &str, limit: usize) -> Vec<KlineWithSeq>;

        /// 读取指定序列号之后的 K 线（用于增量处理）
        async fn get_since(&self, symbol: &str, min_seq: u64) -> Vec<KlineWithSeq>;

        /// 获取所有 symbol 列表
        async fn get_all_symbols(&self) -> Vec<String>;
    }

7.3 读写流程

    DataSvc 写入:
      1. self.seq.fetch_add(1, SeqCst)  // 获取序列号
      2. self.klines.insert(symbol, KlineWithSeq { kline, seq, timestamp })

    策略读取:
      1. current_seq = self.get_kline(symbol)?.seq
      2. self.get_since(symbol, current_seq)  // 读取所有未处理的 K 线
      3. 逐根处理，更新 internal_seq

注意: 策略协程内部维护自己的 internal_seq，每次处理只读 seq > internal_seq 的数据。

================================================================================
八、EngineManager 详细设计
================================================================================

8.1 职责

  - spawn_strategy: 启动策略协程
  - restart_policy: 监听 StateCenter，自动重启异常组件
  - shutdown_all: 优雅关闭所有协程
  - query_health: 查询组件健康状态

8.2 结构

    pub struct EngineManager {
        config: EngineConfig,
        state_center: Arc<dyn StateCenter>,
        /// 所有策略协程句柄
        handles: RwLock<HashMap<String, StrategyHandle>>,
        /// shutdown 信号
        shutdown_tx: broadcast::Sender<()>,
    }

    pub struct StrategyHandle {
        pub component_id: String,
        pub symbol: String,
        pub join_handle: JoinHandle<()>,
        pub shutdown_tx: mpsc::Sender<()>,
        /// 重试计数
        retry_count: AtomicU64,
        /// 是否活跃
        active: AtomicBool,
    }

8.3 重启策略

    最大重启次数: 无限
    重启间隔: 指数退避 (1s, 2s, 4s, 8s, 16s, 32s, 60s)
    触发条件: 仅可恢复错误

    impl EngineManager {
        /// 启动策略协程
        pub async fn spawn(&self, service: Arc<dyn StrategyService>) -> Result<(), EngineError> {
            let component_id = service.component_id().to_string();
            let symbol = service.symbol().to_string();
            let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

            let handle = tokio::spawn({
                let service = service.clone();
                let state_center = self.state_center.clone();
                async move {
                    let s = service.clone();
                    let rx = shutdown_rx;
                    let id = s.component_id().to_string();
                    s.run(rx).await;
                    // 正常退出，上报停止
                    let _ = state_center.report_alive(&id).await;
                }
            });

            let strategy_handle = StrategyHandle {
                component_id,
                symbol,
                join_handle: handle,
                shutdown_tx,
                retry_count: AtomicU64::new(0),
                active: AtomicBool::new(true),
            };

            self.handles.write().insert(component_id.clone(), strategy_handle);
            Ok(())
        }

        /// 监听 StateCenter，自动重启异常组件
        pub async fn run_restart_loop(&self) {
            let stale_threshold_secs = 30i64;

            loop {
                tokio::select! {
                    _ = self.shutdown_tx.subscribe().recv() => break,
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {
                        let stale = self.state_center.get_stale(stale_threshold_secs).await;
                        for component_id in stale {
                            self.handle_stale(&component_id).await;
                        }
                    }
                }
            }
        }

        async fn handle_stale(&self, component_id: &str) {
            let handle = self.handles.read().get(component_id).cloned();

            if let Some(h) = handle {
                // 指数退避
                let delay = min(60, 2_i64.pow(h.retry_count.load(Ordering::SeqCst))) as u64;
                h.retry_count.fetch_add(1, Ordering::SeqCst);

                tracing::warn!("[Engine] {} 心跳超时，{}s 后重启", component_id, delay);
                tokio::time::sleep(Duration::from_secs(delay)).await;

                // 重新检查是否仍然 Stale
                if let Some(state) = self.state_center.get(component_id).await {
                    if state.status == ComponentStatus::Stale && h.active.load(Ordering::SeqCst) {
                        self.respawn(component_id).await;
                    }
                }
            }
        }
    }

================================================================================
九、main.rs 重构
================================================================================

目标: main.rs 简化为 50 行以内，纯启动引导，无业务逻辑。

9.1 重构后的 main.rs 结构

    #[tokio::main]
    async fn main() -> Result<(), Box<dyn std::error::Error>> {
        // 1. 初始化 tracing
        init_tracing();

        // 2. 创建共享组件
        let (state_center, shared_store) = create_shared_components().await?;

        // 3. 创建业务组件
        let (data_svc, indicator_svc, risk_svc) = create_services(
            &shared_store,
            &state_center
        ).await?;

        // 4. 创建 EngineManager 并 spawn 所有策略
        let engine = EngineManager::new(
            config.clone(),
            state_center.clone(),
        );
        engine.spawn(data_svc).await?;
        engine.spawn(indicator_svc).await?;

        // 5. 主循环: 心跳监控
        run_monitor_loop(&engine, &state_center).await;

        Ok(())
    }

9.2 启动流程

    main.rs:
      1. init_tracing()
      2. create_shared_components()
         - StateCenter::new()
         - SharedStore::new()
         - TradeLock::new()
      3. create_services()
         - Kline1mService::new()        [b]
         - MinIndicatorService::new()   [c]
         - DayIndicatorService::new()    [c]
         - RiskService::new()            [e]
         - H15mStrategyService::new()   [d]
         - H1mStrategyService::new()    [d] (未来)
      4. engine.spawn() 所有服务
      5. run_monitor_loop()

================================================================================
十、迁移路径
================================================================================

10.1 第一阶段: StateCenter 先行

改动:
  - x_data/src/state: 新增 ComponentState + StateCenter trait + 实现
  - 各组件新增 state_center: Arc<dyn StateCenter> 依赖
  - main.rs: 创建 StateCenter，传递给各组件
  - 各组件: 自循环末尾调用 report_alive()

验证:
  - 现有 main.rs 流水线正常运行
  - StateCenter 能收到所有组件的心跳
  - get_stale() 能检测超时组件

10.2 第二阶段: 风控服务抽取

改动:
  - e_risk_monitor: 抽取 RiskService trait + TradeLock
  - main.rs: 创建 RiskService 实例，传递给策略协程
  - 策略协程: 新增两阶段风控调用

验证:
  - 风控两阶段调用正常
  - TradeLock 锁机制生效
  - 预检能快速拒绝无效请求

10.3 第三阶段: 指标层改造

改动:
  - c_data_process: 改造为 MinIndicatorService（日触发）+ DayIndicatorService（串行）
  - IndicatorStore trait 统一访问接口
  - 策略协程: 改为 request_indicators() 按需触发

验证:
  - 指标计算正常
  - 策略协程能读到指标
  - 日线指标串行批量计算正常

10.4 第四阶段: 数据层自运行

改动:
  - b_data_mock: Kline1mSvc 独立 spawn
  - main.rs: tokio::spawn DataSvc
  - SharedStore 替换 pipeline_store

验证:
  - DataSvc 正常回放数据
  - SharedStore 序列号机制生效

10.5 第五阶段: 策略协程自治 + EngineManager

改动:
  - d_checktable: Trader 改造为 H15mStrategyService
  - f_engine: EngineManager 实现
  - main.rs: 简化为启动引导

验证:
  - 策略协程自循环运行
  - EngineManager 重启策略正常
  - main.rs < 50 行

================================================================================
十一、验收标准
================================================================================

功能验收:
  [ ] main.rs < 50 行，无业务流水线逻辑
  [ ] StateCenter 轻量心跳上报正常
  [ ] 风控两阶段检查生效
  [ ] MinIndicatorService 事件触发计算正常
  [ ] DayIndicatorService 串行批量计算正常
  [ ] 策略协程自循环运行
  [ ] EngineManager 能监控和重启策略协程
  [ ] feature flag 切换实盘/沙盒

非功能验收:
  [ ] cargo check 零警告
  [ ] cargo test 全通过
  [ ] cargo clippy 全通过

================================================================================
十二、文件变更清单
================================================================================

12.1 新增文件

    crates/x_data/src/state/component.rs    # ComponentState
    crates/x_data/src/state/center.rs       # StateCenter trait + 实现
    crates/b_data_mock/src/service/          # Kline1mSvc
    crates/b_data_source/src/service/         # 实盘版本
    crates/c_data_process/src/service/        # MinSvc + DaySvc
    crates/c_data_process/src/traits.rs       # IndicatorStore trait
    crates/e_risk_monitor/src/risk_service.rs # RiskService trait + TradeLock
    crates/d_checktable/src/h_15m/strategy_service.rs # H15mStrategyService
    crates/f_engine/src/engine_manager.rs      # EngineManager

12.2 修改文件

    crates/x_data/src/state/mod.rs    # 导出
    crates/e_risk_monitor/src/lib.rs  # 导出 RiskService
    crates/f_engine/src/lib.rs        # 导出 EngineManager
    src/main.rs                       # 重构为启动引导

12.3 保留文件（逐步废弃）

    旧流水线代码保留直到迁移完成

================================================================================
十三、附录
================================================================================

13.1 与 v1.0 的核心差异

    v1.0                              v2.0
    =================================  =================================
    所有组件自循环                      IndicatorService 事件触发
    StateCenter 实时上报                StateCenter 心跳式轻量上报
    风控内嵌策略协程                    风控独立全局串行服务
    无版本号机制                        SharedStore 保留 seq 版本号
    EngineManager 职责模糊              EngineManager 专注生命周期管理

13.2 术语表

    TradeLock:      全局交易锁，确保同时只有一个协程在交易
    两阶段检查:      Stage1 预检(锁获取前) + Stage2 终检(锁获取后)
    心跳上报:        组件存活状态定期上报到 StateCenter
    事件触发:        策略协程主动请求时触发计算，非主动轮询
    串行批量:        低频串行处理多 symbol，适用于日线指标
