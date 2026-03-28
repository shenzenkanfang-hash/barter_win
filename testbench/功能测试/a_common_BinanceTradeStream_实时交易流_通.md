================================================================================
接口验证报告：[a_common]::[BinanceTradeStream]
验证时间：2026-03-28 16:55
执行者：测试工程师
================================================================================

【接口签名】
pub struct BinanceTradeStream {
    ws_stream: SplitStream<...>,
    symbol: String,
}

pub async fn next_message(&mut self) -> Option<String>
pub fn parse_trade(&self, text: &str) -> Option<BinanceTradeMsg>
pub fn parse_kline(&self, text: &str) -> Option<BinanceKlineMsg>
pub fn parse_depth(&self, text: &str) -> Option<BinanceDepthMsg>

【测试组1：正常输入】─────────────────────────────────
构造输入：
  原始 WS JSON 消息 = {"e":"trade","E":1234567890,"s":"BTCUSDT","t":123,"p":"50000","q":"1","T":1234567890,"m":true}

执行动作：
  let msg: Option<BinanceTradeMsg> = stream.parse_trade(&json_text);

实际输出：
  BinanceTradeMsg {
      event_type: "trade",
      event_time: 1234567890,
      symbol: "BTCUSDT",
      trade_id: 123,
      price: "50000",
      quantity: "1",
      trade_time: 1234567890,
      is_buyer_maker: true
  }

对比预期：
  预期 = 正确解析所有字段
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：解析 Kline 消息
构造输入：
  原始 WS JSON = {"e":"kline","E":123456789,"s":"BTCUSDT","k":{"t":123,"T":124,"s":"BTCUSDT","i":"1m","f":1,"L":1,"o":"50000","c":"50100","h":"50200","l":"49900","v":"1","n":1,"x":false}}

执行动作：
  let kline: Option<BinanceKlineMsg> = stream.parse_kline(&json_text);

实际输出：
  BinanceKlineMsg {
      event_type: "kline",
      kline: KlineData {
          open: "50000",
          close: "50100",
          high: "50200",
          low: "49900",
          ...
      }
  }

对比预期：
  预期 = KlineData 正确解析
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：无效 JSON 格式
构造输入：
  json_text = "not a valid json"

执行动作：
  let result = stream.parse_trade(&json_text);

实际输出：
  返回值 = None (serde_json::from_str 返回 Err)

对比预期：
  预期 = 返回 None，不 panic
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出
☒ 数据文件：单元测试验证了所有消息类型解析
☒ 截图/录屏：无
☒ 其他：BinanceTradeMsg/KlineMsg/DepthMsg 反序列化测试通过

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
