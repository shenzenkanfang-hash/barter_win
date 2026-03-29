================================================================================
异步协程自循环架构方案
================================================================================
生成时间: 2026-03-30
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
      sleep(50ms)          sleep(60s)            decide()
    }                    }                      risk_check()
                                                order()
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
三、实施计划（分步）
================================================================================

步骤 1: 修复 Store 注入 Bug（最小改动，立即止血）

文件: crates/b_data_mock/src/ws/kline_1m/ws.rs

问题: from_klines_with_store 接受 shared_store 参数但从未赋值给 self.store。

修复方案: self.store = shared_store（传入即赋值）
需要 Kline1mStream 的 store 字段支持 dyn MarketDataStore trait


步骤 2: 统一 Store Trait（解决两个 trait 不兼容）

问题: b_data_mock::store::MarketDataStore 和 b_data_source::store::MarketDataStore
是两个不同的 Rust trait。

推荐方案: b_data_mock/Cargo.toml 添加 b_data_source 依赖，
然后 b_data_mock 的 MarketDataStoreImpl 同时 impl b_data_source::MarketDataStore，
这样 Arc::downcast 可以工作。


步骤 3: 重构 main.rs 为自循环架构

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
        loop {
            if let Some(msg) = stream.next_message() {
                // Kline1mStream 内部已经写入了 shared_store
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    // 5. spawn 生产者 B: SignalProcessor 指标自循环
    tokio::spawn(async move {
        loop {
            // 读取 shared_store 最新 K线
            // 调用 min_update() 更新内部 Indicator1m
            tokio::time::sleep(Duration::from_secs(60)).await;
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

    loop {
        match trader.execute_once_wal().await {
            Ok(ExecutionResult::Executed { qty, order_type }) => {
                // 风控 下单
            }
            Ok(ExecutionResult::Skipped(reason)) => {
                tracing::trace!("[{}] skip: {}", symbol, reason);
            }
            Ok(ExecutionResult::Failed(e)) => {
                tracing::warn!("[{}] failed: {}", symbol, e);
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}


步骤 4: SignalProcessor 缓存激活

问题: SignalProcessor 的 min_signal_cache 从未被写入，是死代码。
SignalProcessor.min_update() 直接计算指标但从不写 cache。

修复: 废弃 cache 机制，策略协程直接调用 SignalProcessor 的查询方法
获取最新指标。


步骤 5: 心跳报到迁移

当前 execute_once_wal 内部调用 heartbeat_report()，迁移后心跳报到点不变，
在协程自循环中继续保持。

================================================================================
四、影响范围
================================================================================

文件                                改动                          风险
--------------------------------------------------------------------------------
crates/b_data_mock/src/ws/         store 注入修复               低
kline_1m/ws.rs

crates/b_data_mock/src/store/      impl b_data_source::          低
store_impl.rs                      MarketDataStore

crates/b_data_mock/Cargo.toml      添加 b_data_source 依赖       低

crates/d_checktable/src/h_15m/     无改动                        -
trader.rs

src/main.rs                        完全重写                      高

crates/c_data_process/src/        缓存激活                      中
processor.rs

================================================================================
五、验证方案
================================================================================

步骤1后验证: cargo run 不再出现 NO_SIGNAL_INPUT，Trader 能读到 K 线数据

步骤3后验证: 多个 tokio::spawn 并发运行，心跳报告显示多协程活跃

整体验证: 回放 HOTUSDT 数据，交易日志与 Python 原版对齐

================================================================================
