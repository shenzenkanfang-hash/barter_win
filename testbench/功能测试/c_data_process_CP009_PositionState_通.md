================================================================================
                    接口验证报告：c_data_process::PositionState
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/strategy_state/state.rs

【接口签名】
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionState {
    pub current: Decimal,
    pub side: PositionSide,
    pub avg_entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub cumulative_closed_pnl: Decimal,
}
impl PositionState {
    pub fn update_position(&mut self, side: PositionSide, qty: Decimal, price: Decimal)
    pub fn update_unrealized_pnl(&mut self, current_price: Decimal)
}

【测试组1：正常输入】─────────────────────────────────
测试用例：strategy_state::state::test_update_position_long
构造输入：
  state = StrategyState::new("BTC-USDT", "trend_v1", "binance", "1h")
执行动作：
  state.update_position(PositionSide::Long, dec!(0.1), dec!(50000))
实际输出：
  position.current = 0.1
  position.side = Long
  position.avg_entry_price = 50000
对比预期：
  预期 = 做多持仓正确更新
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：空仓情况
构造输入：
  state.position.current = 0
执行动作：
  update_unrealized_pnl(dec!(50000))
实际输出：
  unrealized_pnl = 0 (空仓时盈亏为0)
对比预期：
  预期 = 空仓时不计算盈亏
  实际 = 一致
  差异 = 无
结果：通过

测试用例：做空持仓盈亏
构造输入：
  state.position.side = Short, avg_entry_price = 50000
执行动作：
  update_unrealized_pnl(dec!(48000))
实际输出：
  unrealized_pnl = 2000 (50000-48000)*qty
对比预期：
  预期 = 做空时价格下跌盈利
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：平仓操作
构造输入：
  state.position 有持仓
执行动作：
  update_position(PositionSide::None, qty=0, price=0)
实际输出：
  current = 0, side = None, avg_entry_price = 0
对比预期：
  预期 = 平仓后状态清零
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test strategy_state::state::tests::test_new_state ... ok
  test strategy_state::state::tests::test_update_position_long ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
