//! 指标对比验证程序
//!
//! 从币安获取1000根日线数据，使用分钟级指标计算，输出CSV进行对比验证

use chrono::{DateTime, TimeZone, Utc};
use engine::SymbolRules;
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

/// Pine颜色中文映射
fn pine_color_to_cn(color: &str) -> String {
    match color {
        "PureGreen" => "纯绿".to_string(),
        "LightGreen" => "浅绿".to_string(),
        "PureRed" => "纯红".to_string(),
        "LightRed" => "浅红".to_string(),
        "Purple" => "紫色".to_string(),
        "Neutral" => "中性".to_string(),
        _ => color.to_string(),
    }
}

/// EMA比较中文
fn ema_compare_to_cn(compare: &str) -> &str {
    match compare {
        "above" => "上方",
        "below" => "下方",
        "equal" => "相等",
        _ => compare,
    }
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

    /// 获取最新TR值
    fn latest(&self) -> Decimal {
        self.history.back().copied().unwrap_or(dec!(0))
    }

    /// 获取TR数组引用
    fn history(&self) -> &VecDeque<Decimal> {
        &self.history
    }
}

/// TR排名百分位计算
fn tr_rank_percentile(history: &VecDeque<Decimal>, current: Decimal) -> Decimal {
    if history.is_empty() {
        return dec!(50);
    }
    let len = history.len();
    let rank = history.iter().filter(|&&x| x < current).count();
    Decimal::from(rank) / Decimal::from(len) * dec!(100)
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

    fn update(&mut self, tr: Decimal) -> (Decimal, Decimal, Decimal, Decimal, Decimal, Decimal) {
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
        let acceleration = if let Some(prev_vel) = self.velocity_history.back().copied() {
            if prev_vel > dec!(0) {
                (velocity - prev_vel) / prev_vel.abs()
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

        // Power = velocity * acceleration
        let power = velocity * acceleration;

        // Percentiles
        let vel_pct = self.percentile(&self.velocity_history, velocity);
        let acc_pct = self.percentile(&self.acc_history, acceleration);
        let power_pct = self.percentile(&self.velocity_history, power);

        (velocity, acceleration, power, vel_pct, acc_pct, power_pct)
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
    println!("从币安获取日线数据，计算指标输出CSV");
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

    // 创建交易规则
    let symbol_rules = SymbolRules::new(symbol.clone());
    let price_precision = symbol_rules.price_precision;
    let qty_precision = symbol_rules.quantity_precision;

    // 从币安API获取K线数据
    let url = format!(
        "https://api.binance.com/api/v3/klines?symbol={}&interval=1d&limit={}",
        symbol, limit
    );
    println!("请求URL: {}\n", url);

    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send()?;
    let text = response.text()?;
    let klines_raw: Vec<serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| format!("JSON解析错误: {}", e))?;

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

    // CSV表头 (中文)
    csv_content.push_str("时间戳,父级1m时间戳,Tick索引,开盘价,最高价,最低价,收盘价,成交量,");
    csv_content.push_str("TR比率5日全量,TR比率5日20日排名20日,TR比率5日20D,TR基准5日,");
    csv_content.push_str("TR比率20日全量,TR比率20日60日排名20日,TR比率20日60D,TR基准20日,");
    csv_content.push_str("可读时间,加加速度信号,Top3平均振幅百分比,");
    csv_content.push_str("松林颜色20_50柱,松林颜色20_50背景,");
    csv_content.push_str("松林颜色100_200柱,松林颜色100_200背景,");
    csv_content.push_str("松林颜色12_26柱,松林颜色12_26背景,");
    csv_content.push_str("实时TR5日,实时TR20日,EMA100对比EMA200,");
    csv_content.push_str("实时TR比率5日20日,实时TR比率20日60日,");
    csv_content.push_str("20日位置归一化,MA5在20日MA5位置,MA20在60日MA20位置,");
    csv_content.push_str("MA5在全部MA5位置,MA20在全部MA20位置,");
    csv_content.push_str("速度百分位日,加速度百分位日,功率,功率百分位60日,");
    csv_content.push_str("策略计算时间戳\n");

    let mut prev_close = None;
    let mut tick_index = 0i64;

    // TR历史用于排名计算
    let mut tr_5d_all_history: VecDeque<Decimal> = VecDeque::new();
    let mut tr_20d_all_history: VecDeque<Decimal> = VecDeque::new();

    for kline in &klines {
        let open_time = kline.open_time as i64;

        // 使用交易规则解析价格精度
        let open = round_price(parse_decimal(&kline.open), price_precision);
        let high = round_price(parse_decimal(&kline.high), price_precision);
        let low = round_price(parse_decimal(&kline.low), price_precision);
        let close = round_price(parse_decimal(&kline.close), price_precision);
        let volume = round_qty(parse_decimal(&kline.volume), qty_precision);

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

            // 更新TR历史
            let current_tr = RealTimeTR::calculate_tr(high, low, close, prev);
            tr_5d_all_history.push_back(current_tr);
            if tr_5d_all_history.len() > 100 {
                tr_5d_all_history.pop_front();
            }

            let tr_20d_val = (high - low) / prev;
            tr_20d_all_history.push_back(tr_20d_val);
            if tr_20d_all_history.len() > 100 {
                tr_20d_all_history.pop_front();
            }
        }
        prev_close = Some(close);

        // 计算TR排名
        let tr_5d_latest = real_time_tr_5d.latest();
        let tr_ratio_5d_all = tr_rank_percentile(&tr_5d_all_history, tr_5d_latest);

        // tr_ratio_5d_20d_rank_20d: 需要20日TR历史用于排名
        let tr_ratio_5d_20d_rank_20d = if real_time_tr_20d.history().len() >= 20 {
            tr_rank_percentile(real_time_tr_20d.history(), tr_5d_latest)
        } else {
            dec!(0)
        };

        // TR 20d 排名
        let tr_20d_latest = real_time_tr_20d.latest();
        let tr_ratio_20d_all = tr_rank_percentile(&tr_20d_all_history, tr_20d_latest);

        // tr_ratio_20d_60d_rank_20d: 需要60日TR历史，但我们只有20日
        let tr_ratio_20d_60d_rank_20d = dec!(0); // 简化

        // 计算实时TR比率
        let rt_tr_5d_avg = real_time_tr_5d.avg();
        let rt_tr_20d_avg = real_time_tr_20d.avg();
        let rt_tr_ratio_5d_20d = if rt_tr_20d_avg > dec!(0) {
            tr_5d_latest / rt_tr_20d_avg
        } else {
            dec!(0)
        };
        let rt_tr_ratio_20d_60d = if rt_tr_20d_avg > dec!(0) && big_cycle.tr_60d_avg() > dec!(0) {
            rt_tr_20d_avg / big_cycle.tr_60d_avg()
        } else {
            dec!(0)
        };

        // 速度百分位
        let vel_pct = vel_percentile.update(tr_20d_latest);

        // 功率计算
        let (_velocity, acceleration, power, _vel_pct, _acc_pct, power_pct) = power_calc.update(tr_20d_latest);

        // EMA比较
        let ema_100_val = ema_100.calculate(close);
        let ema_200_val = ema_200.calculate(close);
        let ema_compare = if ema_100_val > ema_200_val {
            "上方"
        } else if ema_100_val < ema_200_val {
            "下方"
        } else {
            "相等"
        };

        // RSI和PricePosition
        let _rsi_value = rsi.calculate(close);
        let _price_position = price_pos.calculate(close, high, low);

        // 获取大周期指标
        let indicators = big_cycle.calculate(high, low, close);

        // PineColor字符串 (中文)
        let pine_bar_20_50 = pine_color_to_cn(&format!("{:?}", indicators.pine_color_20_50));
        let pine_bg_20_50 = pine_color_to_cn(&format!("{:?}", indicators.pine_color_20_50));
        let pine_bar_100_200 = pine_color_to_cn(&format!("{:?}", indicators.pine_color_100_200));
        let pine_bg_100_200 = pine_color_to_cn(&format!("{:?}", indicators.pine_color_100_200));
        let pine_bar_12_26 = pine_color_to_cn(&format!("{:?}", indicators.pine_color_12_26));
        let pine_bg_12_26 = pine_color_to_cn(&format!("{:?}", indicators.pine_color_12_26));

        // jerk_signal 和 top3_avg_amplitude_pct
        let jerk_signal = acceleration;
        let top3_avg_amplitude_pct = dec!(0); // 简化

        // ma5_in_all_ma5_pos
        let ma5_in_all_ma5_pos = indicators.ma5_in_20d_ma5_pos;
        let ma20_in_all_ma20_pos = indicators.ma20_in_60d_ma20_pos;

        // 写入CSV行
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},{},",
            open_time, 0, tick_index, open, high, low, close, volume,
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},",
            tr_ratio_5d_all, tr_ratio_5d_20d_rank_20d, indicators.tr_ratio_5d_20d, big_cycle.tr_5d_avg(),
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},",
            tr_ratio_20d_all, tr_ratio_20d_60d_rank_20d, indicators.tr_ratio_20d_60d, big_cycle.tr_20d_avg(),
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
    println!("价格精度: {} 位小数", price_precision);
    println!("数量精度: {} 位小数", qty_precision);
    println!("============================================");

    Ok(())
}

fn parse_decimal(s: &str) -> Decimal {
    s.parse().unwrap_or(dec!(0))
}

/// 按精度四舍五入价格
fn round_price(price: Decimal, precision: u8) -> Decimal {
    price.round_dp(precision as u32)
}

/// 按精度四舍五入数量
fn round_qty(qty: Decimal, precision: u8) -> Decimal {
    qty.round_dp(precision as u32)
}
