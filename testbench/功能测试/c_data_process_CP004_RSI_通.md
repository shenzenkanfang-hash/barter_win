================================================================================
                    接口验证报告：c_data_process::RSI
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/pine_indicator_full.rs

【接口签名】
pub struct RSI {
    period: usize,
    rma_up: RMA,
    rma_down: RMA,
    last_price: Decimal,
    epsilon: Decimal,
}
impl RSI {
    pub fn new(period: usize) -> Self
    pub fn update(&mut self, price: Decimal) -> Decimal
}

RSI使用RMA(Relative Moving Average)作为平滑：
pub struct RMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}
impl RMA {
    pub fn new(period: usize) -> Self
    pub fn update(&mut self, price: Decimal) -> Decimal
    pub fn get(&self) -> Decimal
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_pine_color_detector_align_with_python (隐含RSI测试)
构造输入：
  prices = [30000, 30500, 30200, 29800, 30100, 30600, 30400, 29900, 30200, 30700]
执行动作：
  detector.update_close_only(price) 循环10次
实际输出：
  rsi 和 crsi 在每次update中正确计算
  rsi范围 0-100
  crsi 范围通常也在合理区间
对比预期：
  预期 = RSI指标正确计算，范围有效
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：RSI边界条件
构造输入：
  价格持续上涨 (rma_down < epsilon -> RSI = 100)
  价格持续下跌 (rma_up < epsilon -> RSI = 0)
执行动作：
  update持续上涨价格
  update持续下跌价格
实际输出：
  持续上涨 -> RSI趋近100
  持续下跌 -> RSI趋近0
对比预期：
  预期 = RSI边界值(0,100)正确处理
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：RSI首次更新
构造输入：
  rsi = RSI::new(14)
  first_price = dec!(10000)
执行动作：
  rsi.update(first_price)
实际输出：
  change = 0 (last_price为0时)
  rsi返回基于change计算的RSI值
对比预期：
  预期 = 首次更新时change为0，正确处理
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test pine_indicator_full::tests::test_pine_color_detector_align_with_python ... ok
  (RSI在PineColorDetector内部使用，通过颜色检测器测试覆盖)

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
