use tracing_subscriber::fmt::format::FmtSpan;

fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .json()
        .init();

    tracing::info!("Trading system started");

    // Placeholder - runtime will be added in later phases
}
