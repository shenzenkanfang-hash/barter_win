//! 测试网模拟运行模式
//!
//! 使用币安测试网数据进行实时模拟运行：
//! - 测试网行情数据 (WebSocket)
//! - 测试网账户数据 (API)
//! - StateManager 状态管理
//! - UnifiedStateView 统一状态视图
//!
//! 运行: cargo run -p h_sandbox --example testnet_simulation

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use parking_lot::RwLock;
use tokio::time::interval;

use a_common::api::BinanceApiGateway;
use b_data_source::Paths;
use b_data_source::ws::{Kline1mStream, DepthStream};
use e_risk_monitor::shared::account_pool::AccountPool;
use e_risk_monitor::position::position_manager::LocalPositionManager;
use e_risk_monitor::UnifiedStateView;

/// 测试网模拟系统
struct TestnetSimulation {
    /// 账户池 (实现 StateManager)
    account_pool: Arc<AccountPool>,
    /// 持仓管理器 (实现 StateManager)
    position_manager: Arc<LocalPositionManager>,
    /// 统一状态视图
    unified_view: Arc<UnifiedStateView>,
    /// 测试网 API 网关
    gateway: BinanceApiGateway,
    /// 当前价格
    current_price: RwLock<Decimal>,
    /// BTC/USDT K线流
    kline_stream: Option<Kline1mStream>,
    /// BTC/USDT 订单簿流
    depth_stream: Option<DepthStream>,
}

impl TestnetSimulation {
    /// 创建测试网模拟系统
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║          barter-rs 测试网模拟运行模式                       ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");

        // 1. 初始化测试网配置
        println!("【1】初始化测试网配置");
        let paths = Paths::new();
        println!("  平台: {:?}", paths.platform());
        println!("  内存备份: {}", paths.memory_backup_dir);

        // 2. 创建测试网 API 网关 (实盘行情 + 测试网账户)
        println!("\n【2】创建测试网 API 网关");
        let gateway = BinanceApiGateway::new_futures_with_testnet();
        println!("  市场API: https://fapi.binance.com (实盘行情)");
        println!("  账户API: https://testnet.binancefuture.com (测试网账户)");

        // 3. 初始化账户池 (默认 100,000 USDT 测试资金)
        println!("\n【3】初始化账户池 (100,000 USDT)");
        let account_pool = Arc::new(AccountPool::new());
        let init_balance = account_pool.total_equity();
        println!("  初始权益: {} USDT", init_balance);

        // 4. 初始化持仓管理器
        println!("\n【4】初始化持仓管理器");
        let position_manager = Arc::new(LocalPositionManager::new());
        println!("  LocalPositionManager 已创建");

        // 5. 创建统一状态视图
        println!("\n【5】创建统一状态视图 (UnifiedStateView)");
        let unified_view = Arc::new(UnifiedStateView::new(
            position_manager.clone() as Arc<dyn e_risk_monitor::StateManager>,
            account_pool.clone() as Arc<dyn e_risk_monitor::StateManager>,
        ));
        println!("  UnifiedStateView 已创建");

        // 6. 连接测试网 K线 WebSocket (仅 BTC)
        println!("\n【6】连接测试网 WebSocket");
        println!("  正在连接 BTC/USDT 1分钟K线...");
        let kline_stream = match Kline1mStream::new(vec!["btcusdt".to_string()]).await {
            Ok(stream) => {
                println!("  ✅ K线流连接成功");
                Some(stream)
            }
            Err(e) => {
                println!("  ⚠️ K线流连接失败: {:?}", e);
                None
            }
        };

        // 7. 连接测试网订单簿 WebSocket
        println!("  正在连接 BTC/USDT 订单簿...");
        let depth_stream = match DepthStream::new_btc_only().await {
            Ok(stream) => {
                println!("  ✅ 订单簿流连接成功");
                Some(stream)
            }
            Err(e) => {
                println!("  ⚠️ 订单簿流连接失败: {:?}", e);
                None
            }
        };

        println!("\n═══════════════════════════════════════════════════════════════");
        println!("                    测试网连接完成                                ");
        println!("═══════════════════════════════════════════════════════════════\n");

        Ok(Self {
            account_pool,
            position_manager,
            unified_view,
            gateway,
            current_price: RwLock::new(dec!(0)),
            kline_stream,
            depth_stream,
        })
    }

    /// 获取测试网账户信息
    async fn fetch_testnet_account(&self) {
        match self.gateway.fetch_futures_account().await {
            Ok(account) => {
                println!("  [账户更新] 权益: {} | 可用: {} | 未实现: {}",
                    account.total_margin_balance, account.available_balance, account.total_unrealized_profit);
            }
            Err(e) => {
                println!("  [账户更新失败] {:?}", e);
            }
        }
    }

    /// 获取测试网持仓信息
    async fn fetch_testnet_positions(&self) {
        match self.gateway.fetch_futures_positions().await {
            Ok(positions) => {
                println!("  [持仓更新] {} 个持仓", positions.len());
                for pos in positions.iter().take(3) {
                    println!("    {} {} 数量:{} 杠杆:{}x",
                        pos.symbol, pos.position_side, pos.position_amt, pos.leverage);
                }
            }
            Err(e) => {
                println!("  [持仓更新失败] {:?}", e);
            }
        }
    }

    /// 打印系统快照
    fn print_snapshot(&self) {
        let snapshot = self.unified_view.snapshot();
        let price = *self.current_price.read();

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│                   测试网系统快照                             │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  时间: {}                                  │", Utc::now().format("%Y-%m-%d %H:%M:%S"));
        println!("├─────────────────────────────────────────────────────────────┤");

        // 账户信息
        if let Some(account) = &snapshot.account {
            println!("│  【账户】                                                  │");
            println!("│    权益: {} USDT                              │", account.equity);
            println!("│    可用: {} USDT                              │", account.available);
            println!("│    未实现: {} USDT                            │", account.unrealized_pnl);
        } else {
            println!("│  【账户】 无数据                                           │");
        }

        // 持仓信息
        if snapshot.positions.is_empty() {
            println!("│  【持仓】 无持仓                                            │");
        } else {
            println!("│  【持仓】 {} 个symbol                                     │", snapshot.positions.len());
            for pos in snapshot.positions.iter().take(3) {
                println!("│    {} Long:{} 空:{}                         │",
                    pos.symbol, pos.long_qty, pos.short_qty);
            }
        }

        // 价格信息
        if price > dec!(0) {
            println!("│  【行情】 BTC/USDT: {} USDT                    │", price);
        } else {
            println!("│  【行情】 等待价格数据...                                  │");
        }

        println!("└─────────────────────────────────────────────────────────────┘");
    }

    /// 运行主循环
    async fn run(&mut self) {
        println!("开始测试网模拟主循环...\n");

        let mut loop_count = 0u64;
        let mut last_account_fetch = std::time::Instant::now();
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            loop_count += 1;

            tokio::select! {
                // K线消息
                msg = async {
                    if let Some(ref mut stream) = self.kline_stream {
                        stream.next_message().await
                    } else {
                        None
                    }
                } => {
                    if let Some(msg) = msg {
                        // 解析价格 - 简化处理
                        if let Ok(price_f) = msg.parse::<f64>() {
                            if price_f > 0.0 {
                                *self.current_price.write() = Decimal::from_f64_retain(price_f).unwrap_or(dec!(0));
                            }
                        }
                    }
                }
                // 订单簿消息
                _ = async {
                    if let Some(ref mut stream) = self.depth_stream {
                        stream.next_message().await;
                    }
                } => {
                    // 订单簿数据可用于风控检查
                }
                // 主循环定时器
                _ = ticker.tick() => {
                    // 每秒打印状态快照
                    println!("\n--- Tick #{} ---", loop_count);

                    // 获取测试网账户数据 (每10秒一次)
                    if last_account_fetch.elapsed() >= Duration::from_secs(10) {
                        self.fetch_testnet_account().await;
                        self.fetch_testnet_positions().await;
                        last_account_fetch = std::time::Instant::now();
                    }

                    // 打印状态快照
                    self.print_snapshot();
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建并运行测试网模拟系统
    match TestnetSimulation::new().await {
        Ok(mut simulation) => {
            simulation.run().await;
        }
        Err(e) => {
            eprintln!("测试网模拟系统初始化失败: {:?}", e);
            return Err(e);
        }
    }

    Ok(())
}
