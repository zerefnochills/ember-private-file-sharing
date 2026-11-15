//! Ember relay server executable.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use ember_server::{app, AppState, FilesystemStorage};
use tokio::net::TcpListener;

#[derive(Parser, Debug)]
#[command(
    name = "ember-server",
    version,
    about = "Ember: ephemeral ciphertext relay server"
)]
struct ServerArgs {
    /// Host address to bind to
    #[arg(short, long, default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Directory for storing uploaded ciphertext parts
    #[arg(short, long, default_value = "data/blobs")]
    storage_dir: PathBuf,

    /// Sweeper interval in seconds
    #[arg(long, default_value_t = 30)]
    sweep_interval_secs: u64,

    /// Proof-of-work difficulty (leading zero bits)
    #[arg(long, default_value_t = 16)]
    pow_difficulty: u32,

    /// Max blob creation requests per 15-minute window per IP
    #[arg(long, default_value_t = 60)]
    rate_limit: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = ServerArgs::parse();

    let storage = Arc::new(FilesystemStorage::new(&args.storage_dir).await?);
    let rate_limit_config = ember_server::RateLimitConfig {
        max_requests: args.rate_limit,
        window_secs: 900,
    };
    let state = AppState::with_pow_and_rate_limit(storage, args.pow_difficulty, rate_limit_config);

    // Spawn background TTL and inactive upload sweeper
    ember_server::sweeper::spawn_sweeper(
        state.clone(),
        Duration::from_secs(args.sweep_interval_secs),
    );

    let app = app(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    println!("ember-server running on http://{addr}");
    println!(
        "Storage backend: filesystem at {}",
        args.storage_dir.display()
    );

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
