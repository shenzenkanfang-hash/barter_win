================================================================================
                    接口验证报告：c_data_process::SignalProcessor
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/processor.rs

【接口签名】
pub struct SignalProcessor { ... }
impl SignalProcessor {
    pub fn new() -> Self
    pub fn with_ttl(ttl_secs: u64) -> Self
    pub fn register_symbol(&self, symbol: &str)
    pub fn unregister_symbol(&self, symbol: &str)
    pub fn registered_symbols(&self) -> Vec<String>
    pub fn is_registered(&self, symbol: &str) -> bool
    pub fn min_update(&self, symbol: &str, high: Decimal, low: Decimal, close: Decimal, volume: Decimal) -> Result<(), String>
    pub fn day_update(&self, symbol: &str, high: Decimal, low: Decimal, close: Decimal) -> Result<(), String>
    pub fn day_cleanup_if_needed(&self)
    pub fn min_get_tr_ratio(&self, symbol: &str) -> Option<Decimal>
    pub fn day_get_tr_ratio(&self, symbol: &str) -> Option<(Decimal, Decimal)>
    pub fn day_get_pine(&self, symbol: &str) -> Option<BigCycleIndicators>
    pub fn cleanup_expired(&self) -> usize
    pub fn active_count(&self) -> usize
    pub fn set_min_signal(&self, symbol: &str, decision: TradingDecision)
    pub fn get_min_signal(&self, symbol: &str) -> Option<(TradingDecision, i64)>
    pub fn set_day_signal(&self, symbol: &str, decision: TradingDecision)
    pub fn get_day_signal(&self, symbol: &str) -> Option<(TradingDecision, i64)>
    pub fn day_is_ready(&self, symbol: &str) -> bool
    pub fn day_bar_count(&self, symbol: &str) -> usize
    pub fn min_is_ready(&self, symbol: &str) -> bool
    pub fn stop(&self)
    pub fn is_running(&self) -> bool
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_register_unregister
构造输入：
  symbol = "btcusdt"
执行动作：
  processor.register_symbol("btcusdt")
  processor.is_registered("btcusdt")
  processor.is_registered("BTCUSDT")
  processor.active_count()
  processor.unregister_symbol("btcusdt")
实际输出：
  is_registered("btcusdt") = true
  is_registered("BTCUSDT") = true
  active_count() = 1
  unregister后 is_registered = false
  active_count() = 0
对比预期：
  预期 = 注册后可查询，取消后可移除
  实际 = 一致
  差异 = 无
结果：通过

测试用例：test_day_indicators
构造输入：
  100根日线数据 (base=100+i, high=base+2, low=base-2, close=base)
执行动作：
  for i in 0..100 { day_update("BTCUSDT", high, low, close) }
  day_get_tr_ratio("BTCUSDT")
  day_get_pine_20_50("BTCUSDT")
  day_is_ready("BTCUSDT")
  day_bar_count("BTCUSDT")
实际输出：
  tr_ratio = Some((tr_5d_20d, tr_20d_60d))
  pine = Some(PineColorBig)
  day_is_ready = true
  bar_count = 100
对比预期：
  预期 = 日线指标正确计算，就绪状态正确
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：test_day_update_validation
构造输入：
  high = 100, low = 102 (high < low)
  high = 102, low = 100, close = 99 (close < low)
执行动作：
  day_update("BTCUSDT", dec!(100), dec!(102), dec!(101))
  day_update("BTCUSDT", dec!(102), dec!(100), dec!(99))
实际输出：
  第一调用返回 Err("Invalid data: high(100) < low(102)")
  第二调用返回 Err("Invalid data: close(99) not in [low(100), high(102)]")
对比预期：
  预期 = 数据验证失败时返回明确错误信息
  实际 = 一致
  差异 = 无
结果：通过

测试用例：test_min_update_validation
构造输入：
  high = 100, low = 102 (high < low)
  未注册的symbol
执行动作：
  min_update("btcusdt", dec!(100), dec!(102), dec!(101), dec!(1000))
  unregister后再调用min_update
实际输出：
  返回 Err("Invalid data: high(100) < low(102)")
  返回 Err("Symbol BTCUSDT not registered for 1m")
对比预期：
  预期 = 高<低验证，未注册验证均正确拦截
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：test_running_state
构造输入：
  processor = Arc::new(SignalProcessor::new())
执行动作：
  processor.start_loop()
  processor.is_running()
  processor.stop()
实际输出：
  is_running() = true (启动后)
  stop() 执行后 running标志清除
对比预期：
  预期 = 运行状态正确跟踪，stop可停止
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test processor::tests::test_register_unregister ... ok
  test processor::tests::test_day_indicators ... ok
  test processor::tests::test_day_update_validation ... ok
  test processor::tests::test_min_update_validation ... ok
  test processor::tests::test_running_state ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
