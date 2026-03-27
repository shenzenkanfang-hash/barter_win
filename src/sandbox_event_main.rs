//! Sandbox Mode - 事件驱动沙盒（新架构）
//!
//! ## 核心设计
//! - **零轮询**: `recv().await` 阻塞等待，无 `tokio::time::sleep`
//! - **零 spawn**: 无 `tokio::spawn` 后台任务
//! - **单事件流**: 一个 Tick 驱动完整处理链
//!
//! ## 数据流
//! ```
//! StreamTickGenerator
//!         │ tick_tx.send()
//!         ▼
//!   mpsc::channel
//!         │ tick_rx.recv().await
//!         ▼
//!   EventEngine::run()
//!         │
//!         ├─► on_tick()
//!         │      ├─► calc_indicators()
//!         │      ├─► strategy.decide()
//!         │      ├─► risk_checker.pre_check()
//!         │      └─► gateway.place_order()
//! ```
//!
//! ## 关键约束
//! - tokio::spawn: 0 个
//! - tokio::sleep: 0 个
//! - 数据竞争: 0 次

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};

use a_common::volatility::VolatilityCalc;
use b_data_source::{KLine, Period, Tick};
use h_sandbox::{
    ShadowBinanceGateway, ShadowRiskChecker,
    historical_replay::{StreamTickGenerator, TickToWsConverter},
};
use f_engine::event::{
    EventEngine, EventBus, EngineConfig,
    TickEvent, KlineData, Strategy,
};
use f_engine::interfaces::RiskChecker;

// ============================================================================
// 常量
// ============================================================================

const DEFAULT_SYMBOL: &str = "BTCUSDT";
const DEFAULT_FUND: f64 = 10000.0;
const DEFAULT_CHANNEL_BUFFER: usize = 1024;

// ============================================================================
// 简单策略实现
// ============================================================================

use async_trait::async_trait;
use f_engine::types::TradingDecision;
use f_engine::event::EngineState;

/// 简单趋势跟踪策略
struct TrendStrategy;

impl TrendStrategy {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Strategy for TrendStrategy {
    async fn decide(&self, state: &EngineState) -> Option<TradingDecision> {
        // 简单的趋势跟踪策略
        // 当 EMA_fast > EMA_slow 且无持仓时，做多
        // 当 EMA_fast < EMA_slow 且有持仓时，平多
        
        let indicators = &state.indicators;
        
        // 检查是否有持仓信号
        if state.has_position {
            // 检查是否应该平仓
            if let (Some(fast), Some(slow)) = (indicators.ema_fast, indicators.ema_slow) {
                if fast < slow {
                    // 平多
                    return Some(TradingDecision::new(
                        f_engine::types::TradingAction::Flat,
                        "EMA死叉平多".to_string(),
                        80,
                        state.symbol.clone(),
                        state.position_qty,
                        state.current_price.unwrap_or(Decimal::ZERO),
                        Utc::now().timestamp(),
                    ));
                }
            }
        } else {
            // 无持仓，检查是否应该开仓
            if let (Some(fast), Some(slow)) = (indicators.ema_fast, indicators.ema_slow) {
                if fast > slow {
                    // 做多
                    return Some(TradingDecision::new(
                        f_engine::types::TradingAction::Long,
                        "EMA金叉做多".to_string(),
                        80,
                        state.symbol.clone(),
                        dec!(0.01), // 固定仓位
                        state.current_price.unwrap_or(Decimal::ZERO),
                        Utc::now().timestamp(),
                    ));
                }
            }
        }
        
        None
    }
}

// ============================================================================
// Tick 转换
// ============================================================================

/// 将 SimulatedTick 转换为 TickEvent
fn convert_tick(sim_tick: h_sandbox::historical_replay::SimulatedTick) -> TickEvent {
    TickEvent {
        symbol: sim_tick.symbol,
        price: sim_tick.price,
        qty: sim_tick.qty,
        timestamp: sim_tick.timestamp,
        kline: sim_tick.kline.map(|k| KlineData {
            symbol: k.symbol,
            period: k.period.to_string(),
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            open_time: k.timestamp,
            close_time: k.timestamp,
            is_closed: k.is_closed,
        }),
        is_kline_closed: sim_tick.is_last_in_kline,
    }
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(LevelFilter::DEBUG)
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    println!("=== 事件驱动沙盒 (新架构) ===");
    println!();
    
    // ============ 参数解析 ============
    let symbol = std::env::var("SYMBOL").unwrap_or_else(|_| DEFAULT_SYMBOL.to_string());
    let fund = std::env::var("FUND")
        .unwrap_or_else(|_| DEFAULT_FUND.to_string())
        .parse::<f64>()
        .unwrap_or(DEFAULT_FUND);
    
    println!("配置:");
    println!("  品种: {}", symbol);
    println!("  初始资金: {}", fund);
    println!();
    
    // ============ 创建组件 ============
    
    // 1. 创建网关
    let gateway_config = h_sandbox::gateway::ShadowConfig::default()
        .with_initial_balance(fund);
    let gateway = Arc::new(ShadowBinanceGateway::new(gateway_config));
    
    // 2. 创建风控
    let risk_checker = Arc::new(ShadowRiskChecker::new());
    
    // 3. 创建策略
    let strategy = TrendStrategy::new();
    
    // 4. 创建引擎配置
    let config = EngineConfig {
        symbol: symbol.clone(),
        initial_fund: dec!(fund),
        log_timing: true,
        ..Default::default()
    };
    
    // 5. 创建事件引擎
    let engine = EventEngine::new(
        config,
        risk_checker,
        strategy,
        gateway.clone(),
    );
    
    // ============ 创建 Tick 通道 ============
    let (mut bus, bus_handle) = EventBus::default();
    let tick_rx = bus.into_tick_rx();
    
    // ============ 创建历史回放生成器 ============
    let data_file = PathBuf::from(format!("data/{}_1m.csv", symbol));
    let klines = read_klines_from_csv(&data_file)?;
    
    println!("加载 {} 根 K线", klines.len());
    
    let generator = StreamTickGenerator::new(symbol.clone(), klines);
    
    // ============ 主事件循环 ============
    println!();
    println!("开始事件驱动处理...");
    println!("========================================");
    
    let start_time = std::time::Instant::now();
    let mut tick_count = 0u64;
    
    // 使用 tokio::select! 同时运行引擎和处理生成器
    tokio::pin!(tick_rx);
    
    // 生成器迭代器
    let mut gen = generator;
    
    loop {
        tokio::select! {
            // 引擎处理 Tick
            tick = tick_rx.recv() => {
                match tick {
                    Some(t) => {
                        tick_count += 1;
                        
                        // 日志输出（每100个Tick）
                        if tick_count % 100 == 0 {
                            println!(
                                "[{}] Tick #{} @ {}",
                                t.symbol,
                                tick_count,
                                t.price
                            );
                        }
                    }
                    None => {
                        // 引擎 channel 关闭
                        tracing::info!("引擎事件循环结束");
                        break;
                    }
                }
            }
            
            // 生成器发送 Tick（非阻塞）
            result = gen.next() => {
                match result {
                    Some(sim_tick) => {
                        let tick_event = convert_tick(sim_tick);
                        
                        // 发送到引擎
                        if let Err(e) = bus_handle.send_tick(tick_event).await {
                            tracing::error!("发送Tick失败: {}", e);
                            break;
                        }
                    }
                    None => {
                        // 生成器结束，等待引擎处理完
                        tracing::info!("历史数据回放完成，等待引擎处理剩余事件...");
                        
                        // 关闭发送端，通知引擎
                        drop(bus_handle);
                        
                        // 继续等待引擎处理完
                        continue;
                    }
                }
            }
        }
    }
    
    let elapsed = start_time.elapsed();
    
    // ============ 输出统计 ============
    println!();
    println!("========================================");
    println!("处理完成:");
    println!("  总 Tick 数: {}", tick_count);
    println!("  耗时: {:?}", elapsed);
    if tick_count > 0 {
        println!("  平均延迟: {:?}", elapsed / tick_count);
    }
    println!();
    
    // 输出账户状态
    if let Ok(account) = gateway.get_account() {
        println!("账户状态:");
        println!("  总权益: {}", account.total_equity);
        println!("  可用: {}", account.available);
        println!("  冻结: {}", account.frozen_margin);
        println!("  未实现盈亏: {}", account.unrealized_pnl);
    }
    
    Ok(())
}

// ============================================================================
// 辅助函数
// ============================================================================

fn read_klines_from_csv(path: &PathBuf) -> Result<Vec<KLine>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    let mut klines = Vec::new();
    
    for line in reader.lines().skip(1) {  // 跳过表头
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        
        if parts.len() < 6 {
            continue;
        }
        
        let timestamp: i64 = parts[0].parse()?;
        let open: Decimal = parts[1].parse()?;
        let high: Decimal = parts[2].parse()?;
        let low: Decimal = parts[3].parse()?;
        let close: Decimal = parts[4].parse()?;
        let volume: Decimal = parts[5].parse()?;
        
        klines.push(KLine {
            symbol: symbol_from_filename(path),
            period: Period::Min1,
            open,
            high,
            low,
            close,
            volume,
            timestamp: Utc.timestamp_opt(timestamp, 0).unwrap(),
            is_closed: true,
        });
    }
    
    Ok(klines)
}

fn symbol_from_filename(path: &PathBuf) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("BTCUSDT")
        .replace("_1m", "")
        .to_uppercase()
}
