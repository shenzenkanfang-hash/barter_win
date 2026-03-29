================================================================================
异步协程自循环架构方案（优化版）
================================================================================
生成时间: 2026-03-30
更新: 优化时序同步 + 观测表设计 + 实施顺序
范围: main.rs 重构

================================================================================
一、现状问题诊断
================================================================================

根本原因：NO_SIGNAL_INPUT 永不消失

数据流断链:

    Kline1mStream::next_message()
        self.store.write_kline()  写入 Kline1mStream 自己的 store
        自身 Arc<b_data_mock::store::MarketDataStoreImpl>

    Trader::execute_once_wal()
        self.store.get_current_kline()  读取传入的 shared_store
        shared_store Arc<b_data_source::store::MarketDataStoreImpl（永远是空的）

两个 store 是不同的 Rust trait（b_data_mock::store::MarketDataStore vs
b_data_source::store::MarketDataStore），即便方法是同一个也无法自动转换。
Kline1mStream 自己的 self.store 从未被注入的 shared_store 赋值。

================================================================================
二、目标架构（生产者-协程模型）
================================================================================

                        main.rs 入口
  1. 初始化 heartbeat
  2. 创建共享组件（shared_store, gateway, signal_processor 等）
  3. 创建 Kline1mStream 并注入 shared_store
  4. tokio::spawn { Kline1mStream 数据生产者循环 }
  5. tokio::spawn { SignalProcessor 指标生产者循环 }
  6. tokio::spawn { 每品种一个 StrategyCoroutine }
  7. 主循环: 心跳监控 + shutdown 信号

        生产者 A               生产者 B              引擎管理
    Kline1mStream        SignalProcessor       Strategy Coroutine
    [spawn]              [spawn]              [spawn per symbol]

    loop {               loop {               loop {
      next_msg()           read_store()          store.get_kline()
      store.write()         compute()             signal_processor
      notify(ready)        notify(ready)         decide()
      sleep(50ms)          sleep(60s)            risk_check()
    }                    }                      order()
                                                sleep(100ms)
                                              }

        所有读写都走这里
        共享 StoreRef
        b_data_source impl

与 v5.5 的本质区别:

v5.5 (tick-driven)            目标 (async self-loop)
--------------------------------------------------------------------------------
单循环按 tick 串行调用各 stage   每个组件独立 self-loop，并发运行
main loop 手动驱动数据传递       通过共享 store 自动传递
引擎角色: 驱动每个 tick         引擎只管理协程 spawn/shutdown/monitor
指标更新: 被 d 调用              生产者自循环，缓存到 store
心跳: 手动间隔上报              每组件协程内自动报到

================================================================================
三、关键风险分析
================================================================================

风险1：时序竞争

    Kline1mStream 每 50ms 写入
    SignalProcessor 每 60s 读取计算
    策略协程每 100ms 读取

    问题：策略可能读到旧指标（SignalProcessor 还没算完）

风险2：数据一致性

    生产者A写入 K线
    生产者B读取计算指标（基于旧K线？）
    策略读取指标决策

    问题：三者看到的数据可能不是同一时刻的

风险3：缓存失效

    SignalProcessor 的 cache 是死代码
    方案说废弃 cache，直接查询

    问题：每次查询都重新计算？性能如何？

风险4：协程失败感知

    引擎如何感知协程失败？
    需要错误上报机制

================================================================================
四、时序同步机制
================================================================================

推荐方案C（快照读取）：策略协程读取时，一次性读取 K线+指标+信号 的当前快照，
即使不是最新，也保证三者是一致的。

实现方式：版本号机制

    Store 中每个数据带 version/timestamp

    PipelineState {
        data_version: u64,       // K线数据版本
        indicator_version: u64,   // 指标版本
        signal_version: u64,     // 信号版本
        last_kline_time: i64,   // 最新K线时间戳
        last_indicator_time: i64, // 最新指标时间戳
    }

    策略协程读取时：
    1. 检查 data_version == indicator_version（指标基于最新K线？）
    2. 如果落后，等待一次 SignalProcessor 通知
    3. 如果差距过大（> 5s），记录告警日志

================================================================================
五、观测表设计（Store PipelineState）
================================================================================

Store 不仅要存 K线，还要存指标快照、信号状态、订单状态。
每个协程更新时，写入自己的阶段状态+时间戳。
引擎通过查询 Store，拿到全链路时间线。

数据流观测表（Store 中记录）:

| 时间戳 | 阶段 | 输入 | 输出 | 状态 |
|--------|------|------|------|------|
| T0 | K线到达 | 原始报价 | K线数据 | 已写入 Store |
| T1 | 指标计算 | K线数据 | EMA/RSI/波动率 | 已更新 Store |
| T2 | 信号生成 | 指标值 | 交易信号 | 已记录 Store |
| T3 | 策略决策 | 信号+持仓 | 交易指令 | 已决策 Store |
| T4 | 风控检查 | 指令+账户 | 通过/拒绝 | 已检查 Store |
| T5 | 订单执行 | 合规指令 | 成交回报 | 已成交 Store |
| T6 | 状态更新 | 成交结果 | 新持仓/资金 | 已更新 Store |


Store 统一状态定义:

    // Store 中增加全流程状态追踪
    pub struct TradingPipelineState {
        pub symbol: String,
        pub last_update: Timestamp,

        // 各阶段状态
        pub data_stage: DataStage,           // K线数据阶段
        pub indicator_stage: IndicatorStage,  // 指标计算阶段
        pub signal_stage: SignalStage,        // 信号生成阶段
        pub decision_stage: DecisionStage,     // 策略决策阶段
        pub risk_stage: RiskStage,            // 风控检查阶段
        pub execution_stage: ExecutionStage,   // 订单执行阶段

        // 版本号（用于时序一致性）
        pub data_version: u64,
        pub indicator_version: u64,
        pub signal_version: u64,

        // 完整链路日志
        pub pipeline_log: Vec<PipelineEvent>,
    }


引擎查询接口:

    pub fn get_pipeline_state(&self, symbol: &str) -> Option<TradingPipelineState>;
    pub fn get_full_pipeline_log(&self, symbol: &str) -> Vec<PipelineEvent>;
    pub fn get_stage_version(&self, symbol: &str) -> StageVersions;


PipelineStage 枚举定义:

    pub enum PipelineStage {
        DataReceived,       // K线已到达
        DataWritten,       // K线已写入Store
        IndicatorComputed,  // 指标已计算
        SignalGenerated,   // 信号已生成
        DecisionMade,      // 决策已完成
        RiskChecked,       // 风控已检查
        OrderSubmitted,     // 订单已提交
        OrderFilled,       // 订单已成交
        PositionUpdated,    // 持仓已更新
    }


PipelineEvent 日志条目:

    pub struct PipelineEvent {
        pub timestamp: i64,
        pub stage: PipelineStage,
        pub input_hash: u64,   // 输入数据哈希（用于回溯）
        pub output_hash: u64,  // 输出数据哈希
        pub duration_ms: u64,  // 本阶段耗时
        pub metadata: HashMap<String, String>,
    }

================================================================================
六、实施计划（分步，优化顺序）
================================================================================

步骤0: 修复 Store 注入 Bug（先止血）

文件: crates/b_data_mock/src/ws/kline_1m/ws.rs

问题: from_klines_with_store 接受 shared_store 参数但从未赋值给 self.store。

修复方案: self.store = shared_store（传入即赋值）
需要 Kline1mStream 的 store 字段支持 dyn MarketDataStore trait

验证: cargo run 不再出现 NO_SIGNAL_INPUT，Trader 能读到 K 线数据


步骤1: 统一 Store Trait（解决两个 trait 不兼容）

问题: b_data_mock::store::MarketDataStore 和 b_data_source::store::MarketDataStore
是两个不同的 Rust trait。

推荐方案: b_data_mock/Cargo.toml 添加 b_data_source 依赖，
然后 b_data_mock 的 MarketDataStoreImpl 同时 impl b_data_source::MarketDataStore，
这样 Arc::downcast 可以工作。


步骤2: 添加观测表结构（先能看，再改架构）

文件: b_data_source/src/store/pipeline_state.rs（新增）

    新增 PipelineState 结构
    新增 PipelineStage 枚举
    新增 PipelineEvent 结构
    MarketDataStoreImpl 增加 write_pipeline_event()
    MarketDataStoreImpl 增加 get_pipeline_state()

验证: Kline1mStream 写入后，能从 Store 读出完整链路


步骤3: 改为自循环架构（再改架构）

文件: src/main.rs（重写）

新 main.rs 结构:

#[tokio::main]
async fn main() {
    // 1. init tracing + heartbeat

    // 2. 创建共享组件（所有 spawn 共享同一个 Arc）
    //    shared_store: StoreRef
    //    gateway: Arc<MockApiGateway>
    //    signal_processor: Arc<SignalProcessor>
    //    risk_checker: Arc<RiskPreChecker>
    //    order_checker: Arc<OrderCheck>

    // 3. Kline1mStream 注入 shared_store

    // 4. spawn 生产者 A: Kline1mStream 数据自循环
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(50));
        loop {
            interval.tick().await;
            if let Some(msg) = stream.next_message() {
                // Kline1mStream 内部已经写入了 shared_store
                // 写入 PipelineState.data_stage
                store.write_pipeline_event(PipelineEvent {
                    stage: PipelineStage::DataWritten,
                    ...
                });
            }
        }
    });

    // 5. spawn 生产者 B: SignalProcessor 指标自循环
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            // 读取 shared_store 最新 K线
            // 调用 min_update() 更新内部 Indicator1m
            // 写入 PipelineState.indicator_stage
        }
    });

    // 6. spawn 策略协程（每品种一个）
    let strategy_handle = tokio::spawn(run_strategy_coroutine(
        SYMBOL.to_string(),
        shared_store.clone(),
        signal_processor.clone(),
        risk_checker.clone(),
        order_checker.clone(),
        gateway.clone(),
    ));

    // 7. 主循环：心跳监控 + shutdown 等待
    let hb_handle = tokio::spawn(heartbeat_monitor_loop());

    // shutdown 逻辑...
    // 输出 heartbeat report
}

async fn run_strategy_coroutine(
    symbol: String,
    store: StoreRef,
    signal_processor: Arc<SignalProcessor>,
    risk_checker: Arc<RiskPreChecker>,
    order_checker: Arc<OrderCheck>,
    gateway: Arc<MockApiGateway>,
) {
    // 创建 Trader（使用传入的 store）
    let trader = create_trader(store);

    // 时序一致性检查
    let wait_for_indicators = || async {
        let state = store.get_pipeline_state(&symbol)?;
        let now = chrono::Utc::now().timestamp_millis();
        // 如果指标版本落后超过 5s，等待一次指标计算
        if now - state.indicator_timestamp > 5000 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Some(())
    };

    loop {
        // 时序同步：等待指标就绪
        if wait_for_indicators().await.is_none() {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        match trader.execute_once_wal().await {
            Ok(ExecutionResult::Executed { qty, order_type }) => {
                // 写入 PipelineState.decision_stage
                // 风控 下单
                // 写入 PipelineState.execution_stage
            }
            Ok(ExecutionResult::Skipped(reason)) => {
                tracing::trace!("[{}] skip: {}", symbol, reason);
            }
            Ok(ExecutionResult::Failed(e)) => {
                tracing::warn!("[{}] failed: {}", symbol, e);
                // 写入 PipelineState.error_stage
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}


步骤4: 心跳报到迁移

当前 execute_once_wal 内部调用 heartbeat_report()，迁移后心跳报到点不变，
在协程自循环中继续保持。


步骤5: 错误上报机制

协程失败时，发送通知到引擎：

    // 策略协程内部
    if let Err(e) = trader.execute_once_wal().await {
        // 写入 PipelineState.error_stage
        // 通知引擎（通过 channel 或 shared state）
    }

    // 引擎主循环
    tokio::select! {
        err = error_rx.recv() => {
            tracing::error!("[Engine] 协程异常: {:?}", err);
            // 决定是否重启协程
        }
    }

================================================================================
七、影响范围
================================================================================

文件                                改动                          风险
--------------------------------------------------------------------------------
crates/b_data_mock/src/ws/         store 注入修复               低
kline_1m/ws.rs

crates/b_data_mock/src/store/      impl b_data_source::          低
store_impl.rs                      MarketDataStore

crates/b_data_mock/Cargo.toml      添加 b_data_source 依赖       低

b_data_source/src/store/           新增 PipelineState           低
pipeline_state.rs                  结构

crates/d_checktable/src/h_15m/    无改动                        -
trader.rs

src/main.rs                        完全重写                      高

crates/c_data_process/src/        缓存激活（可选）              中
processor.rs

================================================================================
八、验证方案
================================================================================

步骤0后验证: cargo run 不再出现 NO_SIGNAL_INPUT，Trader 能读到 K 线数据

步骤2后验证: PipelineState 能记录完整链路，从 Store 能读出各阶段状态

步骤3后验证: 多个 tokio::spawn 并发运行，心跳报告显示多协程活跃

整体验证: 回放 HOTUSDT 数据，交易日志与 Python 原版对齐

================================================================================
