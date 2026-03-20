================================================================================
数据流与存储架构文档
================================================================================
Author: 产品经理
Created: 2026-03-20
Stage: documentation
Status: 初稿
================================================================================

一、数据架构总览
================================================================================

1.1 数据分类
--------------------------------------------------------------------------------

系统中的数据分为两大类：

| 分类 | 说明 | 存储方式 | 生命周期 |
|------|------|----------|----------|
| 运行时数据 | 交易过程中产生的临时数据 | 内存 (HashMap/Vec) | 程序运行期间 |
| 持久化数据 | 需要长期保存的重要记录 | SQLite/CSV | 永久存储 |

运行时数据（按层级）:
- 市场数据层: Tick、K线、订单簿
- 指标层: EMA、RSI、PineColor、PricePosition、BigCycleCalculator
- 策略层: Signal、TradingMode
- 引擎层: Account、Position、Order、Trade、PipelineForm、CheckTable

持久化数据（按用途）:
- 账户快照: account_snapshots 表
- 持仓记录: exchange_positions、local_positions 表
- 通道事件: channel_events 表
- 风控事件: risk_events 表
- 指标事件: indicator_events 表
- 指标对比: CSV 文件

1.2 数据流总图
--------------------------------------------------------------------------------

                    ┌─────────────────────────────────────────────────────────────┐
                    │                      市场数据层 (market)                   │
                    │                                                             │
                    │   WebSocket ──► Tick ──► K线合成 ──► 订单簿更新           │
                    │                           │                               │
                    │                    产出: Tick, KLine                     │
                    └─────────────────────────────────────────────────────────────┘
                                               │
                                               ▼
                    ┌─────────────────────────────────────────────────────────────┐
                    │                      指标层 (indicator)                     │
                    │                                                             │
                    │   增量计算: EMA, RSI, PineColor, TR Ratio, PricePosition  │
                    │                    BigCycleCalculator                       │
                    │                           │                               │
                    │                    产出: 指标快照                            │
                    └─────────────────────────────────────────────────────────────┘
                                               │
                                               ▼
                    ┌─────────────────────────────────────────────────────────────┐
                    │                      策略层 (strategy)                        │
                    │                                                             │
                    │   VolatilityChannel ──► 策略判断 ──► Signal                 │
                    │                           │                               │
                    │                    产出: Signal, PipelineForm                │
                    └─────────────────────────────────────────────────────────────┘
                                               │
                                               ▼
                    ┌─────────────────────────────────────────────────────────────┐
                    │                      引擎层 (engine)                          │
                    │                                                             │
                    │   Signal ──► 风控预检 ──► 订单执行 ──► 持仓更新            │
                    │                           │                               │
                    │              产出: Order, Trade, Position, Account          │
                    │              内存: AccountPool, StrategyPool, Position       │
                    └─────────────────────────────────────────────────────────────┘
                                               │
                                               ▼
                    ┌─────────────────────────────────────────────────────────────┐
                    │                    持久化层 (persistence)                   │
                    │                                                             │
                    │   EventRecorder ──► SQLite (6张表) ──► CSV (指标对比)       │
                    └─────────────────────────────────────────────────────────────┘

================================================================================

二、市场数据层 (market crate)
================================================================================

2.1 职责
--------------------------------------------------------------------------------
- 接收WebSocket原始数据
- 增量更新K线（1m/15m/1d）
- 维护订单簿深度
- 产出Tick、KLine给下游

2.2 核心数据结构
--------------------------------------------------------------------------------

【Tick - 原始行情】
struct Tick {
    symbol:     String,           // 交易对 "BTCUSDT"
    price:      Decimal,          // 最新价格
    qty:        Decimal,          // 成交量
    timestamp:  DateTime<Utc>,    // 时间戳
}

【KLine - K线数据】
struct KLine {
    symbol:     String,           // 交易对
    period:     Period,            // 周期 (Minute(1), Minute(15), Day)
    open:       Decimal,           // 开盘价
    high:       Decimal,           // 最高价
    low:        Decimal,           // 最低价
    close:      Decimal,          // 收盘价
    volume:     Decimal,           // 成交量
    timestamp:  DateTime<Utc>,    // K线时间
}

【Period - 周期枚举】
enum Period {
    Minute(u8),  // 分钟周期，如 Minute(1), Minute(15)
    Day,         // 日线
}

【VolatilityStats - 波动率统计】
struct VolatilityStats {
    is_high_volatility: bool,  // 是否高波动
    vol_1m: Decimal,          // 1分钟波动率
    vol_15m: Decimal,          // 15分钟波动率
}

2.3 模块结构
--------------------------------------------------------------------------------
market/
├── types.rs              # Tick, KLine, Period, VolatilityStats
├── kline.rs              # KLineSynthesizer - K线合成器
├── orderbook.rs           # OrderBook - 订单簿
├── volatility.rs          # VolatilityDetector - 波动率检测
├── websocket.rs           # WebSocket连接器
├── binance_ws.rs          # Binance WebSocket
├── data_feeder.rs         # DataFeeder - 数据馈送
├── kline_persistence.rs   # KlinePersistence - K线持久化
└── symbol_registry.rs     # SymbolRegistry - 品种注册

2.4 关键数据流
--------------------------------------------------------------------------------
WebSocket接收 Tick
    │
    ├──► KLineSynthesizer.update(tick) ──► K线增量更新
    │                                      产出: completed_kline (Option<KLine>)
    │
    └──► VolatilityDetector.check(tick) ──► 波动率检测
                                             产出: VolatilityStats

2.5 存储
--------------------------------------------------------------------------------
| 数据 | 存储位置 | 类型 | 说明 |
|------|----------|------|------|
| 当前K线 | KLineSynthesizer 内部 | 内存 | 仅保留当前K线 |
| K线历史 | KlinePersistence | SQLite | 可配置保留根数 |
| 订单簿 | OrderBook 内部 | 内存 | 实时更新 |

================================================================================

三、指标层 (indicator crate)
================================================================================

3.1 职责
--------------------------------------------------------------------------------
- 增量计算各类指标（O(1)复杂度）
- 无锁设计，高频路径零开销
- 提供TR、PineColor、PricePosition三大指标体系

3.2 核心数据结构
--------------------------------------------------------------------------------

【PineColor - Pine颜色】
enum PineColor {
    PureGreen,    // 纯绿 - 强势多头
    LightGreen,   // 淡绿 - 弱势多头
    PureRed,      // 纯红 - 强势空头
    LightRed,     // 淡红 - 弱势空头
    Purple,       // 紫色 - 超买超卖
    Neutral,      // 中性
}

判断逻辑:
| 颜色 | MACD条件 | RSI条件 |
|------|----------|---------|
| 纯绿 | MACD >= Signal && MACD >= 0 | - |
| 淡绿 | MACD <= Signal && MACD >= 0 | - |
| 纯红 | MACD <= Signal && MACD <= 0 | - |
| 淡红 | MACD >= Signal && MACD <= 0 | - |
| 紫色 | - | RSI >= 70 或 RSI <= 30 |

【BigCycleIndicators - 大周期指标】
struct BigCycleIndicators {
    tr_ratio_5d_20d:    Decimal,  // TR比率 5日/20日
    tr_ratio_20d_60d:   Decimal,  // TR比率 20日/60日
    pos_norm_20:        Decimal,  // 价格位置 20周期
    ma5_in_20d_pos:     Decimal,  // MA5在MA20中的位置
    ma20_in_60d_pos:    Decimal,  // MA20在MA60中的位置
    pine_color_20_50:   PineColor,  // Pine颜色 20/50
    pine_color_100_200: PineColor,  // Pine颜色 100/200
    pine_color_12_26:   PineColor,  // Pine颜色 12/26
}

3.3 模块结构
--------------------------------------------------------------------------------
indicator/
├── lib.rs                    # 导出入口
├── ema.rs                    # EMA (指数移动平均)
├── rsi.rs                    # RSI (相对强弱指数)
├── pine_color.rs             # Pine颜色检测
├── price_position.rs          # 价格位置
├── tr_ratio.rs               # TR比率
├── z_score.rs                # Z-Score
├── velocity.rs               # 速度指标
├── big_cycle.rs              # 大周期计算器
├── pine_indicator_full.rs    # 完整Pine指标
└── error.rs                  # 错误类型

3.4 关键数据流
--------------------------------------------------------------------------------
K线收盘 ──► EMA.calculate(close) ──► EMA快线/慢线
                │
                ▼
           MACD = EMA_fast - EMA_slow
                │
                ▼
           Signal = EMA_signal.calculate(MACD)
                │
                ▼
           Histogram = MACD - Signal
                │
                ▼
           PineColorDetector.detect(MAC, Signal, RSI)
                │
                ▼
           产出: PineColor, EMA值, RSI值

3.5 存储
--------------------------------------------------------------------------------
| 数据 | 存储位置 | 类型 | 说明 |
|------|----------|------|------|
| EMA状态 | EMA内部 | 内存 | 平滑因子，O(1)更新 |
| RSI状态 | RSI内部 | 内存 | 上涨/下跌平均值 |
| 价格历史 | PricePosition内部 | VecDeque | 固定窗口14个 |
| TR历史 | BigCycleCalculator内部 | VecDeque | 多周期TR |

注意: 指标数据全部在内存中计算，不持久化。重启后指标需重新warm-up。

================================================================================

四、策略层 (strategy crate)
================================================================================

4.1 职责
--------------------------------------------------------------------------------
- 接收Tick和指标数据
- 根据策略逻辑产生交易信号
- 管理策略状态机

4.2 核心数据结构
--------------------------------------------------------------------------------

【Signal - 交易信号】
enum Signal {
    LongEntry,      // 多头入场
    ShortEntry,     // 空头入场
    LongHedge,      // 多头对冲
    ShortHedge,     // 空头对冲
    LongExit,       // 多头平仓
    ShortExit,      // 空头平仓
    ExitHighVol,   // 高波动退出
}

【TradingMode - 交易模式】
enum TradingMode {
    Low,    // 低频模式 (日线)
    Medium, // 中频模式 (分钟)
    High,   // 高频模式 (Tick)
}

【OrderRequest - 订单请求】
struct OrderRequest {
    symbol:     String,       // 交易对
    side:       Side,         // 多/空
    order_type: OrderType,    // 市价/限价
    qty:        Decimal,      // 数量
    price:      Option<Decimal>,  // 限价
}

4.3 模块结构
--------------------------------------------------------------------------------
strategy/
├── lib.rs              # 导出入口
├── traits.rs           # Strategy trait
├── types.rs            # Signal, TradingMode, OrderRequest
├── trend_strategy.rs   # 趋势策略
└── pin_strategy.rs     # 马丁/插针策略

4.4 VolatilityChannel - 波动率通道
--------------------------------------------------------------------------------
VolatilityChannel 是策略层的核心组件，负责:
1. K线合成 (1m, 15m, 1d)
2. 指标计算 (EMA, RSI, PineColor, BigCycle)
3. 通道切换 (Slow <-> High)
4. Check表填充

struct VolatilityChannel {
    symbol:         String,              // 品种
    strategy_id:    StrategyId,          // 策略ID
    kline_1m:      KLineSynthesizer,    // 1分钟K线
    kline_15m:     KLineSynthesizer,    // 15分钟K线
    kline_1d:      KLineSynthesizer,    // 日线K线
    ema_fast:      EMA,                  // EMA12
    ema_slow:      EMA,                  // EMA26
    ema_100:       EMA,                  // EMA100 (日线)
    ema_200:       EMA,                  // EMA200 (日线)
    rsi:           RSI,                  // RSI14
    rsi_daily:     RSI,                  // RSI14 (日线)
    price_position:    PricePosition,     // 价格位置
    big_cycle:     BigCycleCalculator,   // 大周期计算器
    current_channel:  ChannelType,        // 当前通道
    check_table:   CheckTable,           // Check表
    round_guard:   Arc<RoundGuard>,      // 轮次守卫
}

通道切换条件:
| 通道 | 触发条件 | 计算频率 |
|------|----------|----------|
| Slow -> High | 1min波动率 >= 3% 或 15min波动率 >= 13% | Tick级 |
| High -> Slow | 波动率 < 阈值 | Tick级 |

4.5 存储
--------------------------------------------------------------------------------
| 数据 | 存储位置 | 类型 | 说明 |
|------|----------|------|------|
| 策略状态 | VolatilityChannel内部 | 内存 | 包含指标和K线 |
| Check表 | CheckTable | 内存 | FnvHashMap |
| 信号 | Signal枚举 | 内存 | 直接传递给引擎层 |

================================================================================

五、引擎层 (engine crate)
================================================================================

5.1 职责
--------------------------------------------------------------------------------
- 风控检查 (锁外预检 + 锁内复核)
- 订单执行 (MockBinanceGateway)
- 持仓管理 (LocalPositionManager)
- 账户管理 (AccountPool, StrategyPool)
- 事件记录 (EventRecorder)

5.2 核心数据结构
--------------------------------------------------------------------------------

【MockAccount - 模拟账户】
struct MockAccount {
    account_id:       String,    // 账户ID
    total_equity:     Decimal,   // 总权益
    available:        Decimal,   // 可用资金
    frozen_margin:    Decimal,   // 冻结保证金
    unrealized_pnl:   Decimal,   // 浮盈亏
    update_ts:        i64,      // 更新时间戳
}

【MockPosition - 模拟持仓】
struct MockPosition {
    symbol:            String,    // 品种
    long_qty:          Decimal,  // 多头数量
    long_avg_price:    Decimal,  // 多头均价
    short_qty:         Decimal,  // 空头数量
    short_avg_price:   Decimal,  // 空头均价
    unrealized_pnl:    Decimal,  // 浮盈亏
    margin_used:       Decimal,  // 已用保证金
}

【MockOrder - 模拟订单】
struct MockOrder {
    order_id:      String,       // 订单ID
    symbol:        String,       // 品种
    side:          Side,         // 多/空
    qty:           Decimal,       // 数量
    price:         Decimal,       // 价格
    order_type:    OrderType,    // 市价/限价
    status:        OrderStatus,   // 状态
    filled_qty:    Decimal,      // 已成交数量
    filled_price:  Decimal,      // 成交价格
    created_ts:    i64,          // 创建时间
    filled_ts:     Option<i64>,  // 成交时间
}

enum OrderStatus {
    Pending,    // 待成交
    Filled,     // 已成交
    Cancelled,  // 已取消
    Rejected,   // 已拒绝
}

【MockTrade - 成交记录】
struct MockTrade {
    trade_id:     String,    // 成交ID
    order_id:     String,    // 订单ID
    symbol:       String,    // 品种
    side:         Side,      // 多/空
    qty:          Decimal,   // 数量
    price:        Decimal,   // 价格
    commission:   Decimal,   // 手续费
    realized_pnl: Decimal,   // 已实现盈亏
    ts:           i64,      // 时间戳
}

【RiskConfig - 风控配置】
struct RiskConfig {
    max_position_ratio:       Decimal,  // 最大持仓比例 95%
    min_reserve_ratio:        Decimal,  // 最低保留比例 5%
    max_order_value_ratio:    Decimal,  // 最大订单金额比例 10%
    maintenance_margin_rate:   Decimal,  // 维持保证金率 0.5%
    order_frequency_limit:    u32,     // 订单频率限制 (次/秒)
    price_deviation_limit:    Decimal,  // 价格偏离限制 1%
}

【RejectReason - 拒绝原因】
enum RejectReason {
    InsufficientBalance,       // 余额不足
    PositionLimitExceeded,     // 持仓超限
    MarginInsufficient,        // 保证金不足
    PriceDeviationExceeded,    // 价格偏离超限
    SymbolNotTradable,         // 品种不可交易
    OrderFrequencyExceeded,    // 订单频率超限
    SystemError,              // 系统错误
}

5.3 模块结构
--------------------------------------------------------------------------------
engine/
├── lib.rs                      # 导出入口
├── engine.rs                   # TradingEngine 主引擎
├── mock_binance_gateway.rs      # MockBinanceGateway
├── risk.rs                     # RiskPreChecker
├── risk_rechecker.rs           # RiskReChecker
├── order.rs                    # OrderExecutor
├── order_check.rs              # OrderCheck
├── position_manager.rs          # LocalPositionManager
├── pnl_manager.rs              # PnlManager
├── account_pool.rs             # AccountPool
├── strategy_pool.rs             # StrategyPool
├── market_status.rs             # MarketStatusDetector
├── position_exclusion.rs        # PositionExclusionChecker
├── thresholds.rs               # ThresholdConstants
├── channel.rs                  # VolatilityChannel (策略层集成)
├── pipeline_form.rs             # PipelineForm
├── check_table.rs              # CheckTable
├── round_guard.rs              # RoundGuard
├── symbol_rules.rs             # SymbolRules
├── sqlite_persistence.rs        # SQLite持久化
└── persistence.rs              # PersistenceService

5.4 关键数据流
--------------------------------------------------------------------------------

【下单流程】
Signal ──► RiskPreChecker.pre_check() ──► GlobalLock.acquire()
                                              │
                                              ▼
                                    RiskReChecker.re_check()
                                              │
                                              ▼
                                    MockBinanceGateway.place_order()
                                              │
                                              ▼
                                    GlobalLock.release()

【持仓更新流程】
成交回报 ──► LocalPositionManager.update() ──► AccountPool.update()
                           │                          │
                           ▼                          ▼
                    持仓变化记录              账户余额更新
                           │                          │
                           ▼                          ▼
                    LocalPositionRecord         AccountSnapshotRecord
                           │                          │
                           └──────────┬───────────────┘
                                      ▼
                            EventRecorder.record()

5.5 存储
--------------------------------------------------------------------------------
| 数据 | 存储位置 | 类型 | 说明 |
|------|----------|------|------|
| 账户 | AccountPool | RwLock<FnvHashMap> | 多策略共享 |
| 持仓 | LocalPositionManager | RwLock | 策略私有 |
| 订单 | MockBinanceGateway | Vec | 内存缓存 |
| 风控配置 | RiskConfig | 内存 | 不可变 |

================================================================================

六、持久化层 (persistence crate)
================================================================================

6.1 职责
--------------------------------------------------------------------------------
- 记录重要事件到SQLite数据库
- 输出指标对比数据到CSV
- 提供事件回放能力

6.2 SQLite表结构
--------------------------------------------------------------------------------

【account_snapshots - 账户快照表】
CREATE TABLE account_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,              -- 时间戳
    account_id TEXT NOT NULL,         -- 账户ID
    total_equity TEXT NOT NULL,       -- 总权益 (Decimal->String)
    available TEXT NOT NULL,           -- 可用资金
    frozen_margin TEXT NOT NULL,      -- 冻结保证金
    unrealized_pnl TEXT NOT NULL,      -- 浮盈亏
    margin_ratio TEXT NOT NULL        -- 保证金率
);
-- 索引: idx_account_snapshots_ts

【exchange_positions - 交易所持仓表】
CREATE TABLE exchange_positions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,              -- 时间戳
    symbol TEXT NOT NULL,             -- 品种
    side TEXT NOT NULL,              -- "long" or "short"
    qty TEXT NOT NULL,               -- 数量
    avg_price TEXT NOT NULL,          -- 均价
    unrealized_pnl TEXT NOT NULL,     -- 浮盈亏
    margin_used TEXT NOT NULL        -- 已用保证金
);
-- 索引: idx_exchange_positions_ts

【local_positions - 本地仓位记录表】
CREATE TABLE local_positions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,              -- 时间戳
    symbol TEXT NOT NULL,             -- 品种
    strategy_id TEXT NOT NULL,       -- 策略ID
    direction TEXT NOT NULL,          -- "long" or "short"
    qty TEXT NOT NULL,               -- 数量
    avg_price TEXT NOT NULL,          -- 均价
    entry_ts INTEGER NOT NULL,       -- 入场时间戳
    remark TEXT NOT NULL DEFAULT ''   -- 备注
);
-- 索引: idx_local_positions_ts

【channel_events - 通道切换事件表】
CREATE TABLE channel_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,              -- 时间戳
    event TEXT NOT NULL,              -- 事件类型
    from_channel TEXT NOT NULL,       -- 原通道
    to_channel TEXT NOT NULL,         -- 新通道
    tr_ratio TEXT NOT NULL,           -- TR比率
    ma5_in_20d_pos TEXT NOT NULL,   -- MA5位置
    pine_color TEXT NOT NULL,         -- Pine颜色
    details TEXT NOT NULL DEFAULT ''  -- 详情
);
-- 索引: idx_channel_events_ts

事件类型:
- SLOW_TO_FAST: 慢速切高速
- FAST_TO_SLOW: 高速切慢速
- ENTER_FAST: 进入高速
- EXIT_FAST: 退出高速

【risk_events - 风控事件表】
CREATE TABLE risk_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,              -- 时间戳
    event_type TEXT NOT NULL,        -- 事件类型
    symbol TEXT NOT NULL,            -- 品种
    order_id TEXT NOT NULL,          -- 订单ID
    reason TEXT NOT NULL,            -- 拒绝原因
    available_before TEXT NOT NULL,   -- 操作前可用
    margin_ratio_before TEXT NOT NULL,  -- 操作前保证金率
    action_taken TEXT NOT NULL,      -- 采取的动作
    details TEXT NOT NULL DEFAULT ''  -- 详情
);
-- 索引: idx_risk_events_ts

事件类型:
- REJECT: 订单被拒绝
- LIQUIDATION: 强制平仓触发
- MARGIN_CALL: 追保通知

【indicator_events - 指标事件表】
CREATE TABLE indicator_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,              -- 时间戳
    symbol TEXT NOT NULL,            -- 品种
    event TEXT NOT NULL,             -- 事件类型
    tr_ratio_5d_20d TEXT NOT NULL, -- TR比率5/20
    tr_ratio_20d_60d TEXT NOT NULL, -- TR比率20/60
    pos_norm_20 TEXT NOT NULL,       -- 价格位置20
    ma5_in_20d_pos TEXT NOT NULL,   -- MA5在MA20位置
    ma20_in_60d_pos TEXT NOT NULL,  -- MA20在MA60位置
    pine_color_20_50 TEXT NOT NULL, -- Pine颜色20/50
    pine_color_100_200 TEXT NOT NULL, -- Pine颜色100/200
    pine_color_12_26 TEXT NOT NULL, -- Pine颜色12/26
    channel_type TEXT NOT NULL,     -- 通道类型
    details TEXT NOT NULL DEFAULT '' -- 详情
);
-- 索引: idx_indicator_events_ts

事件类型:
- TR_RATIO_BREAK: TR比率突破
- PINE_COLOR_CHANGE: Pine颜色变化
- ENTER_HIGH_VOL: 进入高波动
- EXIT_HIGH_VOL: 退出高波动

6.3 CSV输出
--------------------------------------------------------------------------------

【indicator_comparison.csv - 指标对比】
字段: timestamp, symbol, tr_ratio_5d_20d, tr_ratio_20d_60d, pos_norm_20,
      ma5_in_20d_pos, ma20_in_60d_pos, pine_color_20_50, pine_color_100_200,
      pine_color_12_26, vel_percentile, acc_percentile, power, channel_type

用途: 对比Rust和Python计算的指标差异

6.4 EventRecorder trait
--------------------------------------------------------------------------------
pub trait EventRecorder: Send + Sync {
    fn record_account_snapshot(&self, record: AccountSnapshotRecord);
    fn record_exchange_position(&self, record: ExchangePositionRecord);
    fn record_local_position(&self, record: LocalPositionRecord);
    fn record_channel_event(&self, record: ChannelEventRecord);
    fn record_risk_event(&self, record: RiskEventRecord);
    fn record_indicator_event(&self, record: IndicatorEventRecord);
}

实现:
- NoOpEventRecorder: 不记录任何事件（测试用）
- SqliteEventRecorder: 记录到SQLite（生产用）

6.5 存储总结
--------------------------------------------------------------------------------
| 数据 | 存储位置 | 持久化方式 | 说明 |
|------|----------|------------|------|
| 账户快照 | SQLite | account_snapshots | 定期快照 |
| 交易所持仓 | SQLite | exchange_positions | 实时更新 |
| 本地仓位 | SQLite | local_positions | 策略持仓记录 |
| 通道事件 | SQLite | channel_events | 切换时记录 |
| 风控事件 | SQLite | risk_events | 拒绝/强平时记录 |
| 指标事件 | SQLite | indicator_events | 重要变化时记录 |
| 指标对比 | CSV | indicator_comparison.csv | 批量输出 |

================================================================================

七、完整数据旅程
================================================================================

7.1 Tick到成交的完整数据流
--------------------------------------------------------------------------------

【阶段1: 市场数据接收】
WebSocket ──► Tick { symbol, price, qty, timestamp }
    │
    ├──► KLineSynthesizer::update(tick)
    │         │
    │         └─► K线合并，返回 completed_kline
    │
    └──► VolatilityChannel::on_tick(tick)
              │
              ├─► 更新 1m/15m/1d K线
              ├─► 计算 EMA, RSI, PineColor
              ├─► 更新 BigCycle 指标
              └─► 检查波动率，决定通道类型

【阶段2: 策略判断】
VolatilityChannel ──► PipelineForm
    │
    ├─► 判断信号 (LongEntry/ShortEntry/Exit...)
    ├─► 置信度计算
    └─► 填入 CheckTable

CheckTable ──► PositionDecision
    │
    └─► 互斥判断，决定可执行订单

【阶段3: 风控检查】
OrderRequest ──► RiskPreChecker::pre_check()
    │
    ├─► check_account_balance()    -- 余额检查
    ├─► check_position_ratio()     -- 持仓比例检查
    ├─► check_symbol_registered() -- 品种注册检查
    └─► check_volatility_mode()   -- 波动率模式检查

    ├─► 通过 ──► 继续
    └─► 不通过 ──► 返回 RejectReason，记录 risk_event

【阶段4: 订单执行】
GlobalLock::acquire()
    │
    ▼
RiskReChecker::re_check()  -- 锁内复核
    │
    ▼
MockBinanceGateway::place_order()
    │
    ├─► 市价单: 直接成交
    └─► 限价单: 挂单等待
    │
    ▼
OrderResult { status, filled_qty, filled_price }

GlobalLock::release()

【阶段5: 持仓更新】
成交回报 ──► LocalPositionManager::update_position()
    │
    ├─► 开多: long_qty += qty, long_avg_price 更新
    ├─► 开空: short_qty += qty, short_avg_price 更新
    ├─► 平多: long_qty -= qty, 计算已实现盈亏
    └─► 平空: short_qty -= qty, 计算已实现盈亏

【阶段6: 事件记录】
EventRecorder::record_*()
    │
    ├─► record_exchange_position()  ──► exchange_positions 表
    ├─► record_local_position()     ──► local_positions 表
    ├─► record_account_snapshot()    ──► account_snapshots 表
    └─► record_risk_event()         ──► risk_events 表 (如有)

7.2 数据形态变化
--------------------------------------------------------------------------------

| 阶段 | 数据形态 | 说明 |
|------|----------|------|
| Tick接收 | Tick | 原始行情数据 |
| K线合成 | KLine | 聚合后的价格数据 |
| 指标计算 | EMA/RSI/PineColor/BigCycle | 计算后的指标值 |
| 策略判断 | Signal + PipelineForm | 决策结果 |
| 风控检查 | OrderRequest + RejectReason | 风控结论 |
| 订单执行 | MockOrder + MockTrade | 订单和成交记录 |
| 持仓更新 | MockPosition + MockAccount | 更新后的状态 |
| 事件记录 | *Record | 持久化结构 |

7.3 内存数据 vs 持久化数据
--------------------------------------------------------------------------------

【仅存内存（程序重启丢失）】
- Tick流
- K线当前状态
- 指标计算状态 (EMA平滑值等)
- 策略状态机状态
- 通道状态
- CheckTable
- 订单簿

【持久化到SQLite】
- 账户快照 (定期)
- 持仓变化 (每次变化)
- 通道切换事件 (切换时)
- 风控事件 (拒绝/强平时)
- 指标事件 (重要变化时)

【输出到CSV】
- 指标对比数据 (批量)

================================================================================

八、数据存储设计决策
================================================================================

8.1 为什么用SQLite而不是Redis?
--------------------------------------------------------------------------------
| 对比项 | Redis (Go版本) | SQLite (Rust版本) |
|--------|----------------|-------------------|
| 部署 | 需单独服务 | 嵌入式，无需额外进程 |
| 依赖 | 需要Redis服务 | 无外部依赖 |
| 数据量 | 适合高频写入 | 适合低频事件记录 |
| 用途 | 热数据缓存、分布式锁 | 事件持久化、审计日志 |
| 性能 | 极高 (>100k ops/s) | 足够 (<10k writes/s) |

决策:
- 交易核心数据（账户/持仓）仍在内存，由parking_lot::RwLock保护
- SQLite仅用于记录重要事件，供回放和审计使用
- 移除了对Redis的强依赖，降低部署复杂度

8.2 为什么指标数据不持久化?
--------------------------------------------------------------------------------
- 指标数据来自实时行情，重启后可快速重新计算
- 指标状态（EMA平滑值等）是中间计算结果，无需持久化
- K线历史由KlinePersistence单独管理
- 重要指标变化通过indicator_events记录

8.3 数据保留策略
--------------------------------------------------------------------------------
| 数据类型 | 保留策略 | 原因 |
|----------|----------|------|
| 账户快照 | 定期覆盖 | 实时余额可从持仓计算 |
| 持仓记录 | 增量追加 | 需要审计轨迹 |
| 通道事件 | 增量追加 | 需要分析模式切换 |
| 风控事件 | 增量追加 | 需要分析拒绝原因 |
| 指标对比 | 批量追加 | 对比验证用 |

================================================================================

九、相关文档
================================================================================

- docs/2026-03-20-trading-system-rust-design.md - 完整架构设计
- docs/indicator-logic.md - 指标逻辑说明
- docs/architecture-reference.md - 技术栈参考
- docs/mock-binance-gateway-design.md - MockBinanceGateway设计

================================================================================
