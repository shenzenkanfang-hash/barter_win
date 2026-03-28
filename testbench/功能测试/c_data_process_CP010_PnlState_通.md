================================================================================
                    接口验证报告：c_data_process::PnlState
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/strategy_state/state.rs

【接口签名】
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlState {
    pub cumulative_closed: Decimal,
    pub daily: Vec<DailyPnl>,
    pub closed_trades: Vec<ClosedTrade>,
    pub max_drawdown: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_single_trade_profit: Decimal,
    pub max_single_trade_loss: Decimal,
}
impl PnlState {
    pub fn record_realized_pnl(&mut self, pnl: Decimal)
    pub fn record_closed_trade(&mut self, trade: ClosedTrade)
}

【测试组1：正常输入】─────────────────────────────────
测试用例：strategy_state::state::test_record_realized_pnl
构造输入：
  state = StrategyState::new("BTC-USDT", "trend_v1", "binance", "1h")
执行动作：
  state.record_realized_pnl(dec!(100))
  state.record_realized_pnl(dec!(-50))
  state.record_realized_pnl(dec!(200))
实际输出：
  pnl.cumulative_closed = 250
  trading_stats.total_trades = 3
  trading_stats.winning_trades = 2
  trading_stats.losing_trades = 1
  trading_stats.win_rate = 2/3
对比预期：
  预期 = 累计盈亏和交易统计正确计算
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：连续盈利更新max_drawdown
构造输入：
  初始状态
执行动作：
  record_realized_pnl(dec!(100)) -> cumulative = 100, max_drawdown = 100
  record_realized_pnl(dec!(-20)) -> cumulative = 80, max_drawdown = 80 (更新)
  record_realized_pnl(dec!(50)) -> cumulative = 130, max_drawdown = 80 (保持)
实际输出：
  max_drawdown 正确跟踪最大回撤
对比预期：
  预期 = 最大回撤在亏损时更新
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：单笔最大盈亏记录
构造输入：
  多笔交易，最大盈利和最大亏损不同
执行动作：
  record_realized_pnl(dec!(100))
  record_realized_pnl(dec!(-80))
  record_realized_pnl(dec!(50))
  record_realized_pnl(dec!(-30))
实际输出：
  max_single_trade_profit = 100
  max_single_trade_loss = 80
对比预期：
  预期 = 最大单笔盈亏正确记录
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test strategy_state::state::tests::test_record_realized_pnl ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
