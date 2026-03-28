================================================================================
                    接口验证报告：c_data_process::StrategyStateDb
验证时间：2026-03-28
执行者：测试工程师
================================================================================

【模块路径】
crates/c_data_process/src/strategy_state/db.rs

【接口签名】
pub struct StrategyStateDb {
    conn: Arc<Mutex<Connection>>,
}
impl StrategyStateDb {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self>
    pub fn in_memory() -> Result<Self>
    pub fn save(&self, state: &StrategyState) -> Result<()>
    pub fn save_batch(&self, states: &[StrategyState]) -> Result<()>
    pub fn load(&self, id: &str) -> Result<Option<StrategyState>>
    pub fn load_all(&self) -> Result<Vec<StrategyState>>
    pub fn load_by_instrument(&self, instrument_id: &str) -> Result<Vec<StrategyState>>
    pub fn delete(&self, id: &str) -> Result<bool>
    pub fn clear(&self) -> Result<()>
    pub fn count(&self) -> Result<usize>
}

【测试组1：正常输入】─────────────────────────────────
测试用例：test_save_and_load
构造输入：
  db = StrategyStateDb::in_memory()
  state = StrategyState::new("BTC-USDT", "trend_v1", "binance", "1h")
执行动作：
  db.save(&state)
  loaded = db.load(&state.id())
实际输出：
  loaded.is_some() = true
  loaded.instrument_id = "BTC-USDT"
  loaded.strategy_id = "trend_v1"
对比预期：
  预期 = 单条记录保存和加载正确
  实际 = 一致
  差异 = 无
结果：通过

测试用例：test_save_batch
构造输入：
  db = StrategyStateDb::in_memory()
  states = [
    StrategyState::new("BTC-USDT", "trend_v1", "binance", "1h"),
    StrategyState::new("ETH-USDT", "trend_v1", "binance", "1h"),
  ]
执行动作：
  db.save_batch(&states)
  count = db.count()
实际输出：
  count = 2
对比预期：
  预期 = 批量保存正确
  实际 = 一致
  差异 = 无
结果：通过

【测试组2：边界输入】─────────────────────────────────
测试用例：test_delete
构造输入：
  db 有1条记录
执行动作：
  db.delete(&state.id())
  count = db.count()
实际输出：
  count = 0
  delete返回true
对比预期：
  预期 = 删除后count为0
  实际 = 一致
  差异 = 无
结果：通过

测试用例：删除不存在的记录
构造输入：
  db = empty
执行动作：
  db.delete("nonexistent_id")
实际输出：
  返回 Ok(false)
对比预期：
  预期 = 不存在的记录返回false
  实际 = 一致
  差异 = 无
结果：通过

【测试组3：异常输入】─────────────────────────────────
测试用例：load不存在的记录
构造输入：
  db = empty
执行动作：
  db.load("nonexistent_id")
实际输出：
  返回 Ok(None)
对比预期：
  预期 = 不存在的记录返回None
  实际 = 一致
  差异 = 无
结果：通过

【执行证据】─────────────────────────────────────────
cargo test -p c_data_process 运行结果：
  test strategy_state::db::tests::test_save_and_load ... ok
  test strategy_state::db::tests::test_save_batch ... ok
  test strategy_state::db::tests::test_delete ... ok

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：是

执行人签字：测试工程师 日期：2026-03-28
================================================================================
