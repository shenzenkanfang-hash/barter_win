================================================================================
                    接口验证报告：c_data_process::min::trend
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/min/trend.rs

【接口签名】
pub struct Indicator1m { ... }
pub struct Indicator1mOutput {
    pub tr_ratio_10min_1h: Decimal,
    pub tr_ratio_zscore_10min_1h: Decimal,
    pub velocity: Decimal,
    pub acceleration: Decimal,
    pub power: Decimal,
    pub zscore_1h_1m: Decimal,
    pub zscore_14_1m: Decimal,
    pub pos_norm_60: Decimal,
    pub velocity_percentile: Decimal,
    pub power_percentile: Decimal,
}
impl Indicator1m {
    pub fn new() -> Self
    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, volume: Decimal) -> Indicator1mOutput
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_indicator_1m_basic
构造输入：
  60+根1分钟K线数据
执行动作：
  indicator.update(high, low, close, volume) 循环
实际输出：
  pos_norm_60, velocity, acceleration, power 等指标输出
  zscore_1h_1m, zscore_14_1m 正常计算
对比预期：
  预期 = 分钟级指标正确计算
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：test_zscore_calculation
构造输入：
  不同波动率的价格序列
执行动作：
  update多组数据
  检查zscore计算
实际输出：
  zscore值在合理范围内
  极端情况下zscore值正确
对比预期：
  预期 = ZScore计算正确，边界情况安全处理
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：test_percentile_alignment
构造输入：
  长时间序列数据
执行动作：
  update连续数据
  检查percentile计算
实际输出：
  velocity_percentile 和 power_percentile 在 0-100 范围内
对比预期：
  预期 = 百分位计算正确
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test min::trend::tests::test_indicator_1m_basic ... ok
  test min::trend::tests::test_zscore_calculation ... ok
  test min::trend::tests::test_percentile_alignment ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
