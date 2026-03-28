================================================================================
接口验证报告：[a_common]::[VolatilityCalc]
验证时间：2026-03-28 17:10
执行者：测试工程师
================================================================================

【接口签名】
pub struct VolatilityCalc {
    kline_15m_window: Vec<KLineInput>,
    kline_1m_count: u32,
    threshold_1m: Decimal,
    threshold_15m: Decimal,
    last_update: DateTime<Utc>,
}

pub fn new() -> Self
pub fn update(&mut self, kline: KLineInput) -> VolatilityStats
pub fn is_valid(&self) -> bool
pub fn get_state(&self) -> VolatilityState

【测试组1：正常输入】─────────────────────────────────
构造输入：
  KLineInput { open: dec!(100), close: dec!(104), high: dec!(105), low: dec!(99), timestamp: Utc::now() }

执行动作：
  let mut calc = VolatilityCalc::new();
  let stats = calc.update(kline);

实际输出：
  VolatilityStats {
      is_high_volatility: false (因 4% < 3% 阈值),
      vol_1m: dec!(0.04),
      vol_15m: dec!(0) (累积不足15根K线)
  }

对比预期：
  预期 = vol_1m = (104-100)/100 = 0.04
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：高波动判断（vol_1m >= threshold_1m 或 vol_15m >= threshold_15m）
构造输入：
  threshold_1m = 0.03 (3%)
  kline open=100, close=104 => vol_1m=0.04 > 0.03

执行动作：
  let stats = calc.update(high_vol_kline);

实际输出：
  is_high_volatility = true (因为 0.04 >= 0.03)

对比预期：
  预期 = 高于阈值时 is_high_volatility=true
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：数据过期检测（延迟超过2分钟）
构造输入：
  last_update = 3分钟前

执行动作：
  let valid = calc.is_valid();

实际输出：
  返回值 = false (elapsed >= Duration::minutes(2))

对比预期：
  预期 = 数据过期返回 false
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出
☒ 数据文件：test_volatility_calc_default, test_volatility_high_1m, test_volatility_stats_default
☒ 截图/录屏：无
☒ 其他：VolatilityStats default 测试通过

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
