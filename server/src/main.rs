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
    /// Host address to bind to (no short flag: `-h` is reserved for help)
    #[arg(long, default_value = "127.0.0.1")]
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

    /// IP address of a reverse proxy whose X-Forwarded-For / X-Real-IP headers are trusted
    /// (repeat the flag for several). Without it the TCP peer address is always used.
    #[arg(long = "trusted-proxy")]
    trusted_proxies: Vec<std::net::IpAddr>,

    /// Max blob creation requests per 15-minute window per IP
    #[arg(long, default_value_t = 60)]
    rate_limit: u32,

    /// Path to persistent access log file
    #[arg(long, default_value = "data/access.log")]
    access_log_file: Option<PathBuf>,

    /// Access log retention in days (older entries are purged by sweeper)
    #[arg(long, default_value_t = 30)]
    access_log_retention_days: u32,

    /// Hex-encoded X25519 public key of the reviewer (enables sealed-box encryption of reports at rest)
    #[arg(long)]
    reviewer_public_key: Option<String>,

    /// Optional separate socket address to listen on for admin endpoints (e.g. 127.0.0.1:9090)
    /// If omitted, admin endpoints are served on the main listener.
    #[arg(long)]
    admin_listen: Option<SocketAddr>,
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

    let access_logger = match args.access_log_file {
        Some(path) => {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            ember_server::AccessLogRecorder::with_file(path, args.access_log_retention_days)
        }
        None => ember_server::AccessLogRecorder::new(),
    };

    let reviewer_pk = match args.reviewer_public_key {
        Some(ref hex) => Some(ember_core::parse_hex_key(hex)?),
        None => None,
    };

    let state = AppState::with_pow_and_rate_limit(storage, args.pow_difficulty, rate_limit_config)
        .with_trusted_proxies(args.trusted_proxies.clone())
        .with_access_logger(access_logger)
        .with_reviewer_public_key(reviewer_pk);

    // Spawn background TTL and inactive upload sweeper
    ember_server::sweeper::spawn_sweeper(
        state.clone(),
        Duration::from_secs(args.sweep_interval_secs),
    );

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    println!("ember-server running on http://{addr}");
    println!(
        "Storage backend: filesystem at {}",
        args.storage_dir.display()
    );

    if let Some(admin_addr) = args.admin_listen {
        println!("ember-server admin running on http://{admin_addr} (isolated admin interface)");
        let public_listener = TcpListener::bind(addr).await?;
        let admin_listener = TcpListener::bind(admin_addr).await?;

        let public_app = ember_server::public_app(state.clone());
        let admin_app = ember_server::admin_app(state);

        let public_server = axum::serve(
            public_listener,
            public_app.into_make_service_with_connect_info::<SocketAddr>(),
        );
        let admin_server = axum::serve(
            admin_listener,
            admin_app.into_make_service_with_connect_info::<SocketAddr>(),
        );

        tokio::try_join!(public_server, admin_server)?;
    } else {
        let app = app(state);
        let listener = TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;
    }

    Ok(())
}
