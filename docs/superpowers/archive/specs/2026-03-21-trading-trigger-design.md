================================================================================
TradingTrigger 交易触发器设计文档
================================================================================
Author: 产品经理
Created: 2026-03-21
Status: approved
================================================================================

一、设计目标
================================================================================

在 indicator 模块中实现完整的交易触发逻辑：
1. 基于指标计算生成市场状态
2. 基于市场状态生成交易信号
3. 基于仓位和价格生成价格控制决策
4. 根据波动率自动切换 分钟级/日线级 策略

================================================================================
二、目录结构
================================================================================

crates/indicator/src/
├── lib.rs
├── pine_indicator_full.rs              # 公用 Pine 指标
├── trading_trigger.rs                 # 交易触发器
├── min/
│   ├── indicator_1m.rs              # 1分钟指标计算
│   ├── market_status_generator.rs     # 市场状态生成
│   ├── signal_generator.rs            # 信号生成
│   └── price_control_generator.rs     # 价格控制
└── day/
    ├── indicator_1d.rs               # 日线指标计算
    ├── market_status_generator.rs     # 市场状态生成
    ├── signal_generator.rs            # 信号生成
    └── price_control_generator.rs     # 价格控制

================================================================================
三、公共类型定义
================================================================================

// 市场状态枚举 (与老代码一致)
pub enum MarketStatus {
    TREND,   // 趋势状态
    RANGE,   // 震荡状态
    PIN,     // 插针状态
    INVALID, // 数据无效
}

// 波动率等级
pub enum VolatilityLevel {
    HIGH,    // 高波动 (15min TR > 13%)
    NORMAL,  // 正常波动
    LOW,     // 低波动
}

// 策略层级
pub enum StrategyLevel {
    MIN,  // 分钟级策略
    DAY,  // 日线级策略
}

// 交易动作
pub enum TradingAction {
    Long,      // 做多
    Short,     // 做空
    Flat,      // 平仓
    Hedge,     // 对冲
    Wait,      // 等待
}

================================================================================
四、MarketStatusGenerator 设计
================================================================================

职责：根据指标数据判断市场状态 (PIN/RANGE/TREND/INVALID + VolatilityLevel)

4.1 min/MarketStatusGenerator
--------------------------------------------------------------------------------

前置条件：无

VolatilityLevel 判定：
- HIGH: 15min TR > 13%
- NORMAL: 3% <= 15min TR <= 13%
- LOW: 15min TR < 3%

MarketStatus 判定 (优先级: INVALID > PIN > RANGE > TREND):
1. INVALID: 数据超时或无效
2. PIN: 满足插针条件 (见 SignalGenerator)
3. RANGE: 低波动 + Z-Score 接近 0 + TR < 1
4. TREND: 默认

输入结构：
pub struct MinMarketStatusInput {
    pub tr_ratio_10min: Decimal,
    pub tr_ratio_15min: Decimal,
    pub price_position: Decimal,  // 0-100
    pub zscore: Decimal,
    pub tr_base_60min: Decimal,
}

输出结构：
pub struct MinMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_level: VolatilityLevel,
    pub high_volatility_reason: Option<String>,
}

4.2 day/MarketStatusGenerator
--------------------------------------------------------------------------------

VolatilityLevel 判定：
- HIGH: 日线 TR 极端
- NORMAL/LOW: 正常

MarketStatus 判定：
1. INVALID: 数据超时或无效
2. PIN: 日线 PineColor + TR 极端
3. RANGE: 低 TR + 无强趋势颜色 + 动能适中
4. TREND: 默认

输入结构：
pub struct DayMarketStatusInput {
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub pine_color: PineColorBig,
    pub ma5_in_20d_ma5_pos: Decimal,
    pub power_percentile: Decimal,
}

输出结构：
pub struct DayMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_level: VolatilityLevel,
}

================================================================================
五、SignalGenerator 设计
================================================================================

职责：将指标转换为互斥的布尔交易信号

5.1 min/SignalGenerator
--------------------------------------------------------------------------------

前置条件：tr_base_60min > 15%

输入结构：
pub struct MinSignalInput {
    pub indicators: MinMarketStatusOutput,
    pub tr_base_60min: Decimal,
    pub zscore_14_1m: Decimal,
    pub zscore_1h_1m: Decimal,
    pub tr_ratio_60min_5h: Decimal,
    pub tr_ratio_10min_1h: Decimal,
    pub pos_norm_60: Decimal,
    pub acc_percentile_1h: Decimal,
    pub pine_bg_color: String,
    pub pine_bar_color: String,
    pub price_deviation: Decimal,
    pub price_deviation_horizontal_position: Decimal,
}

输出结构：
pub struct MinSignalOutput {
    pub long_entry: bool,     // 做多开仓
    pub short_entry: bool,    // 做空开仓
    pub long_exit: bool,      // 做多平仓/退出
    pub short_exit: bool,     // 做空平仓/退出
    pub long_hedge: bool,     // 多头对冲
    pub short_hedge: bool,    // 空头对冲
    pub exit_high_volatility: bool,  // 退出高波动
}

插针条件 (7个，满足 >= 4):
1. extreme_z: |zscore_14_1m| > 2 或 |zscore_1h_1m| > 2
2. extreme_vol: tr_ratio_60min_5h > 1 或 tr_ratio_10min_1h > 1
3. extreme_pos: pos_norm_60 > 90 或 < 10
4. extreme_speed: acc_percentile_1h > 90
5. extreme_bg_color: pine_bg_color == "纯绿" 或 "纯红"
6. extreme_bar_color: pine_bar_color == "纯绿" 或 "纯红"
7. price_deviation_extreme: |price_deviation_horizontal_position| == 100

对冲条件 (前置: tr_base_60min < 15%):
- 多头对冲: price_deviation < 0 + 多个条件满足
- 空头对冲: price_deviation > 0 + 多个条件满足

退出高波动条件 (前置: tr_base_60min < 15%):
- tr_ratio_60min_5h < 1 且 tr_ratio_10min_1h < 1
- 20 < pos_norm_60 < 80
- 10 < |price_deviation_horizontal_position| <= 90

5.2 day/SignalGenerator
--------------------------------------------------------------------------------

Pine颜色分组验证 (优先级: 100_200 > 20_50 > 12_26):
- 有效组: bar + bg 都有非空值
- 最小周期 12_26 优先判断

输入结构：
pub struct DaySignalInput {
    pub indicators: DayMarketStatusOutput,
    pub pine_color_100_200: PineColorBig,
    pub pine_color_20_50: PineColorBig,
    pub pine_color_12_26: PineColorBig,
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub ma5_in_20d_ma5_pos: Decimal,
}

输出结构：
pub struct DaySignalOutput {
    pub long_entry: bool,
    pub short_entry: bool,
    pub long_exit: bool,
    pub short_exit: bool,
    pub long_hedge: bool,
    pub short_hedge: bool,
}

做多信号条件:
- 所有有效 pine 颜色组必须为 "纯绿"
- tr_ratio_5d_20d > 1 或 tr_ratio_20d_60d > 1
- ma5_in_20d_ma5_pos > 70

做空信号条件:
- 所有有效 pine 颜色组必须为 "紫色" 或 "纯红"
- tr_ratio_5d_20d > 1 或 tr_ratio_20d_60d > 1
- ma5_in_20d_ma5_pos < 30

平仓条件 (使用最大有效周期):
- 多头平仓: ma5_in_20d_ma5_pos > 50 + 最大周期颜色 != "纯绿"
- 空头平仓: ma5_in_20d_ma5_pos < 50 + 最大周期颜色 != "纯红"

================================================================================
六、PriceControlGenerator 设计
================================================================================

职责：基于本地仓位和价格判断价格控制条件

6.1 输入结构
--------------------------------------------------------------------------------

pub struct PriceControlInput {
    // 仓位信息
    pub position_entry_price: Decimal,
    pub position_side: PositionSide,  // LONG / SHORT / NONE
    pub position_size: Decimal,

    // 当前价格
    pub current_price: Decimal,

    // 配置阈值
    pub profit_threshold: Decimal,     // 盈利平仓阈值 (如 1%)
    pub loss_threshold: Decimal,       // 止损阈值
    pub add_threshold: Decimal,        // 加仓阈值
    pub move_stop_threshold: Decimal,  // 移动止损阈值
}

6.2 输出结构
--------------------------------------------------------------------------------

pub struct PriceControlOutput {
    pub should_add: bool,      // 是否加仓
    pub should_stop: bool,     // 是否止损
    pub should_take_profit: bool,  // 是否止盈
    pub should_move_stop: bool,     // 是否移动止损

    // 价格条件
    pub profit_distance_pct: Decimal,  // 盈利距离百分比
    pub stop_distance_pct: Decimal,   // 止损距离百分比
}

6.3 判断逻辑
--------------------------------------------------------------------------------

盈利距离: (current_price - entry_price) / entry_price (多)
止损距离: (entry_price - current_price) / entry_price (多)

加仓条件:
- 有持仓 + 盈利 > add_threshold + 方向一致

止损条件:
- 亏损 > loss_threshold

止盈条件:
- 盈利 > profit_threshold + 满足条件数 >= 2

移动止损条件:
- 盈利 > move_stop_threshold + 更新追踪止损价

================================================================================
七、TradingTrigger 设计
================================================================================

职责：协调所有组件，根据波动率选择策略层级，输出最终交易决策

7.1 结构
--------------------------------------------------------------------------------

pub struct TradingTrigger {
    // min/ 组件
    min_status_gen: min::MarketStatusGenerator,
    min_signal_gen: min::SignalGenerator,
    min_price_ctrl: min::PriceControlGenerator,

    // day/ 组件
    day_status_gen: day::MarketStatusGenerator,
    day_signal_gen: day::SignalGenerator,
    day_price_ctrl: day::PriceControlGenerator,
}

7.2 输入
--------------------------------------------------------------------------------

pub struct TradingTriggerInput {
    pub symbol: String,

    // 价格数据
    pub current_price: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,

    // K线数据
    pub kline_1m: Option<KLine>,
    pub kline_1d: Option<KLine>,

    // 仓位状态
    pub check_list: CheckList,
}

7.3 输出
--------------------------------------------------------------------------------

pub struct TradingDecision {
    pub action: TradingAction,    // Long / Short / Flat / Hedge / Wait
    pub reason: String,            // 决策原因
    pub confidence: u8,           // 置信度 0-100
    pub level: StrategyLevel,     // MIN / DAY
}

7.4 核心逻辑
--------------------------------------------------------------------------------

impl TradingTrigger {
    pub fn run(&mut self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. 计算波动率等级
        let vol_level = self.calculate_volatility(input);

        // 2. 根据波动率选择策略
        match vol_level {
            VolatilityLevel::HIGH => self.run_min_strategy(input),
            _ => self.run_day_strategy(input),
        }
    }

    fn calculate_volatility(&self, input: &TradingTriggerInput) -> VolatilityLevel {
        // 从 kline_1m 或指标计算 15min TR
        let tr_15min = self.calculate_tr_15min(input);

        if tr_15min > dec!(0.13) {
            VolatilityLevel::HIGH
        } else if tr_15min < dec!(0.03) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    fn run_min_strategy(&mut self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. MarketStatusGenerator
        let status = self.min_status_gen.detect(&input.indicators_1m);

        // 2. SignalGenerator (前置检查)
        if input.indicators_1m.tr_base_60min <= dec!(0.15) {
            return TradingDecision {
                action: TradingAction::Wait,
                reason: "tr_base_60min <= 15%".to_string(),
                confidence: 0,
                level: StrategyLevel::MIN,
            };
        }
        let signal = self.min_signal_gen.generate(&input.indicators_1m, &status);

        // 3. PriceControlGenerator
        let price_ctrl = self.min_price_ctrl.check(&input.check_list, &input.current_price);

        // 4. 综合决策
        self.make_decision(signal, price_ctrl, StrategyLevel::MIN)
    }

    fn run_day_strategy(&mut self, input: &TradingTriggerInput) -> TradingDecision {
        // 类似 min 逻辑，但使用 day/ 组件
    }

    fn make_decision(
        &self,
        signal: SignalOutput,
        price_ctrl: PriceControlOutput,
        level: StrategyLevel,
    ) -> TradingDecision {
        // 优先级: 止损 > 止盈 > 对冲 > 开仓 > 等待

        if price_ctrl.should_stop {
            TradingDecision {
                action: TradingAction::Flat,
                reason: "stop_loss".to_string(),
                confidence: 100,
                level,
            }
        } else if price_ctrl.should_take_profit {
            TradingDecision {
                action: TradingAction::Flat,
                reason: "take_profit".to_string(),
                confidence: 95,
                level,
            }
        } else if signal.long_hedge || signal.short_hedge {
            TradingDecision {
                action: TradingAction::Hedge,
                reason: "hedge_signal".to_string(),
                confidence: 80,
                level,
            }
        } else if signal.long_entry {
            TradingDecision {
                action: TradingAction::Long,
                reason: "long_entry".to_string(),
                confidence: 75,
                level,
            }
        } else if signal.short_entry {
            TradingDecision {
                action: TradingAction::Short,
                reason: "short_entry".to_string(),
                confidence: 75,
                level,
            }
        } else {
            TradingDecision {
                action: TradingAction::Wait,
                reason: "no_signal".to_string(),
                confidence: 0,
                level,
            }
        }
    }
}

================================================================================
八、实现顺序
================================================================================

1. 定义公共类型 (lib.rs 或单独的 types.rs)
2. 实现 min/market_status_generator.rs
3. 实现 min/signal_generator.rs
4. 实现 min/price_control_generator.rs
5. 实现 day/market_status_generator.rs
6. 实现 day/signal_generator.rs
7. 实现 day/price_control_generator.rs
8. 实现 trading_trigger.rs
9. 更新 lib.rs 导出

================================================================================
九、参考实现
================================================================================

老代码 Python 实现位于:
- D:\量化策略开发\tradingW\backup_old_code\e_strategy\market_status.py
- D:\量化策略开发\tradingW\backup_old_code\e_strategy\pin_status_detector.py
- D:\量化策略开发\tradingW\backup_old_code\e_strategy\trend_status_detector.py
- D:\量化策略开发\tradingW\backup_old_code\e_strategy\pin_main.py

================================================================================
