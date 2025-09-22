use anyhow::Result;
use log::{info, warn, error};
use clap::{Parser, Subcommand};

mod bbs;
mod meshtastic;
mod config;
mod storage;
mod protobuf; // for meshtastic-proto feature generated code

use crate::bbs::BbsServer;
use crate::config::Config;

#[derive(Parser)]
#[command(name = "meshbbs")]
#[command(about = "A Bulletin Board System for Meshtastic mesh networks")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Configuration file path (can be used before or after subcommand)
    #[arg(short, long, default_value = "config.toml", global = true)]
    config: String,
    
    /// Verbose logging (-v, -vv for more; may appear before or after subcommand)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the BBS server
    Start {
        /// Meshtastic device port (e.g., /dev/ttyUSB0)
        #[arg(short, long)]
        port: Option<String>,
    },
    /// Initialize a new BBS configuration
    Init,
    /// Show BBS status and statistics
    Status,
    /// Run a serial smoke test: collect node & channel info
    SmokeTest {
        /// Device serial port
        #[arg(short, long)]
        port: String,
        /// Baud rate
        #[arg(short = 'b', long, default_value_t = 115200)]
        baud: u32,
        /// Seconds to wait before giving up
        #[arg(short, long, default_value_t = 10)]
        timeout: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging based on verbosity
    let log_level = match cli.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .init();

    info!("Starting MeshBBS v{}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Commands::Start { port } => {
            let config = Config::load(&cli.config).await?;
            let mut bbs = BbsServer::new(config).await?;
            
            if let Some(port_path) = port {
                info!("Connecting to Meshtastic device on {}", port_path);
                bbs.connect_device(&port_path).await?;
            }
            
            info!("BBS server starting...");
            bbs.run().await?;
        }
        Commands::Init => {
            info!("Initializing new BBS configuration");
            Config::create_default(&cli.config).await?;
            info!("Configuration file created at {}", cli.config);
        }
        Commands::Status => {
            let config = Config::load(&cli.config).await?;
            let bbs = BbsServer::new(config).await?;
            bbs.show_status().await?;
        }
    Commands::SmokeTest { port, baud, timeout } => {
            #[cfg(not(all(feature = "serial", feature = "meshtastic-proto")))]
            {
                error!("SmokeTest requires 'serial' and 'meshtastic-proto' features");
                std::process::exit(2);
            }
            #[cfg(all(feature = "serial", feature = "meshtastic-proto"))]
            {
                use tokio::time::{Instant, Duration, sleep};
                use crate::meshtastic::MeshtasticDevice;
                let mut device = MeshtasticDevice::new(&port, baud).await?;
                info!("Starting smoke test on {} @ {} baud", port, baud);
                let mut last_hb = Instant::now();
                let start = Instant::now();
                let deadline = start + Duration::from_secs(timeout);
                // initial want_config request generation handled in ensure_want_config
                while Instant::now() < deadline {
                    // Periodic heartbeat
                    if last_hb.elapsed() >= Duration::from_secs(3) {
                        let _ = device.send_heartbeat();
                        let _ = device.ensure_want_config();
                        last_hb = Instant::now();
                    } else {
                        // still make sure initial want_config was sent promptly
                        let _ = device.ensure_want_config();
                    }
                    if let Some(_summary) = device.receive_message().await? {
                        if device.initial_sync_complete() { break; }
                    } else {
                        sleep(Duration::from_millis(40)).await;
                    }
                }
                #[cfg(feature = "meshtastic-proto")]
                {
                    let status_ok = device.initial_sync_complete();
                    if !status_ok && !device.binary_detected() {
                        warn!("No binary protobuf frames detected. Device likely not in PROTO serial mode (still in text console). Enable with: meshtastic --set serial.enabled true --set serial.mode PROTO");
                    }
                    let payload = serde_json::json!({
                        "status": if status_ok { "ok" } else { "incomplete" },
                        "config_complete": device.is_config_complete(),
                        "have_myinfo": device.have_my_info(),
                        "have_radio_config": device.have_radio_config(),
                        "have_module_config": device.have_module_config(),
                        "node_count": device.node_count(),
                        "binary_detected": device.binary_detected(),
                        "config_request_id": device.config_request_id_hex(),
                        "timeout_seconds": timeout,
                    });
                    println!("{}", payload.to_string());
                    std::process::exit(if status_ok { 0 } else { 1 });
                }
            }
        }
    }

    Ok(())
}