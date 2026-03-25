================================================================================
h_15m / l_1d 目录简化重构方案
================================================================================

Author: Droid
Created: 2026-03-25
Status: Draft
Version: 1.0

================================================================================
一、问题分析
================================================================================

1.1 当前混乱问题

h_15m/ 目录存在12个文件，结构混乱：
- 6个根文件 + check/目录6个文件
- check/ 目录的 a/b/c/d/e 命名不直观
- pipeline_form.rs、price_control_generator.rs 作用不明确
- market_status_generator.rs 可内联
- check/ 目录已被 signal_generator.rs 取代但未删除

l_1d/ 目录存在类似问题，check/ 目录冗余。

1.2 根因

历史演进过程中，检查链从 5个独立文件(a_exit/b_close/c_hedge/d_add/e_open)
逐步被 signal_generator.rs 统一，但旧文件未清理。

================================================================================
二、方案设计
================================================================================

2.1 设计原则

- 参考 Python pin_main.py 的清晰结构
- 职责单一：一个文件一个职责
- 流程清晰：信号生成 + 状态维护 + 入口编排
- 双通道支持：高速/低速通道自动切换

2.2 双通道策略架构

    分钟级策略 (h_15m)
    ─────────────────────────────────────────────────────────────────
       VolatilityTier::High  ──────────────→  高速通道 (Fast)
            │                                        - 高频交易
            │                                        - 0.05基础开仓
            │                                        - 0.15最大持仓
            │                                        - 7条件Pin模式
            ▼
       VolatilityTier::Low   ──────────────→  低速通道 (Slow)
       VolatilityTier::Medium                              - 低频交易
                                                 - 保守策略
                                                 - 参考日线方向

================================================================================
三、新目录结构
================================================================================

3.1 h_15m/ 简化后

    h_15m/
    ├── mod.rs              入口 + 双通道分发 + 数量计算
    ├── signal.rs           7条件Pin模式 + 双通道信号生成
    └── status.rs          PinStatus状态机维护（新增）

3.2 l_1d/ 简化后

    l_1d/
    ├── mod.rs              入口 + 流程
    ├── signal.rs           3组Pine颜色模式
    └── status.rs          TrendStatus状态机（新增）

================================================================================
四、各文件职责
================================================================================

4.1 h_15m/signal.rs

职责：7条件Pin模式 + 双通道信号生成

公开方法：
    pub struct MinSignalGenerator;

    impl MinSignalGenerator {
        /// 双通道主入口（自动选择高速/低速）
        pub fn generate(
            &self,
            input: &MinSignalInput,
            vol_tier: &VolatilityTier,
            day_direction: Option<PositionSide>,
        ) -> MinSignalOutput

        /// 高速通道信号生成（高频）
        pub fn generate_fast_signal(&self, input: &MinSignalInput) -> MinSignalOutput

        /// 低速通道信号生成（保守，参考日线方向）
        pub fn generate_slow_signal(
            &self,
            input: &MinSignalInput,
            day_direction: PositionSide,
        ) -> MinSignalOutput
    }

内部方法（私有）：
    /// 统计满足的插针条件数量（7个条件）
    fn count_pin_conditions(&self, input: &MinSignalInput) -> u8

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool

    /// 检查做空入场条件
    fn check_short_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool

    /// 检查做多退出条件
    fn check_long_exit(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool

    /// 检查做空退出条件
    fn check_short_exit(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &MinSignalInput) -> bool

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &MinSignalInput) -> bool

    /// 检查退出高波动条件
    fn check_exit_high_volatility(&self, input: &MinSignalInput) -> bool

4.2 h_15m/status.rs（新增）

职责：PinStatus状态机（从pin_main.py移植）

    /// Pin状态枚举（整合版）
    pub enum PinStatus {
        INITIAL,           // 初始状态
        HEDGE_ENTER,       // 进入对冲
        POS_LOCKED,        // 仓位锁定
        Long_INITIAL,      // 多头-初始
        Long_FIRST_OPEN,   // 多头-第一次开仓
        Long_DOUBLE_ADD,   // 多头-翻倍加仓
        Long_DAY_ALLOW,   // 多头-日线开放
        Short_INITIAL,    // 空头-初始
        Short_FIRST_OPEN,  // 空头-第一次开仓
        Short_DOUBLE_ADD,  // 空头-翻倍加仓
        Short_DAY_ALLOW,  // 空头-日线开放
    }

    /// Pin状态机
    pub struct PinStatusMachine {
        current_status: PinStatus,
    }

    impl PinStatusMachine {
        pub fn new() -> Self

        /// 获取当前状态
        pub fn current_status(&self) -> PinStatus

        /// 状态转换（根据信号输出）
        pub fn transition(&mut self, signal_output: &MinSignalOutput, market_status: MarketStatus)

        /// 判断是否可以开多
        pub fn can_long_open(&self) -> bool

        /// 判断是否可以开空
        pub fn can_short_open(&self) -> bool
    }

4.3 h_15m/mod.rs

职责：入口 + 双通道分发 + 数量计算

    pub fn run_check_chain(
        symbol: &str,
        input: &MinSignalInput,
        day_direction: Option<PositionSide>,  // 日线方向参考
        ctx: &CheckChainContext,
    ) -> Option<StrategySignal>

内部流程：
1. 判断波动率通道（High → 高速，Low/Medium → 低速）
2. 生成信号（调用 MinSignalGenerator::generate）
3. 状态更新（调用 PinStatusMachine::transition）
4. 数量计算（调用 MinQuantityCalculator::calc）
5. 生成 StrategySignal

4.4 l_1d/signal.rs

职责：3组Pine颜色模式（已完善，保持不变）

4.5 l_1d/status.rs（新增）

职责：TrendStatus状态机

    /// Trend状态枚举
    pub enum TrendStatus {
        INITIAL,
        Long_ENTER,
        Long_HOLD,
        Short_ENTER,
        Short_HOLD,
    }

    pub struct TrendStatusMachine {
        current_status: TrendStatus,
    }

================================================================================
五、需要删除的文件
================================================================================

5.1 删除 h_15m/

    h_15m/
    ├── market_status_generator.rs    ← 内联到 mod.rs
    ├── pipeline_form.rs            ← 冗余
    ├── price_control_generator.rs  ← 冗余
    └── check/                     ← 已被 signal.rs 取代
        ├── a_exit.rs
        ├── b_close.rs
        ├── c_hedge.rs
        ├── d_add.rs
        ├── e_open.rs
        ├── check_chain.rs
        └── mod.rs

5.2 删除 l_1d/

    l_1d/
    └── check/                     ← 已被 signal.rs 取代
        ├── a_exit.rs
        ├── b_close.rs
        ├── c_hedge.rs
        ├── d_add.rs
        ├── e_open.rs
        ├── check_chain.rs
        └── mod.rs

================================================================================
六、实施步骤
================================================================================

| 步骤 | 任务 | 状态 |
|------|------|------|
| 1 | 新建 h_15m/status.rs | 待执行 |
| 2 | 新建 l_1d/status.rs | 待执行 |
| 3 | 重写 h_15m/signal.rs（保留原逻辑 + 双通道） | 待执行 |
| 4 | 重写 h_15m/mod.rs（入口 + 数量计算） | 待执行 |
| 5 | 重写 l_1d/mod.rs（入口 + 流程） | 待执行 |
| 6 | 删除 h_15m 冗余文件 | 待执行 |
| 7 | 删除 l_1d 冗余文件 | 待执行 |
| 8 | cargo check --all 验证 | 待执行 |
| 9 | Git commit | 待执行 |

================================================================================
七、风险评估
================================================================================

风险等级：低

理由：
- signal_generator.rs 原逻辑保留
- 新增 status.rs 从 Python 移植，逻辑清晰
- 双通道模式是对现有逻辑的扩展，非破坏性变更

================================================================================
八、验收标准
================================================================================

1. h_15m/ 从 12个文件减少到 3个文件
2. l_1d/ 从 10个文件减少到 3个文件
3. cargo check --all 通过，0错误
4. 双通道逻辑正常工作（High → 高速，Low/Medium → 低速）
5. PinStatusMachine 状态转换正确
6. StrategySignal 输出格式不变
