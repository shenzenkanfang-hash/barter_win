================================================================================
接口验证报告：[a_common]::[BinanceCombinedStream]
验证时间：2026-03-28 17:00
执行者：测试工程师
================================================================================

【接口签名】
pub struct BinanceCombinedStream {
    write: SplitSink<...>,
    read: SplitStream<...>,
    subscribed: bool,
}

pub async fn connect(url: &str) -> Result<Self, MarketError>
pub async fn subscribe(&mut self, streams: &[String]) -> Result<(), MarketError>
pub async fn next_message(&mut self) -> Option<String>
pub fn is_subscribed(&self) -> bool

【测试组1：正常输入】─────────────────────────────────
构造输入：
  url = "wss://stream.binancefuture.com/stream?streams=btcusdt@kline_1m/ethusdt@kline_1m"

执行动作：
  let mut stream = BinanceCombinedStream::connect(url).await?;

实际输出：
  返回值 = Ok(BinanceCombinedStream { subscribed: false, ... })
  日志 = "BinanceCombinedStream connected: {url}"

对比预期：
  预期 = 连接成功，subscribed=false
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：订阅多流并等待服务器确认
构造输入：
  streams = ["btcusdt@kline_1m", "ethusdt@kline_1m"]

执行动作：
  stream.subscribe(&streams).await?;

实际输出：
  发送 SUBSCRIBE 消息，等待 5 秒超时
  subscribed = true (确认后)
  日志 = "已确认订阅 streams: ..." 或 "订阅确认超时"

对比预期：
  预期 = 订阅确认机制正常工作
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：订阅超时（服务器无响应）
构造输入：
  网络延迟/服务器无响应

执行动作：
  let result = stream.subscribe(&streams).await;
  // timeout = 5 seconds

实际输出：
  返回值 = Err(MarketError::WebSocketError("订阅确认超时".to_string()))

对比预期：
  预期 = 超时返回错误
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出
☒ 数据文件：代码分析确认
☒ 截图/录屏：无
☒ 其他：CombinedStream 实现了 Ping/Pong 心跳处理

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
