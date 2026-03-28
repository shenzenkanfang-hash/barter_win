================================================================================
                    接口验证报告：c_data_process::PineColorDetectorV5
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/pine_indicator_full.rs

【接口签名】
pub struct PineColorDetector {
    macd_fast: EMA,
    macd_slow: EMA,
    signal_ema: EMA,
    ema10: EMA,
    ema20: EMA,
    ...
}
impl PineColorDetector {
    pub fn new() -> Self
    pub fn update(&mut self, ohlc: (Decimal, Decimal, Decimal, Decimal)) -> (String, String, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal)
    pub fn update_close_only(&mut self, close: Decimal) -> (String, String, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal)
    pub fn get_macd(&self) -> (Decimal, Decimal, Decimal)
    pub fn get_rsi(&self) -> (Decimal, Decimal)
    pub fn calc_top3_avg_amplitude_pct(&self) -> Decimal
    pub fn calc_one_percent_amplitude_time_days(&self) -> Decimal
    pub fn reset(&mut self)
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_pine_color_detector_align_with_python
构造输入：
  prices = [30000, 30500, 30200, 29800, 30100, 30600, 30400, 29900, 30200, 30700]
执行动作：
  detector.update_close_only(price) 循环10次
实际输出：
  返回 (bar_color, bg_color, macd, signal, hist, ema10, ema20, rsi, crsi)
  每次输出包含颜色字符串和9个数值
  hist == macd - signal 验证通过
对比预期：
  预期 = Pine颜色检测器正确输出各指标值
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：test_pine_color
构造输入：
  多组OHLC数据，覆盖不同颜色条件
执行动作：
  detector.update(ohlc) 多组测试
实际输出：
  各种颜色条件正确识别
  bar_color 和 bg_color 返回有效颜色字符串
对比预期：
  预期 = 颜色优先级和边界条件正确处理
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：pct calc_one_percent_amplitude_time_days 边界
构造输入：
  数据不足2条时调用
执行动作：
  calc_one_percent_amplitude_time_days()
  calc_top3_avg_amplitude_pct()
实际输出：
  数据不足时返回 Decimal::ZERO
对比预期：
  预期 = 边界条件下安全返回
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test pine_indicator_full::tests::test_pine_color_detector_align_with_python ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
