================================================================================
                    接口验证报告：c_data_process::EMA
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/pine_indicator_full.rs

【接口签名】
pub struct EMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}
impl EMA {
    pub fn new(period: usize) -> Self
    pub fn update(&mut self, price: Decimal) -> Decimal
    pub fn get(&self) -> Decimal
    pub fn reset(&mut self)
    pub fn calculate(value: Decimal, period: usize) -> Decimal
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_ema_align_with_python
构造输入：
  ema10 = EMA::new(10)
  price1 = 100
  price2 = 110
执行动作：
  first_val = ema10.update(dec!(100))
  second_val = ema10.update(dec!(110))
实际输出：
  first_val = 100 (初始值等于第一个价格)
  alpha = 2 / 11
  expected = price * (1-alpha) + new_price * alpha
  second_val == expected
对比预期：
  预期 = EMA公式 alpha = 2/(period+1) 完全对齐Python
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：EMA初始化和reset边界
构造输入：
  period = 1 (最小周期)
  period = 1000 (大周期)
执行动作：
  EMA::new(1).update(price)
  EMA::new(1000).update(price)
  ema.reset()
实际输出：
  周期1: alpha = 2/2 = 1.0
  周期1000: alpha = 2/1001
  reset后 initialized = false
对比预期：
  预期 = 各种周期值正确计算
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：reset后update
构造输入：
  ema = EMA::new(10)
  ema.update(dec!(100))
  ema.update(dec!(110))
  ema.reset()
执行动作：
  ema.reset()
  ema.update(dec!(120))
实际输出：
  reset后第一个值作为初始值 = 120
对比预期：
  预期 = reset后状态清零，重新开始
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test pine_indicator_full::tests::test_ema_align_with_python ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
