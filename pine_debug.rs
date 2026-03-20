//! Pine 指标单独调试程序
//!
//! 只计算 Pine Script v5 相关指标，便于对比验证

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

// ==================== EMA ====================

struct EMA {
    period: usize,
    value: Decimal,
}

impl EMA {
    fn new(period: usize) -> Self {
        Self {
            period,
            value: Decimal::ZERO,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        if self.value == Decimal::ZERO {
            self.value = price;
        } else {
            // Python _sma_or_ema uses alpha = 1.0 / window
            let alpha = dec!(1) / Decimal::from(self.period);
            self.value = price * alpha + self.value * (dec!(1) - alpha);
        }
        self.value
    }

    fn get(&self) -> Decimal {
        self.value
    }
}

// ==================== RMA (RSI 平滑) ====================

struct RMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}

impl RMA {
    fn new(period: usize) -> Self {
        Self {
            period,
            alpha: dec!(1) / Decimal::from(period),
            value: Decimal::ZERO,
            initialized: false,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        if !self.initialized {
            self.value = price;
            self.initialized = true;
        } else {
            self.value = price * self.alpha + self.value * (dec!(1) - self.alpha);
        }
        self.value
    }
}

// ==================== Dominant Cycle RSI ====================

struct DominantCycleRSI {
    period: usize,
    cyclelen: usize,
    torque: Decimal,
    phasinglag: usize,
    rma_up: RMA,
    rma_down: RMA,
    crsi: Decimal,
    last_price: Decimal,
    rsi_history: VecDeque<Decimal>,
}

impl DominantCycleRSI {
    fn new(period: usize) -> Self {
        let cyclelen = period / 2;
        let torque = dec!(2) / Decimal::from(11); // vibration=10 -> 2/(10+1)
        let phasinglag = 4; // (10-1)/2 = 4.5 -> 4

        Self {
            period,
            cyclelen,
            torque,
            phasinglag,
            rma_up: RMA::new(cyclelen),
            rma_down: RMA::new(cyclelen),
            crsi: Decimal::ZERO,
            last_price: Decimal::ZERO,
            rsi_history: VecDeque::new(),
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        let change = if self.last_price > Decimal::ZERO {
            price - self.last_price
        } else {
            Decimal::ZERO
        };
        self.last_price = price;

        let up = if change > Decimal::ZERO { change } else { -change };
        let down = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        let rsi = if down == Decimal::ZERO {
            dec!(100)
        } else if up == Decimal::ZERO {
            dec!(0)
        } else {
            dec!(100) - dec!(100) / (dec!(1) + up / down)
        };

        let rma_up_val = self.rma_up.update(up);
        let rma_down_val = self.rma_down.update(down);

        let rsi_rma = if rma_down_val == Decimal::ZERO {
            dec!(100)
        } else if rma_up_val == Decimal::ZERO {
            dec!(0)
        } else {
            dec!(100) - dec!(100) / (dec!(1) + rma_up_val / rma_down_val)
        };

        self.rsi_history.push_back(rsi_rma);

        let rsi_lagged = self.get_rsi_lagged();

        let crsi_calc = if rsi_lagged.is_some() {
            self.torque * (dec!(2) * rsi_rma - rsi_lagged.unwrap()) +
            (dec!(1) - self.torque) * self.crsi
        } else {
            self.torque * (dec!(2) * rsi_rma) +
            (dec!(1) - self.torque) * self.crsi
        };

        self.crsi = crsi_calc;
        self.crsi
    }

    fn get_rsi_lagged(&self) -> Option<Decimal> {
        let len = self.rsi_history.len();
        if len > self.phasinglag {
            self.rsi_history.get(len - 1 - self.phasinglag).copied()
        } else {
            None
        }
    }

    fn get_value(&self) -> Decimal {
        self.crsi
    }
}

// ==================== PineColor Detector ====================

// Pine颜色映射 (对应 Python PINE_COLOR_MAP)
mod colors {
    pub const STRONG_TOP: &'static str = "浅蓝";      // selltimeS/buytimeS
    pub const STRONG_BOT: &'static str = "橙色";      // (unused in current logic)
    pub const TOP_WARNING: &'static str = "浅黄";     // selltime
    pub const BOTTOM_WARNING: &'static str = "纯蓝";  // buytime
    pub const WEAK_SIGNAL: &'static str = "纯红";     // selltimeT/buytimeT
    pub const BULL_TREND: &'static str = "纯绿";      // isUp
    pub const BEAR_TREND: &'static str = "紫色";      // isDown
    pub const DEFAULT: &'static str = "白色";         // default

    // BG颜色映射 (对应 Python PINE_BG_COLOR_MAP)
    pub const BULL_TREND_BG: &'static str = "纯绿";      // macd >= signal && macd >= 0
    pub const BULL_CONSOLIDATION: &'static str = "浅绿";  // macd <= signal && macd >= 0
    pub const BEAR_TREND_BG: &'static str = "纯红";      // macd <= signal && macd <= 0
    pub const BEAR_CONSOLIDATION: &'static str = "浅红"; // macd >= signal && macd <= 0
    pub const DEFAULT_BG: &'static str = "白色";
}

struct PineColorDetector {
    macd_fast: EMA,   // fast=100, slow=200 for MACD
    macd_slow: EMA,
    signal_ema: EMA,  // signal=9
    ema10: EMA,       // fixed 10
    ema20: EMA,       // fixed 20
    hist_prev: Option<Decimal>,
    rsi: DominantCycleRSI,
}

impl PineColorDetector {
    fn new() -> Self {
        Self {
            // MACD 使用 Python 的 100/200 参数
            macd_fast: EMA::new(100),
            macd_slow: EMA::new(200),
            signal_ema: EMA::new(9),
            // EMA10/EMA20 固定 10/20
            ema10: EMA::new(10),
            ema20: EMA::new(20),
            hist_prev: None,
            rsi: DominantCycleRSI::new(20),
        }
    }

    fn update(&mut self, close: Decimal) -> (String, String, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal) {
        let fast_ma = self.macd_fast.update(close);
        let slow_ma = self.macd_slow.update(close);
        let macd = fast_ma - slow_ma;
        let signal = self.signal_ema.update(macd);
        let hist = macd - signal;
        let ema10_val = self.ema10.update(close);
        let ema20_val = self.ema20.update(close);
        let rsi_val = self.rsi.update(close);

        let hist_prev = self.hist_prev;
        self.hist_prev = Some(hist);

        let bar_color = self.detect_bar_color(macd, signal, hist_prev, hist, rsi_val, ema10_val, ema20_val);
        let bg_color = self.detect_bg_color(macd, signal);

        (bar_color, bg_color, macd, signal, hist, ema10_val, ema20_val, rsi_val)
    }

    /// 检测bar颜色 - 匹配Python的优先级顺序
    fn detect_bar_color(&self, macd: Decimal, _signal: Decimal, hist_prev: Option<Decimal>, hist: Decimal, rsi: Decimal, ema10_val: Decimal, ema20_val: Decimal) -> String {
        let hist_prev_val = match hist_prev {
            Some(v) => v,
            None => return colors::DEFAULT.to_string(),
        };

        let ema20_above_ema10 = ema20_val > ema10_val;
        let ema20_below_ema10 = ema20_val < ema10_val;
        let is_up = rsi >= dec!(70);
        let is_down = rsi <= dec!(30);

        let selltimeS = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO && is_up;
        let buytimeS = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO && is_down;
        let selltimeT = macd <= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO;
        let buytimeT = macd >= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO;
        let selltime = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO;
        let buytime = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO;

        // Python 优先级顺序 (检查顺序很重要):
        // 1. st (selltimeT) -> 纯红 (weak_signal)
        // 2. bt (buytimeT) -> 纯红 (weak_signal)
        // 3. sl (selltime) -> 浅黄 (top_warning)
        // 4. by (buytime) -> 纯蓝 (bottom_warning)
        // 5. up (isUp) -> 纯绿 (bull_trend)
        // 6. dn (isDown) -> 紫色 (bear_trend)
        // 7. s (selltimeS) -> 浅蓝 (strong_top)
        // 8. b (buytimeS) -> 橙色 (strong_bot)

        // 注意: Python 代码中 selltimeS/buytimeS 的优先级最低！

        // 检查优先级1-2: selltimeT/buytimeT -> 纯红
        if selltimeT || buytimeT { return colors::WEAK_SIGNAL.to_string(); }  // 纯红
        // 检查优先级3-4: selltime/buytime
        if selltime { return colors::TOP_WARNING.to_string(); }                // 浅黄
        if buytime { return colors::BOTTOM_WARNING.to_string(); }               // 纯蓝
        // 检查优先级5-6: isUp/isDown
        if is_up { return colors::BULL_TREND.to_string(); }                    // 纯绿
        if is_down { return colors::BEAR_TREND.to_string(); }                   // 紫色
        // 检查优先级7-8: selltimeS/buytimeS -> 浅蓝/橙色
        if selltimeS { return colors::STRONG_TOP.to_string(); }                // 浅蓝
        if buytimeS { return colors::STRONG_BOT.to_string(); }                  // 橙色

        colors::DEFAULT.to_string()
    }

    /// 检测背景颜色 - 匹配Python逻辑
    fn detect_bg_color(&self, macd: Decimal, signal: Decimal) -> String {
        // Python 顺序:
        // macd >= sig && macd >= 0 -> 纯绿 (bull_trend)
        // macd <= sig && macd <= 0 -> 纯红 (bear_trend)
        // macd <= sig && macd >= 0 -> 浅绿 (bull_consolidation)
        // macd >= sig && macd <= 0 -> 浅红 (bear_consolidation)
        if macd >= signal && macd >= Decimal::ZERO {
            colors::BULL_TREND_BG.to_string()
        } else if macd <= signal && macd <= Decimal::ZERO {
            colors::BEAR_TREND_BG.to_string()
        } else if macd <= signal && macd >= Decimal::ZERO {
            colors::BULL_CONSOLIDATION.to_string()
        } else {
            colors::BEAR_CONSOLIDATION.to_string()
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================");
    println!("Pine 指标单独调试程序");
    println!("============================================\n");

    let args: Vec<String> = std::env::args().collect();

    // 支持两种模式:
    // 1. pine_debug --input <csv_file>  - 从 CSV 读取 close 数据
    // 2. pine_debug <symbol> <limit>    - 从币安 API 获取数据

    let mut klines_data: Vec<(String, Decimal)> = Vec::new();  // (date, close)

    if args.len() >= 3 && args[1] == "--input" {
        // 从 CSV 文件读取
        let csv_path = &args[2];
        println!("从 CSV 文件读取: {}\n", csv_path);

        let content = std::fs::read_to_string(csv_path)?;
        let mut reader = csv::Reader::from_reader(content.as_bytes());

        for (i, result) in reader.records().enumerate() {
            let record = result?;
            // CSV 格式: timestamp,close
            // 或者: index,timestamp,open,high,low,close,...

            // 尝试获取 date (可能在第0或第1列)
            let date = record.get(0)
                .map(|s| s.to_string())
                .unwrap_or_else(|| i.to_string());

            // 尝试获取 close (可能在第1或第5列)
            let close = record.get(1)
                .and_then(|s| s.parse::<Decimal>().ok())
                .or_else(|| {
                    record.get(5)
                        .and_then(|s| s.parse::<Decimal>().ok())
                })
                .unwrap_or(dec!(0));

            klines_data.push((date, close));
        }
        println!("从 CSV 读取了 {} 根K线\n", klines_data.len());
    } else {
        // 从币安 API 获取
        let symbol = std::env::args().nth(1).unwrap_or_else(|| "BTCUSDT".to_string());
        let limit = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "100".to_string())
            .parse()
            .unwrap_or(100);

        println!("交易对: {}", symbol);
        println!("获取K线数量: {}\n", limit);

        let url = format!(
            "https://api.binance.com/api/v3/klines?symbol={}&interval=1d&limit={}",
            symbol, limit
        );

        let response = reqwest::blocking::get(&url)?;
        let klines: Vec<Vec<serde_json::Value>> = response.json()?;

        println!("成功获取 {} 根K线\n", klines.len());

        for kline in klines {
            let open_time: i64 = kline[0].as_i64().unwrap_or(0);
            let close: Decimal = kline[4].as_str().unwrap().parse().unwrap_or(dec!(0));

            let dt = Utc.timestamp_millis_opt(open_time)
                .single()
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_default();

            klines_data.push((dt, close));
        }
    }

    let mut detector = PineColorDetector::new();

    // 输出带调试信息的CSV
    println!("date,index,close,macd,signal,hist,ema10,ema20,rsi,bar_color,bg_color,debug");

    for (i, (date, close)) in klines_data.iter().enumerate() {
        let (bar_color, bg_color, macd, signal, hist, ema10_val, ema20_val, rsi_val) = detector.update(*close);

        // 输出调试信息 (仅2026-03-14到2026-03-18)
        let debug = if date.as_str() >= "2026-03-14" && date.as_str() <= "2026-03-18" {
            format!("ema10={:.2},ema20={:.2},macd={:.2},hist={:.2}", ema10_val, ema20_val, macd, hist)
        } else {
            String::new()
        };

        println!("{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{}",
                 date, i, close, macd, signal, hist, ema10_val, ema20_val, rsi_val, bar_color, bg_color, debug);
    }

    println!("\n============================================");
    println!("Pine Script v5 指标说明 (匹配 Python):");
    println!("============================================");
    println!("MACD: 100/200/9 (fast=100, slow=200, signal=9)");
    println!("EMA: 固定10/20 (ema10/ema20)");
    println!("RSI: Dominant Cycle RSI (period=20, vibration=10, cyclelen=10)");
    println!("");
    println!("Bar Color 优先级 (匹配 Python calculate_bar_color):");
    println!("  1. selltimeS/buytimeS -> 浅蓝");
    println!("  2. selltimeT/buytimeT -> 纯红");
    println!("  3. selltime -> 浅黄");
    println!("  4. buytime -> 纯蓝");
    println!("  5. isUp (rsi>=70) -> 纯绿");
    println!("  6. isDown (rsi<=30) -> 紫色");
    println!("");
    println!("BG Color (匹配 Python calculate_bg_color):");
    println!("  - macd >= signal && macd >= 0 -> 纯绿");
    println!("  - macd <= signal && macd <= 0 -> 纯红");
    println!("  - macd <= signal && macd >= 0 -> 浅绿");
    println!("  - macd >= signal && macd <= 0 -> 浅红");

    Ok(())
}
