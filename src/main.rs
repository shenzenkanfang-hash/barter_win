//! Trading System v5.5 - 纯启动引导

mod components;
mod pipeline;
mod tick_context;
mod utils;

use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::components::{create_components, init_heartbeat, print_heartbeat_report};
use crate::pipeline::run_pipeline;
use crate::tick_context::{DATA_FILE, SYMBOL};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("=== Trading System v5.5 | {} | {} ===", SYMBOL, DATA_FILE);
    init_heartbeat();
    let components = create_components().await?;

    tracing::info!("Components: [b]KlineStream [f]MockGateway [d]Trader [c]SignalProc [e]RiskChecker");
    run_pipeline(components).await?;
    print_heartbeat_report().await;

    Ok(())
}
