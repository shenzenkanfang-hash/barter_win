================================================================================
策略层→引擎层 信号通信 规范文档
================================================================================

Author: Droid
Created: 2026-03-25
Status: Approved
Version: V1.1
Based on: V1.0

================================================================================
文档变更历史
================================================================================

V1.0 (2026-03-25):
  - 初始版本
  - 定义 StrategySignal 统一信号结构
  - 定义日线(l_1d)数量计算实现方案
  - 定义平仓功能传递方案

V1.1 (2026-03-25):
  - 新增第6章: 15分钟级策略(h_15m)完整实现方案
  - 新增第7章: Python旧策略 → Rust双周期适配映射表
  - 新增第8章: 双周期统一整改步骤（合并日线+分钟级）
  - 修正: l_1d 缺少 quantity_calculator.rs
  - 修正: check_chain 返回 TriggerEvent 而非 StrategySignal
  - 完善: 双周期统一命名规范

================================================================================
一、现有代码问题检查报告（V1.0问题状态）
================================================================================

1.1 核心问题汇总

  层级          问题                                      严重度    状态
  ─────────────────────────────────────────────────────────────────────────
  架构断层      TriggerEvent在d_checktable内部生成，      致命      待修复
               完全未传递给f_engine
  策略层空转    execute_strategy()是空实现，返回no_action  致命      待修复
  数量计算缺失  策略层DaySignalOutput只有bool，            致命      待修复
               引擎被迫自己算
  仓位ID缺失    LocalPosition无position_id，               致命      待修复
               无法指定平仓
  b_close占位符 始终返回false，平仓逻辑未实现              中等      待修复
  c_hedge占位符 始终返回false，对冲逻辑未实现              中等      待修复
  CheckTable未调用 接口定义了check()但从未被调用           中等      待修复

1.2 当前数据流（断裂）

  d_checktable侧:
    run_check_chain() → TriggerEvent { symbol, CheckSignal }  ← 引擎拿不到

  f_engine侧:
    process_tick()
      → MinuteTrigger.check()      ← 只做波动率检查
      → execute_strategy()         ← 空实现，返回no_action
      → pre_check() / lock_check()
      → 创建订单

================================================================================
二、策略层→引擎层 统一信号数据结构
================================================================================

2.1 信号文件位置

  x_data/src/trading/signal.rs

2.2 策略标识

  StrategyType:
    - Trend   (趋势策略)
    - Pin     (Pin因子策略)
    - Grid    (网格策略)

  StrategyLevel:
    - Minute  (15分钟级)
    - Day     (日线级)

  StrategyId:
    - strategy_type: StrategyType
    - instance_id: String
    - level: StrategyLevel

  工厂方法:
    - StrategyId::new_trend_minute(instance_id)
    - StrategyId::new_trend_day(instance_id)
    - StrategyId::new_pin_minute(instance_id)

2.3 交易指令

  TradeCommand:
    - Open           开仓
    - Add            加仓
    - Reduce         减仓（部分平仓）
    - FlatAll        全平
    - FlatPosition   指定仓位平仓
    - HedgeOpen      对冲开仓
    - HedgeClose     对冲平仓

2.4 仓位引用

  PositionRef:
    - position_id: String           仓位唯一ID
    - strategy_instance_id: String  关联的策略实例ID
    - side: PositionSide            持仓方向

2.5 策略信号（核心输出结构）

  StrategySignal:
    - command: TradeCommand          交易指令
    - direction: PositionSide       交易方向
    - quantity: Decimal             交易数量（策略层计算）
    - target_price: Decimal         目标价格
    - strategy_id: StrategyId        策略标识
    - position_ref: Option<PositionRef>  仓位引用
    - full_close: bool               是否全平
    - stop_loss_price: Option<Decimal>   止损价格
    - take_profit_price: Option<Decimal>  止盈价格
    - reason: String                执行原因
    - confidence: u8                置信度0-100
    - timestamp: i64                触发时间戳

  工厂方法:
    - StrategySignal::open(direction, quantity, target_price, strategy_id, reason)
    - StrategySignal::add(direction, quantity, target_price, strategy_id, position_ref, reason)
    - StrategySignal::flat_all(strategy_id, position_ref, reason)
    - StrategySignal::flat_position(quantity, strategy_id, position_ref, reason)
    - StrategySignal::reduce(quantity, strategy_id, position_ref, reason)

================================================================================
三、平仓功能（全局/指定仓位）传递方案
================================================================================

3.1 平仓命令类型

  命令           说明                   full_close  position_ref
  ─────────────────────────────────────────────────────────────────
  FlatAll        全平所有持仓           true        必须
  FlatPosition   指定仓位平仓           false       必须含position_id
  Reduce         部分减仓               false       必须

3.2 LocalPosition新增position_id

  字段:
    - position_id: String          唯一标识
    - strategy_instance_id: String 关联策略

  生成规则:
    position_id = "{symbol}_{direction}_{strategy_instance_id}_{timestamp}"

================================================================================
四、全局规范（铁律）
================================================================================

  规范  说明                                              级别
  ─────────────────────────────────────────────────────────────────
  1     策略层(d_checktable)必须生成完整StrategySignal    必须
  2     StrategySignal在x_data/trading/signal.rs统一维护  必须
  3     引擎层不计算任何策略数量，只执行                   必须
  4     平仓必须支持全平/指定仓位两种模式                 必须
  5     每个仓位必须有唯一position_id                     必须
  6     检查链run_check_chain返回Option<StrategySignal>  必须
  7     日线/分钟策略数量逻辑完全独立                     必须
  8     双周期信号结构命名完全统一                       必须

================================================================================
五、日线策略（l_1d）数量计算实现方案
================================================================================

5.1 现状分析

  问题: V1.0规范要求新增 l_1d/quantity_calculator.rs，但实际代码中不存在

  现有文件:
    l_1d/
      ├── signal_generator.rs     ✓ 已有
      ├── market_status_generator.rs  ✓ 已有
      ├── price_control_generator.rs  ✓ 已有
      ├── check/                  ✓ 已有
      │   ├── mod.rs
      │   ├── a_exit.rs
      │   ├── b_close.rs
      │   ├── c_hedge.rs
      │   ├── d_add.rs
      │   └── e_open.rs
      └── quantity_calculator.rs  ✗ 缺失（需新建）

5.2 日线数量配置

  DayQuantityConfig:
    - base_open_qty: Decimal = 0.1      基础开仓数量
    - max_position_qty: Decimal = 0.3  最大持仓数量
    - add_multiplier: Decimal = 1.5     加仓倍数
    - vol_adjustment: bool = true       波动率调整启用

5.3 日线数量计算方法

  calc_open_quantity(vol_tier):
    - Low:      base * 1.2
    - Medium:   base * 1.0
    - High:     base * 0.8
    - Extreme:  base * 0.5

  calc_add_quantity(current_position_qty, vol_tier):
    - 先计算: base_open_qty * add_multiplier
    - 再限制: 不能超过 max_position_qty - current_position_qty
    - 最后波动率调整

5.4 日线信号生成优先级

  优先级从高到低:
    1. Exit信号（long_exit/short_exit）→ FlatAll
    2. Hedge信号（long_hedge/short_hedge）→ Add
    3. Open信号（long_entry/short_entry）→ Open

================================================================================
六、15分钟级策略（h_15m）完整实现方案
================================================================================

6.1 现状分析

  已有文件:
    h_15m/
      ├── mod.rs
      ├── signal_generator.rs         ✓ 已有（7条件Pin模式）
      ├── market_status_generator.rs  ✓ 已有
      ├── price_control_generator.rs  ✓ 已有
      ├── pipeline_form.rs            ✓ 已有
      ├── quantity_calculator.rs      ✓ 已有（已实现）
      └── check/                      ✓ 已有
          ├── mod.rs
          ├── a_exit.rs
          ├── b_close.rs
          ├── c_hedge.rs
          ├── d_add.rs
          ├── e_open.rs
          └── check_chain.rs          ⚠ 返回TriggerEvent而非StrategySignal

6.2 15分钟级信号逻辑（7条件Pin模式）

  7个极端条件:
    #1  extreme_zscore:       |zscore_14_1m| > 2 OR |zscore_1h_1m| > 2
    #2  extreme_vol:          tr_ratio_60min_5h > 1 OR tr_ratio_10min_1h > 1
    #3  extreme_pos:          pos_norm_60 > 80 OR < 20
    #4  extreme_speed:        acc_percentile_1h > 90
    #5  extreme_bg_color:     pine_bg_color == "纯绿" OR "纯红"
    #6  extreme_bar_color:    pine_bar_color == "纯绿" OR "纯红"
    #7  extreme_price_dev:   |price_deviation_horizontal_position| == 100

  信号判定:
    long_entry:     tr_base_60min > 15% AND price_deviation < 0 AND pin >= 4
    short_entry:    tr_base_60min > 15% AND price_deviation > 0 AND pin >= 4
    long_exit:      pin >= 4 AND pos_norm_60 > 80
    short_exit:     pin >= 4 AND pos_norm_60 < 20
    long_hedge:     tr_base_60min < 15% AND price_deviation < 0 AND 6cond >= 4
    short_hedge:    tr_base_60min < 15% AND price_deviation > 0 AND 6cond >= 4
    exit_high_vol:  tr_base_60min < 15% AND 3cond >= 2

6.3 15分钟级数量配置

  MinQuantityConfig:
    - base_open_qty: Decimal = 0.05     基础开仓（比日线小）
    - max_position_qty: Decimal = 0.15   最大持仓（比日线小）
    - add_multiplier: Decimal = 1.5     加仓倍数
    - vol_adjustment: bool = true        波动率调整启用

6.4 15分钟级信号生成优先级

  优先级从高到低:
    1. Exit信号（long_exit/short_exit）→ FlatAll
    2. exit_high_volatility → FlatAll
    3. Hedge信号（long_hedge/short_hedge）→ Add
    4. Open信号（long_entry/short_entry）→ Open

6.5 h_15m与l_1d核心差异

  维度           h_15m（分钟级）              l_1d（日线级）
  ─────────────────────────────────────────────────────────────────
  信号模式       7条件Pin模式                 3组Pine颜色模式
  基础开仓量     0.05 BTC                     0.1 BTC
  最大持仓量     0.15 BTC                     0.3 BTC
  TR前置条件     tr_base_60min > 15%          tr_ratio > 1
  颜色判断       纯绿/纯红/紫色               全绿/全红/紫色
  位置判断       pos_norm_60 (20-80区间)      ma5_in_20d_ma5_pos (30-70区间)

================================================================================
七、Python旧策略 → Rust双周期适配映射表
================================================================================

7.1 Pin策略（pin_main.py）适配

  Python组件: PinStatusDetector (pin_status_detector.py)

  功能               Python方法                        Rust实现
  ─────────────────────────────────────────────────────────────────
  做多入场检测       check_long_entry()               MinSignalGenerator::check_long_entry()
  做空入场检测       check_short_entry()              MinSignalGenerator::check_short_entry()
  做多退出检测       check_long_exit()                MinSignalGenerator::check_long_exit()
  做空退出检测       check_short_exit()               MinSignalGenerator::check_short_exit()
  多头对冲条件       check_long_hedge_condition()     MinSignalGenerator::check_long_hedge()
  空头对冲条件       check_short_hedge_condition()    MinSignalGenerator::check_short_hedge()
  高波动退出         check_exit_high_volatility()     MinSignalGenerator::check_exit_high_volatility()

  7条件判定:
    Python条件计数                              Rust实现
    ──────────────────────────────────────────────────────────────
    |zscore_14_1m| > 2 OR |zscore_1h_1m| > 2    count_pin_conditions #1
    tr_ratio_60min_5h > 1 OR > 1               count_pin_conditions #2
    pos_norm_60 > 80 OR < 20                   count_pin_conditions #3
    acc_percentile_1h > 90                     count_pin_conditions #4
    pine_bg_color == "纯绿"/"纯红"              count_pin_conditions #5
    pine_bar_color == "纯绿"/"纯红"             count_pin_conditions #6
    |price_deviation_horizontal_position|=100  count_pin_conditions #7

  仓位状态机映射:
    Python状态                      Rust仓位状态
    ─────────────────────────────────────────────────────────────────
    INITIAL                         初始无持仓
    HEDGE_ENTER                     对冲入场（Add）
    POS_LOCKED                      仓位锁定
    Long_INITIAL                    多头初始
    Long_FIRST_OPEN                 多头首次开仓（Open）
    Long_DOUBLE_ADD                 多头加仓（Add）
    Long_DAY_ALLOW                  多头允许日线对冲
    Short_*                         空头对应状态（同理）

  平仓规则映射:
    Python平仓                Rust平仓命令
    ─────────────────────────────────────────────────────────────────
    盈利1%平仓               FlatAll (full_close=true)
    最低平仓线平仓           FlatAll (full_close=true)
    插针行情退出             FlatAll (exit信号)
    趋势行情处理             FlatAll (日线趋势反转)

7.2 Trend策略（trend_main.py）适配

  Python组件: TrendStatusDetector (trend_status_detector.py)

  功能               Python方法                              Rust实现
  ─────────────────────────────────────────────────────────────────
  做多入场检测       check_long_entry()                      DaySignalGenerator::check_long_entry()
  做空入场检测       check_short_entry()                    DaySignalGenerator::check_short_entry()
  做多退出检测       check_long_exit()                      DaySignalGenerator::check_long_exit()
  做空退出检测       check_short_exit()                     DaySignalGenerator::check_short_exit()
  多头对冲条件       check_long_hedge_condition()           DaySignalGenerator::check_long_hedge()
  空头对冲条件       check_short_hedge_condition()          DaySignalGenerator::check_short_hedge()

  Pine颜色分组校验:
    Python方法                              Rust实现
    ─────────────────────────────────────────────────────────────────
    _validate_pine_color_groups()          内置于check_all_pine_green/red_purple
    _check_all_pine_green_for_long()        DaySignalGenerator::check_all_pine_green()
    _check_all_pine_red_purple_for_short()  DaySignalGenerator::check_all_pine_red_purple()

  颜色组定义（3组，需全部满足）:
    周期组         bar_color字段                 bg_color字段
    ─────────────────────────────────────────────────────────────────
    12_26组        pine_bar_color_12_26          pine_bg_color_12_26
    20_50组        pine_bar_color_20_50          pine_bg_color_20_50
    100_200组      pine_bar_color_100_200        pine_bg_color_100_200

  日线入场判定:
    Python条件                                Rust条件
    ─────────────────────────────────────────────────────────────────
    全组纯绿 AND (波动率条件) AND ma5_pos>70  all_green AND vol AND pos>70
    全组紫色/纯红 AND (波动率条件) AND ma5_pos<30  all_red_purple AND vol AND pos<30

  平仓规则映射:
    Python平仓                Rust平仓命令
    ─────────────────────────────────────────────────────────────────
    日线指标平仓              FlatAll (long_exit/short_exit)
    保本平仓                  FlatAll (stop_loss触发)
    趋势反转                  FlatAll (对冲后反转)

7.3 双策略对比表

  维度         Pin策略                    Trend策略
  ─────────────────────────────────────────────────────────────────
  适用周期     15分钟级(h_15m)            日线级(l_1d)
  核心指标     7条件Pin模式               3组Pine颜色模式
  触发条件     TR>15% + 极端位置          Pine全绿/全红 + 波动率
  开仓数量     0.05 BTC（基础）           0.1 BTC（基础）
  最大持仓     0.15 BTC                   0.3 BTC
  对冲条件     TR<15% + 6条件>=4         背景色=淡绿/淡红
  退出条件     pin>=4 + 位置极端          max_valid_bg != 纯色

================================================================================
八、双周期统一整改步骤
================================================================================

8.1 整改总览

  Phase  任务                          涉及文件
  ─────────────────────────────────────────────────────────────────
  P1     修复l_1d缺失quantity_calculator  d_checktable/src/l_1d/
  P2     统一check_chain返回StrategySignal  h_15m/check/check_chain.rs
                                             l_1d/check/check_chain.rs
  P3     引擎层接收StrategySignal          f_engine/src/core/engine_v2.rs
  P4     实现execute_strategy调用检查链     f_engine/src/core/execution.rs

8.2 Phase P1: 修复l_1d缺失文件

  步骤1: 新建 d_checktable/src/l_1d/quantity_calculator.rs

    内容:
      - DayQuantityConfig 结构体（默认base=0.1, max=0.3）
      - DayQuantityCalculator 结构体
      - calc_open_quantity() 方法
      - calc_add_quantity() 方法
      - generate_signal() 方法返回Option<StrategySignal>

  步骤2: 更新 d_checktable/src/l_1d/mod.rs

    添加:
      pub mod quantity_calculator;
      pub use quantity_calculator::{DayQuantityCalculator, DayQuantityConfig};

8.3 Phase P2: 统一check_chain返回StrategySignal

  步骤1: 修改 h_15m/check/check_chain.rs

    变更:
      - run_check_chain() 返回 Option<StrategySignal>
      - 调用 MinQuantityCalculator::generate_signal()
      - 移除 TriggerEvent 转换，直接返回 StrategySignal

  步骤2: 修改 l_1d/check/check_chain.rs

    变更:
      - run_check_chain() 返回 Option<StrategySignal>
      - 调用 DayQuantityCalculator::generate_signal()
      - 移除 TriggerEvent 转换，直接返回 StrategySignal

  步骤3: 统一接口签名

    h_15m和l_1d的check_chain必须保持完全一致的函数签名:
      pub fn run_check_chain(
          symbol: &str,
          input: &MinSignalInput,  // 或 DaySignalInput
          ctx: &CheckChainContext,
      ) -> Option<StrategySignal>

  CheckChainContext结构（双周期通用）:
      pub struct CheckChainContext {
          pub current_position_qty: Decimal,
          pub strategy_id: StrategyId,
          pub position_ref: Option<PositionRef>,
      }

8.4 Phase P3: 引擎层接收StrategySignal

  步骤1: 修改 f_engine/src/core/engine_v2.rs

    添加:
      - use x_data::trading::signal::StrategySignal;
      - execute_strategy() 接收 Option<StrategySignal>
      - 匹配 StrategySignal.command 执行对应操作

  步骤2: 修改 execute_strategy() 实现

    伪代码:
      fn execute_strategy(&mut self, signal: Option<StrategySignal>) -> OrderDecision {
          match signal {
              Some(StrategySignal { command: Open, ... }) => self.handle_open(signal),
              Some(StrategySignal { command: Add, ... }) => self.handle_add(signal),
              Some(StrategySignal { command: FlatAll, ... }) => self.handle_flat_all(signal),
              Some(StrategySignal { command: FlatPosition, ... }) => self.handle_flat_position(signal),
              None => OrderDecision::NoAction,
          }
      }

8.5 Phase P4: 实现完整调用链

  步骤1: 引擎主循环集成

    伪代码:
      loop {
          // 1. 接收市场数据
          let tick = receiver.recv();

          // 2. 构建CheckChainContext
          let ctx = CheckChainContext {
              current_position_qty: self.position_manager.get_qty(&symbol),
              strategy_id: StrategyId::new_trend_minute("instance_1"),
              position_ref: self.position_manager.get_position_ref(&symbol),
          };

          // 3. 调用分钟级检查链
          let min_signal = h_15m::check::run_check_chain(&symbol, &min_input, &ctx);

          // 4. 调用日线级检查链
          let day_signal = l_1d::check::run_check_chain(&symbol, &day_input, &ctx);

          // 5. 优先级合并（分钟级优先）
          let signal = min_signal.or(day_signal);

          // 6. 执行信号
          let decision = self.execute_strategy(signal);
      }

  步骤2: 双周期信号优先级规则

    规则: 分钟级信号优先于日线级信号
    原因: 分钟级捕捉短期机会，日线级决定方向

    优先级:
      1. 分钟级Exit/FlatAll  ← 最高（立即止损）
      2. 分钟级Hedge/Add
      3. 分钟级Open
      4. 日线级Exit/FlatAll
      5. 日线级Hedge/Add
      6. 日线级Open           ← 最低

================================================================================
九、新旧结构对比
================================================================================

  旧结构                              新结构                              变化
  ─────────────────────────────────────────────────────────────────────────
  TriggerEvent { symbol, signal }    StrategySignal { command, ... }     全面扩展
  CheckSignal { Exit, Close, ... }  TradeCommand { Open, Add, ... }     语义更清晰
  无                                   StrategyId { type, instance, level }  新增
  无                                   PositionRef { position_id, ... }       新增
  LocalPosition无ID                   LocalPosition { position_id, ... }    新增
  run_check_chain → TriggerEvent      run_check_chain → StrategySignal       升级

================================================================================
十、信号传递流程（V1.1完整版）
================================================================================

  策略层(d_checktable)侧:
  ─────────────────────
    指标数据
       │
       ▼
    SignalGenerator.generate()
       │                   ┌─────────────────────────────┐
       │                   │ MinSignalOutput / DaySignalOutput │
       │                   │ (纯bool信号)                 │
       │                   └─────────────────────────────┘
       ▼                              │
    QuantityCalculator.generate_signal()│
       │                              │
       │                              ▼
       │                   ┌─────────────────────────────┐
       │                   │ StrategySignal               │
       │                   │ (含数量、平仓信息)            │
       └──────────────────→│ (核心输出结构)               │
                           └─────────────────────────────┘
                                    │
                                    ▼
                           CheckChainContext
                           (持仓信息、策略ID)
                                    │
                                    ▼
                           run_check_chain()
                                    │
                                    ▼
                           Option<StrategySignal>
                                    │
                                    ▼
  引擎层(f_engine)侧:
  ─────────────────────
                           Option<StrategySignal>
                                    │
                                    ▼
                           execute_strategy()
                                    │
                                    ▼
                           ┌─────────────────────────────────────┐
                           │ match command {                      │
                           │   Open        → handle_open()         │
                           │   Add         → handle_add()         │
                           │   FlatAll     → handle_flat_all()    │
                           │   FlatPosition → handle_flat_pos()   │
                           │   Reduce      → handle_reduce()      │
                           │   _           → no_action             │
                           │ }                                    │
                           └─────────────────────────────────────┘
                                    │
                                    ▼
                           OrderExecutor.execute()
                                    │
                                    ▼
                           订单发送至交易所

================================================================================
十一、双周期统一命名规范
================================================================================

11.1 目录结构命名

  d_checktable/src/
    ├── h_15m/                    15分钟级策略
    │   ├── mod.rs
    │   ├── signal_generator.rs
    │   ├── market_status_generator.rs
    │   ├── price_control_generator.rs
    │   ├── pipeline_form.rs
    │   ├── quantity_calculator.rs
    │   └── check/
    │       ├── mod.rs
    │       ├── a_exit.rs
    │       ├── b_close.rs
    │       ├── c_hedge.rs
    │       ├── d_add.rs
    │       ├── e_open.rs
    │       └── check_chain.rs
    │
    └── l_1d/                     日线级策略
        ├── mod.rs
        ├── signal_generator.rs
        ├── market_status_generator.rs
        ├── price_control_generator.rs
        ├── quantity_calculator.rs     ← 新增
        └── check/
            ├── mod.rs
            ├── a_exit.rs
            ├── b_close.rs
            ├── c_hedge.rs
            ├── d_add.rs
            ├── e_open.rs
            └── check_chain.rs

11.2 类型命名

  前缀规则:
    Min  → 分钟级专用类型
    Day  → 日线级专用类型
    无前缀 → 双周期通用类型

  分钟级类型:
    MinSignalInput
    MinSignalOutput
    MinMarketStatusInput
    MinMarketStatusOutput
    MinQuantityConfig
    MinQuantityCalculator
    MinSignalGenerator

  日线级类型:
    DaySignalInput
    DaySignalOutput
    DayMarketStatusInput
    DayMarketStatusOutput
    DayQuantityConfig
    DayQuantityCalculator
    DaySignalGenerator

  通用类型:
    StrategySignal
    StrategyId
    StrategyType
    StrategyLevel
    TradeCommand
    PositionRef
    CheckChainContext
    VolatilityTier

11.3 函数命名

  模块内函数:
    h_15m::check::run_check_chain()
    l_1d::check::run_check_chain()

  生成器方法:
    MinSignalGenerator::generate()
    DaySignalGenerator::generate()
    MinQuantityCalculator::generate_signal()
    DayQuantityCalculator::generate_signal()

  条件检查方法（双周期各自实现）:
    check_long_entry()
    check_short_entry()
    check_long_exit()
    check_short_exit()
    check_long_hedge()
    check_short_hedge()

11.4 注释规范

  每个文件头部注释:
    //! {模块名}
    //!
    //! {功能描述}

  每个结构体注释:
    /// {结构体描述}
    #[derive(...)]
    pub struct Xxx { ... }

================================================================================
十二、引擎层接入最小修改代码
================================================================================

12.1 新增StrategySignal引用

  在 f_engine/src/lib.rs 或相关文件添加:
    use x_data::trading::signal::{StrategySignal, StrategyId, TradeCommand, PositionRef};

12.2 修改execute_strategy签名

  旧:
    fn execute_strategy(&mut self, symbol: &str) -> OrderDecision

  新:
    fn execute_strategy(&mut self, signal: Option<StrategySignal>) -> OrderDecision

12.3 添加策略信号处理

  fn handle_strategy_signal(&mut self, signal: StrategySignal) -> OrderDecision {
      match signal.command {
          TradeCommand::Open => {
              self.handle_open(signal.direction, signal.quantity)
          }
          TradeCommand::Add => {
              self.handle_add(signal.position_ref.unwrap(), signal.quantity)
          }
          TradeCommand::FlatAll => {
              self.handle_flat_all(signal.position_ref.unwrap())
          }
          TradeCommand::FlatPosition => {
              self.handle_flat_position(signal.position_ref.unwrap(), signal.quantity)
          }
          TradeCommand::Reduce => {
              self.handle_reduce(signal.position_ref.unwrap(), signal.quantity)
          }
          _ => OrderDecision::NoAction,
      }
  }

12.4 主循环集成

  伪代码:
    // 引擎主循环
    loop {
        let tick = self.market_data_receiver.recv().await;

        // 构建检查上下文
        let ctx = CheckChainContext {
            current_position_qty: self.position_manager.get_qty(&tick.symbol),
            strategy_id: StrategyId::new_trend_minute("default"),
            position_ref: self.position_manager.get_ref(&tick.symbol),
        };

        // 调用检查链（双周期）
        let min_signal = h_15m::check::run_check_chain(
            &tick.symbol,
            &tick.min_indicators,
            &ctx,
        );
        let day_signal = l_1d::check::run_check_chain(
            &tick.symbol,
            &tick.day_indicators,
            &ctx,
        );

        // 优先级合并
        let signal = min_signal.or(day_signal);

        // 执行
        let decision = self.execute_strategy(signal);

        // 下单
        if let OrderDecision::Execute(order) = decision {
            self.order_executor.execute(order).await?;
        }
    }

================================================================================
文档结束
================================================================================
