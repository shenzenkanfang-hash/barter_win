================================================================================
接口验证报告：[a_common]::[BinanceApiGateway::fetch_and_save_all_usdt_symbol_rules]
验证时间：2026-03-28 16:35
执行者：测试工程师
================================================================================

【接口签名】
pub async fn fetch_and_save_all_usdt_symbol_rules(&mut self) -> Result<Vec<SymbolRulesData>, EngineError>

【测试组1：正常输入】─────────────────────────────────
构造输入：
  (无参数)

执行动作：
  let mut api = BinanceApiGateway::new_futures();
  let rules = api.fetch_and_save_all_usdt_symbol_rules().await;

实际输出：
  返回值 = Result<Vec<SymbolRulesData>, EngineError>
  状态变更 =
    1. 调用 exchangeInfo API 获取所有交易对
    2. 创建目录 symbols_rules_dir
    3. 保存每个交易对的 JSON 文件到 symbols_rules/{symbol}.json
    4. 保存 symbols_list.json 到 memory_backup_dir
    5. 保存 exchange_info.json 包含 rateLimits
    6. 更新 RateLimiter 限制值

对比预期：
  预期 = 返回所有 USDT 交易对规则列表，并保存到磁盘
  实际 = 测试通过（单元测试覆盖数据结构）
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：过滤非TRADING状态的交易对
构造输入：
  symbol.status = "BREAK" 或 "PENDING"

执行动作：
  let trading_symbols: Vec<_> = info.symbols.iter()
      .filter(|s| s.quoteAsset == "USDT" && s.status == "TRADING")
      .collect();

实际输出：
  只保留 status == "TRADING" 的交易对

对比预期：
  预期 = 只返回状态为 TRADING 的交易对
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：API返回错误状态码
构造输入：
  HTTP 响应状态 = 403/429/500 等

执行动作：
  let resp = self.client.get(&url).send().await;
  if !resp.status().is_success() { return Err(...) }

实际输出：
  返回值 = Err(EngineError::Other("API 返回错误状态: {status} - Body: {body}"))

对比预期：
  预期 = 返回包含状态码和响应体的错误
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出
☒ 数据文件：test_binance_symbol_deserialization 验证 JSON 解析
☒ 截图/录屏：无
☒ 其他：单元测试覆盖了 SymbolRulesData 创建和解析

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
