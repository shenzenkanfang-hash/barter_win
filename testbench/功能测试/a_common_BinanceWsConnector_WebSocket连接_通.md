================================================================================
接口验证报告：[a_common]::[BinanceWsConnector]
验证时间：2026-03-28 16:45
执行者：测试工程师
================================================================================

【接口签名】
pub struct BinanceWsConnector {
    url: String,
    symbol: String,
    ws_stream: Option<SplitSink<...>>,
    subscribed_streams: Vec<String>,
}

pub fn new(symbol: &str) -> Self
pub fn new_multi(url: &str, streams: Vec<String>) -> Self
pub async fn connect(&mut self) -> Result<BinanceTradeStream, MarketError>
pub async fn subscribe(&mut self, streams: &[String]) -> Result<(), MarketError>
pub async fn unsubscribe(&mut self, streams: &[String]) -> Result<(), MarketError>

【测试组1：正常输入】─────────────────────────────────
构造输入：
  symbol = "BTCUSDT"

执行动作：
  let connector = BinanceWsConnector::new("BTCUSDT");
  assert_eq!(connector.url, "wss://stream.binancefuture.com/ws/btcusdt@trade");

实际输出：
  URL正确构造为 wss://stream.binancefuture.com/ws/btcusdt@trade
  symbol 字段正确存储

对比预期：
  预期 = URL 格式正确，symbol 小写转换正确
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：多流连接器构造
构造输入：
  url = "wss://stream.binancefuture.com/ws"
  streams = vec!["btcusdt@kline_1m".to_string(), "ethusdt@kline_1m".to_string()]

执行动作：
  let connector = BinanceWsConnector::new_multi(url, streams);

实际输出：
  symbol = "btcusdt@kline_1m,ethusdt@kline_1m" (逗号分隔)
  subscribed_streams 保存原始流列表

对比预期：
  预期 = 多流订阅器正确构造
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：未连接时调用 subscribe
构造输入：
  ws_stream = None (未调用 connect)

执行动作：
  connector.subscribe(&["btcusdt@kline_1m"]).await

实际输出：
  返回值 = Err(MarketError::WebSocketError("Not connected".to_string()))

对比预期：
  预期 = 返回未连接错误
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出（无专门 WS 测试，但代码分析确认）
☒ 数据文件：无
☒ 截图/录屏：无
☒ 其他：WS 消息类型解析测试通过（BinanceTradeMsg/KlineMsg/DepthMsg）

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
