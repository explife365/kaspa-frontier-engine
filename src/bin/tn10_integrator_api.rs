//! Production integrator HTTP API for CEX custody rehearsal.
//!
//!   cargo run --release --bin tn10-integrator-api
//!
//! Env: INTEGRATOR_API_BIND, INTEGRATOR_API_KEYS, TN10_DEPOSIT_DATABASE, TN10_MIN_HEALTHY

use kaspa_frontier_engine::integrator_api::{load_config, serve};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let config = load_config()?;
    serve(config).await?;
    Ok(())
}
