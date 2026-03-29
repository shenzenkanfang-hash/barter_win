================================================================================
v5.3 Session 改动记录
================================================================================
生成时间: 2026-03-30
改动范围: src/main.rs

================================================================================
一、本次新增的重要逻辑（保留）
================================================================================

1. 【数据源】从随机模拟 → 历史 CSV
   - 旧: generate_mock_klines() 生成随机 K 线
   - 新: ReplaySource::from_csv(DATA_FILE) 加载 HOTUSDT 历史数据
   - 文件: D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv

2. 【K线解析】兼容两种 JSON 格式
   - Kline1mStream.next_message() 返回的 JSON 结构有外层包裹 {data: {...}}
   - 或直接返回 {t, T, s, i, o, c, h, l, v, x} 格式
   - 使用 serde_json::from_str().or_else() 兼容两种格式
   - 字段映射使用 #[serde(rename = "open")] 等显式指定

3. 【API 修复】StrategyEventTracker 方法签名变更
   - new(initial_balance: Decimal)            -- 需要传入初始余额
   - tick(tick: u64, timestamp, price)       -- 3参数
   - record_risk_check(ts, sig, passed, opt, map)  -- 5参数
   - record_filled(ts, id, price, qty, slip, comm)  -- 6参数
   - 必须在 b_data_mock::trading_event_tracker 路径下访问

4. 【Trader】create_trader() 改为同步
   - 旧: async fn create_trader() -> Result<Arc<Trader>, _> { ... .await? }
   - 新: fn create_trader() -> Result<Arc<Trader>, _> { ... Ok(trader) }
   - 原因: Trader::new() 本身是同步的，不需要 .await

5. 【配置】最大迭代次数提升
   - 旧: LOOP_ITERATIONS = 200
   - 新: 循环内硬编码 1000 次上限

6. 【性能】数据处理间隔
   - 旧: tokio::time::sleep(100ms)
   - 新: tokio::time::sleep(50ms)

7. 【Trader 交易逻辑】完整执行链路（Python 对齐）
   - execute_once_wal() 返回 ExecutionResult
   - Executed { qty, order_type } -> 风控检查 -> 订单检查 -> 模拟成交
   - Skipped(reason) -> trace 日志
   - Failed(e) -> warn 日志

8. 【心跳基础设施】保持不变
   - heartbeat::init() / generate_token() / set_heartbeat_token() 均已就绪
   - 各组件 set_heartbeat_token() 在 create_components() 时调用一次
   - print_heartbeat_report() 保持完整

================================================================================
二、本次回滚的内容（心跳传递）
================================================================================

1. SystemComponents 移除字段:
   - gateway: Arc<MockApiGateway>
   - event_tracker: Arc<Mutex<StrategyEventTracker>>

2. 移除 create_components 中的初始化:
   - MockApiGateway 创建
   - StrategyEventTracker 创建

3. 移除 run_full_test 中的:
   - event_tick 计数器
   - gateway.update_price() 调用
   - tracker.tick() 调用
   - tracker.record_risk_check() 调用
   - tracker.record_filled() 调用

4. 移除心跳分发逻辑（tokio::select 中的 heartbeat_tick 分支）

5. 移除未使用的 import:
   - chrono::Utc（心跳分发时才需要）

================================================================================
三、保留的 heartbeat 相关基础设施
================================================================================

以下内容在本次会话前已存在，本次未改动，保留原样：

1. a_common/heartbeat 模块
   - Config { stale_threshold, report_interval_secs, max_file_age_hours, max_file_size_mb }
   - Token { sequence, generated_at }
   - global() / generate_token() / init()
   - set_heartbeat_token() / get_heartbeat_token()
   - summary() / generate_report() / get_stale_points() / save_report()

2. 各组件心跳集成方法：
   - Kline1mStream::set_heartbeat_token()
   - SignalProcessor::set_heartbeat_token()
   - Trader::set_heartbeat_token()
   - RiskPreChecker::set_heartbeat_token()
   - OrderCheck::set_heartbeat_token()
   - next_message_with_heartbeat()
   - min_update_with_heartbeat()
   - execute_once_wal()
   - pre_check_with_heartbeat()

3. 监控点名:
   - BS-001: Kline1mStream
   - CP-001: SignalProcessor
   - DT-002: Trader
   - ER-001: RiskPreChecker
   - ER-003: OrderCheck

================================================================================
四、Python 对齐的交易逻辑（ThresholdConfig 默认值）
================================================================================

阈值配置（来自 ThresholdConfig::default()）：

平仓:
  - 盈利平仓: entry * 1.01 (1%)
  - 止损平仓: entry * 0.99 (1%)

对冲:
  - 多头对冲: price < entry * 0.98
  - 多头对冲硬阈值: price < entry * 0.90
  - 空头对冲: price > entry * 1.02
  - 空头对冲硬阈值: price > entry * 1.10

加仓:
  - 多头加仓: signal.long_entry AND price > entry * 1.02
  - 多头加仓硬阈值: price > entry * 1.08
  - 空头加仓: signal.short_entry AND price < entry * 0.98
  - 空头加仓硬阈值: price < entry * 0.92

仓位:
  - max_position: 0.15 (15%)
  - initial_ratio: 0.05 (5%)
  - add_multiplier: 1.5

波动率分层:
  - High: tr_base > 0.15 (15%)
  - Medium: tr_base > 0.05 (5%)
  - Low: tr_base <= 0.05

================================================================================
