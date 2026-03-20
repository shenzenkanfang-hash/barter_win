//! 指标对比验证程序
//!
//! 从币安获取1000根日线数据，使用分钟级指标计算，输出CSV进行对比验证

use chrono::{DateTime, TimeZone, Utc};
use indicator::{BigCycleCalculator, EMA, PricePosition, RSI};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

/// 币安K线数据格式
#[derive(Debug, Clone)]
struct BinanceKline {
    open_time: u64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    close_time: u64,
}

/// CSV 输出行
#[derive(Debug)]
struct CsvRow {
    timestamp: i64,
    parent_1m_ts: i64,
    tick_index: i64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
    tr_ratio_5d_all: Decimal,
    tr_ratio_5d_20d_rank_20d: Decimal,
    tr_ratio_5d_20d: Decimal,
    tr_base_5d: Decimal,
    tr_ratio_20d_all: Decimal,
    tr_ratio_20d_60d_rank_20d: Decimal,
    tr_ratio_20d_60d: Decimal,
    tr_base_20d: Decimal,
    readable_time: String,
    jerk_signal: Decimal,
    top3_avg_amplitude_pct: Decimal,
    pine_bar_color_20_50: String,
    pine_bg_color_20_50: String,
    pine_bar_color_100_200: String,
    pine_bg_color_100_200: String,
    pine_bar_color_12_26: String,
    pine_bg_color_12_26: String,
    real_time_tr_5d: Decimal,
    real_time_tr_20d: Decimal,
    ema_100_200_compare: String,
    real_time_tr_ratio_5d_20d: Decimal,
    real_time_tr_ratio_20d_60d: Decimal,
    pos_norm_20: Decimal,
    ma5_close_in_20d_ma5_pos: Decimal,
    ma20_close_in_60d_ma20_pos: Decimal,
    ma5_close_in_all_ma5_pos: Decimal,
    ma20_close_in_all_ma20_pos: Decimal,
    vel_percentile_d: Decimal,
    acc_percentile_d: Decimal,
    power: Decimal,
    power_percentile_60d: Decimal,
    strategy_calc_ts: i64,
}

/// 实时TR计算器
struct RealTimeTR {
    history: VecDeque<Decimal>,
    max_len: usize,
}

impl RealTimeTR {
    fn new(max_len: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_len),
            max_len,
        }
    }

    fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, prev_close: Decimal) {
        let tr = Self::calculate_tr(high, low, close, prev_close);
        self.history.push_back(tr);
        if self.history.len() > self.max_len {
            self.history.pop_front();
        }
    }

    fn calculate_tr(high: Decimal, low: Decimal, close: Decimal, prev_close: Decimal) -> Decimal {
        let h_l = high - low;
        let h_c = (high - close).abs();
        let l_c = (low - close).abs();
        h_l.max(h_c).max(l_c) / prev_close
    }

    fn avg(&self) -> Decimal {
        if self.history.is_empty() {
            return dec!(0);
        }
        let sum: Decimal = self.history.iter().sum();
        sum / Decimal::from(self.history.len())
    }
}

/// 速度百分位计算器
struct VelocityPercentile {
    tr_history: VecDeque<Decimal>,
    window: usize,
}

impl VelocityPercentile {
    fn new(window: usize) -> Self {
        Self {
            tr_history: VecDeque::with_capacity(window * 2),
            window,
        }
    }

    fn update(&mut self, tr: Decimal) -> Decimal {
        self.tr_history.push_back(tr);
        if self.tr_history.len() > self.window * 2 {
            self.tr_history.pop_front();
        }

        if self.tr_history.len() < self.window {
            return dec!(50);
        }

        let current = *self.tr_history.back().unwrap();
        let sorted: Vec<_> = self.tr_history.iter().take(self.window).cloned().collect();
        let rank = sorted.iter().filter(|&&x| x < current).count();
        Decimal::from(rank) / Decimal::from(sorted.len()) * dec!(100)
    }
}

/// 功率计算器
struct PowerCalculator {
    tr_history: VecDeque<Decimal>,
    velocity_history: VecDeque<Decimal>,
    acc_history: VecDeque<Decimal>,
}

impl PowerCalculator {
    fn new() -> Self {
        Self {
            tr_history: VecDeque::with_capacity(60),
            velocity_history: VecDeque::with_capacity(60),
            acc_history: VecDeque::with_capacity(60),
        }
    }

    fn update(&mut self, tr: Decimal) -> (Decimal, Decimal, Decimal, Decimal) {
        // Velocity: TR变化率
        let velocity = if let Some(prev) = self.tr_history.back() {
            if *prev > dec!(0) {
                (tr - *prev) / *prev
            } else {
                dec!(0)
            }
        } else {
            dec!(0)
        };

        // Acceleration: 速度变化率
        let acceleration = if let Some(prev_vel) = self.velocity_history.back() {
            if *prev_vel > dec!(0) {
                (velocity - *prev_vel) / *prev_vel
            } else {
                dec!(0)
            }
        } else {
            dec!(0)
        };

        self.tr_history.push_back(tr);
        self.velocity_history.push_back(velocity);
        self.acc_history.push_back(acceleration);

        if self.tr_history.len() > 60 {
            self.tr_history.pop_front();
        }
        if self.velocity_history.len() > 60 {
            self.velocity_history.pop_front();
        }
        if self.acc_history.len() > 60 {
            self.acc_history.pop_front();
        }

        // Power = velocity * acceleration (简化)
        let power = velocity * acceleration;

        // Percentiles (简化计算)
        let vel_pct = self.percentile(&self.velocity_history, velocity);
        let acc_pct = self.percentile(&self.acc_history, acceleration);
        let power_pct = self.percentile(&self.velocity_history, power);

        (velocity, acceleration, power, power_pct)
    }

    fn percentile(&self, history: &VecDeque<Decimal>, value: Decimal) -> Decimal {
        if history.is_empty() {
            return dec!(50);
        }
        let sorted: Vec<_> = history.iter().cloned().collect();
        let rank = sorted.iter().filter(|&&x| x < value).count();
        Decimal::from(rank) / Decimal::from(sorted.len()) * dec!(100)
    }
}

impl Default for PowerCalculator {
    fn default() -> Self {
        Self::new()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================");
    println!("指标对比验证程序");
    println!("从币安获取1000根日线，计算指标输出CSV");
    println!("============================================\n");

    // 获取命令行参数
    let symbol = std::env::args().nth(1).unwrap_or_else(|| "BTCUSDT".to_string());
    let limit = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "1000".to_string())
        .parse()
        .unwrap_or(1000);

    println!("交易对: {}", symbol);
    println!("获取K线数量: {}\n", limit);

    // 从币安API获取K线数据
    let url = format!(
        "https://api.binance.com/api/v3/klines?symbol={}&interval=1d&limit={}",
        symbol, limit
    );
    println!("请求URL: {}\n", url);

    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send()?;
    let text = response.text()?;
    let klines_raw: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

    // 解析K线数据
    let mut klines = Vec::new();
    for arr in klines_raw {
        if let Some(items) = arr.as_array() {
            if items.len() >= 6 {
                klines.push(BinanceKline {
                    open_time: items[0].as_i64().unwrap_or(0) as u64,
                    open: items[1].as_str().unwrap_or("0").to_string(),
                    high: items[2].as_str().unwrap_or("0").to_string(),
                    low: items[3].as_str().unwrap_or("0").to_string(),
                    close: items[4].as_str().unwrap_or("0").to_string(),
                    volume: items[5].as_str().unwrap_or("0").to_string(),
                    close_time: items[6].as_i64().unwrap_or(0) as u64,
                });
            }
        }
    }

    println!("成功获取 {} 根K线\n", klines.len());

    // 初始化计算器
    let mut big_cycle = BigCycleCalculator::new();
    let mut real_time_tr_5d = RealTimeTR::new(5);
    let mut real_time_tr_20d = RealTimeTR::new(20);
    let mut vel_percentile = VelocityPercentile::new(60);
    let mut power_calc = PowerCalculator::new();

    // EMA 用于比较
    let mut ema_100 = EMA::new(100);
    let mut ema_200 = EMA::new(200);

    // RSI
    let mut rsi = RSI::new(14);

    // PricePosition
    let mut price_pos = PricePosition::new(20);

    // 准备CSV输出
    let output_path = format!("indicator_comparison_{}.csv", symbol.to_lowercase());
    let mut csv_content = String::new();

    // CSV表头
    csv_content.push_str(&format!(
        "timestamp,parent_1m_ts,tick_index,open,high,low,volume,",
    ));
    csv_content.push_str(&format!(
        "tr_ratio_5d_all,tr_ratio_5d_20d_rank_20d,tr_ratio_5d_20d,tr_base_5d,",
    ));
    csv_content.push_str(&format!(
        "tr_ratio_20d_all,tr_ratio_20d_60d_rank_20d,tr_ratio_20d_60d,tr_base_20d,",
    ));
    csv_content.push_str(&format!(
        "readable_time,jerk_signal,top3_avg_amplitude_pct,",
    ));
    csv_content.push_str(&format!(
        "pine_bar_color_20_50,pine_bg_color_20_50,",
    ));
    csv_content.push_str(&format!(
        "pine_bar_color_100_200,pine_bg_color_100_200,",
    ));
    csv_content.push_str(&format!(
        "pine_bar_color_12_26,pine_bg_color_12_26,",
    ));
    csv_content.push_str(&format!(
        "real_time_tr_5d,real_time_tr_20d,ema_100_200_compare,",
    ));
    csv_content.push_str(&format!(
        "real_time_tr_ratio_5d_20d,real_time_tr_ratio_20d_60d,",
    ));
    csv_content.push_str(&format!(
        "pos_norm_20,ma5_close_in_20d_ma5_pos,ma20_close_in_60d_ma20_pos,",
    ));
    csv_content.push_str(&format!(
        "ma5_close_in_all_ma5_pos,ma20_close_in_all_ma20_pos,",
    ));
    csv_content.push_str(&format!(
        "vel_percentile_d,acc_percentile_d,power,power_percentile_60d,",
    ));
    csv_content.push_str("strategy_calc_ts\n");

    let mut prev_close = None;
    let mut tick_index = 0i64;

    for kline in &klines {
        let open_time = kline.open_time as i64;
        let open = parse_decimal(&kline.open);
        let high = parse_decimal(&kline.high);
        let low = parse_decimal(&kline.low);
        let close = parse_decimal(&kline.close);
        let volume = parse_decimal(&kline.volume);

        let readable_time = Utc.timestamp_millis_opt(open_time)
            .single()
            .map(|dt: DateTime<Utc>| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        // 更新大周期计算器
        big_cycle.update(high, low, close);
        big_cycle.update_pine_ema(high, low, close);

        // 实时TR计算
        if let Some(prev) = prev_close {
            real_time_tr_5d.update(high, low, close, prev);
            real_time_tr_20d.update(high, low, close, prev);
        }
        prev_close = Some(close);

        // 计算实时TR比率
        let rt_tr_5d_avg = real_time_tr_5d.avg();
        let rt_tr_20d_avg = real_time_tr_20d.avg();
        let rt_tr_ratio_5d_20d = if rt_tr_20d_avg > dec!(0) {
            real_time_tr_20d.history.back().copied().unwrap_or(dec!(0)) / rt_tr_20d_avg
        } else {
            dec!(0)
        };
        let rt_tr_ratio_20d_60d = if rt_tr_20d_avg > dec!(0) && big_cycle.tr_60d_avg() > dec!(0) {
            rt_tr_20d_avg / big_cycle.tr_60d_avg()
        } else {
            dec!(0)
        };

        // 速度百分位
        let rt_tr = real_time_tr_20d.history.back().copied().unwrap_or(dec!(0));
        let vel_pct = vel_percentile.update(rt_tr);

        // 功率计算
        let (velocity, acceleration, power, power_pct) = power_calc.update(rt_tr);

        // EMA比较
        let ema_100_val = ema_100.calculate(close);
        let ema_200_val = ema_200.calculate(close);
        let ema_compare = if ema_100_val > ema_200_val {
            "above".to_string()
        } else if ema_100_val < ema_200_val {
            "below".to_string()
        } else {
            "equal".to_string()
        };

        // RSI和PricePosition
        let _rsi_value = rsi.calculate(close);
        let _price_position = price_pos.calculate(close, high, low);

        // 获取大周期指标
        let indicators = big_cycle.calculate(high, low, close);

        // PineColor字符串
        let pine_bar_20_50 = format!("{:?}", indicators.pine_color_20_50);
        let pine_bg_20_50 = format!("{:?}", indicators.pine_color_20_50);
        let pine_bar_100_200 = format!("{:?}", indicators.pine_color_100_200);
        let pine_bg_100_200 = format!("{:?}", indicators.pine_color_100_200);
        let pine_bar_12_26 = format!("{:?}", indicators.pine_color_12_26);
        let pine_bg_12_26 = format!("{:?}", indicators.pine_color_12_26);

        // 简化的jerk_signal和top3_avg_amplitude_pct
        let jerk_signal = acceleration;
        let top3_avg_amplitude_pct = dec!(0); // 简化

        // ma5_in_all_ma5_pos (简化)
        let ma5_in_all_ma5_pos = indicators.ma5_in_20d_ma5_pos;
        let ma20_in_all_ma20_pos = indicators.ma20_in_60d_ma20_pos;

        // 写入CSV行
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},",
            open_time, 0, tick_index, open, high, low, close,
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},",
            dec!(0), dec!(0), indicators.tr_ratio_5d_20d, big_cycle.tr_5d_avg(),
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},",
            dec!(0), dec!(0), indicators.tr_ratio_20d_60d, big_cycle.tr_20d_avg(),
        ));
        csv_content.push_str(&format!(
            "{},{},{},",
            readable_time, jerk_signal, top3_avg_amplitude_pct,
        ));
        csv_content.push_str(&format!(
            "{},{},", pine_bar_20_50, pine_bg_20_50,
        ));
        csv_content.push_str(&format!(
            "{},{},", pine_bar_100_200, pine_bg_100_200,
        ));
        csv_content.push_str(&format!(
            "{},{},", pine_bar_12_26, pine_bg_12_26,
        ));
        csv_content.push_str(&format!(
            "{},{},{},", rt_tr_5d_avg, rt_tr_20d_avg, ema_compare,
        ));
        csv_content.push_str(&format!(
            "{},{},", rt_tr_ratio_5d_20d, rt_tr_ratio_20d_60d,
        ));
        csv_content.push_str(&format!(
            "{},{},{},", indicators.pos_norm_20, indicators.ma5_in_20d_ma5_pos, indicators.ma20_in_60d_ma20_pos,
        ));
        csv_content.push_str(&format!(
            "{},{},", ma5_in_all_ma5_pos, ma20_in_all_ma20_pos,
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},", vel_pct, acceleration, power, power_pct,
        ));
        csv_content.push_str(&format!("{}\n", open_time));

        tick_index += 1;

        // 每100条打印进度
        if tick_index % 100 == 0 {
            println!("已处理 {} / {} 条K线", tick_index, klines.len());
        }
    }

    // 写入文件
    std::fs::write(&output_path, csv_content)?;
    println!("\n============================================");
    println!("CSV文件已生成: {}", output_path);
    println!("共 {} 条记录", tick_index);
    println!("============================================");

    Ok(())
}

fn parse_decimal(s: &str) -> Decimal {
    s.parse().unwrap_or(dec!(0))
}
