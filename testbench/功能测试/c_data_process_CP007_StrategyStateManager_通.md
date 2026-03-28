================================================================================
                    接口验证报告：c_data_process::StrategyStateManager
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/strategy_state/mod.rs

【接口签名】
pub struct StrategyStateManager {
    db: StrategyStateDb,
    cache: Arc<RwLock<FnvHashMap<String, StrategyState>>>,
}
impl StrategyStateManager {
    pub fn new(db: StrategyStateDb) -> Self
    pub fn with_db_path<P: AsRef<std::path::Path>>(db_path: P) -> Result<Self>
    pub fn get_or_create(&self, instrument_id: &str, strategy_id: &str, exchange: &str, channel: &str) -> Result<StrategyState>
    pub fn get(&self, instrument_id: &str, strategy_id: &str) -> Result<Option<StrategyState>>
    pub fn get_all(&self) -> Result<Vec<StrategyState>>
    pub fn get_by_instrument(&self, instrument_id: &str) -> Result<Vec<StrategyState>>
    pub fn update_cache(&self, state: StrategyState) -> Result<()>
    pub fn update_position(&self, instrument_id: &str, strategy_id: &str, side: PositionSide, qty: Decimal, price: Decimal) -> Result<()>
    pub fn update_unrealized_pnl(&self, instrument_id: &str, strategy_id: &str, current_price: Decimal) -> Result<()>
    pub fn record_realized_pnl(&self, instrument_id: &str, strategy_id: &str, pnl: Decimal) -> Result<()>
    pub fn update_risk(&self, instrument_id: &str, strategy_id: &str, stop_loss: Decimal, take_profit: Decimal) -> Result<()>
    pub fn set_trading(&self, instrument_id: &str, strategy_id: &str, enabled: bool) -> Result<()>
    pub fn increment_error(&self, instrument_id: &str, strategy_id: &str) -> Result<()>
    pub fn reset_error(&self, instrument_id: &str, strategy_id: &str) -> Result<()>
    pub fn sync_to_db(&self) -> Result<()>
    pub fn load_from_db(&self) -> Result<()>
    pub fn delete(&self, instrument_id: &str, strategy_id: &str) -> Result<bool>
    pub fn cache_size(&self) -> usize
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_manager_get_or_create
构造输入：
  db = StrategyStateDb::in_memory()
  manager = StrategyStateManager::new(db)
执行动作：
  manager.get_or_create("BTC-USDT", "trend_v1", "binance", "1h")
实际输出：
  state.instrument_id = "BTC-USDT"
  state.strategy_id = "trend_v1"
  state.exchange = "binance"
  state.channel = "1h"
对比预期：
  预期 = 策略状态正确创建
  实际 = 一致
  差异 = 无
结果：通过

测试用例：test_manager_update_position
构造输入：
  已创建的策略状态
执行动作：
  manager.update_position("BTC-USDT", "trend_v1", PositionSide::Long, dec!(0.1), dec!(50000))
  manager.get("BTC-USDT", "trend_v1")
实际输出：
  position.current = 0.1
  position.side = Long
  position.avg_entry_price = 50000
对比预期：
  预期 = 持仓状态正确更新
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：test_manager_sync (缓存与数据库同步)
构造输入：
  manager1 有状态更新后 sync_to_db
  manager2 从数据库加载
执行动作：
  manager.sync_to_db()
  manager2.load_from_db()
  manager2.get()
实际输出：
  manager2 能获取到 manager1 更新的状态
对比预期：
  预期 = 缓存与数据库同步正确
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：get不存在的策略
构造输入：
  manager = new() 未调用get_or_create
执行动作：
  manager.get("NONEXISTENT", "strategy")
实际输出：
  返回 Ok(None)
对比预期：
  预期 = 不存在的策略返回None，不崩溃
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test strategy_state::tests::test_manager_get_or_create ... ok
  test strategy_state::tests::test_manager_update_position ... ok
  test strategy_state::tests::test_manager_sync ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
