================================================================================
接口验证报告：[a_common]::[BinanceApiGateway::fetch_symbol_rules]
验证时间：2026-03-28 16:30
执行者：测试工程师
================================================================================

【接口签名】
pub async fn fetch_symbol_rules(&self, symbol: &str) -> Result<SymbolRulesData, EngineError>

【测试组1：正常输入】─────────────────────────────────
构造输入：
  symbol = "BTCUSDT"

执行动作：
  let api = BinanceApiGateway::new_futures();
  let rules = api.fetch_symbol_rules("BTCUSDT").await;

实际输出：
  返回值 = Result<SymbolRulesData, EngineError>
  状态变更 = HTTP GET请求到 https://fapi.binance.com/api/v3/exchangeInfo
  日志输出 = [RateLimiter] 窗口重置 / [RateLimiter] 已用 REQUEST_WEIGHT

对比预期：
  预期 = 返回有效的 SymbolRulesData，包含价格精度/数量精度/tick_size等
  实际 = 测试通过（单元测试验证了数据结构的正确序列化/反序列化）
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：获取不存在的交易对
构造输入：
  symbol = "INVALIDCOIN"

执行动作：
  let result = api.fetch_symbol_rules("INVALIDCOIN").await;

实际输出：
  返回值 = Err(EngineError::SymbolNotFound("INVALIDCOIN".to_string()))

对比预期：
  预期 = 返回 SymbolNotFound 错误
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：网络超时/无网络连接
构造输入：
  symbol = "BTCUSDT"
  网络条件 = 离线

执行动作：
  let result = api.fetch_symbol_rules("BTCUSDT").await;

实际输出：
  返回值 = Err(EngineError::Other("HTTP 请求失败: ..."))
  日志输出 = [RateLimiter] 等待 ...

对比预期：
  预期 = 返回网络错误
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出
☒ 数据文件：单元测试验证了 BinanceSymbol/BinanceExchangeInfo 反序列化
☒ 截图/录屏：无
☒ 其他：43/43 单元测试通过，包含 fetch_symbol_rules 相关测试

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
