//! Headless binary entrypoint for `nexus-daemon`.

use nexus_daemon::DaemonActor;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!("Initializing Nexus Suite Headless Daemon...");

    let (actor, _cmd_tx, _event_rx) = DaemonActor::new();
    
    // Spawn actor loop
    tokio::spawn(async move {
        if let Err(e) = actor.run().await {
            eprintln!("Daemon actor runtime error: {e}");
        }
    });

    info!("Nexus Daemon running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    info!("Shutting down Nexus Daemon.");

    Ok(())
}
