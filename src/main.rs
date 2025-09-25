use anyhow::Result;
use log::{info, warn};
use clap::{Parser, Subcommand};

// Use the published library crate modules instead of redefining them here.
use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;

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
    /// Set or update the sysop (primary administrator) password in the config file
    SysopPasswd,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Load config early to configure logging (except for Init which writes default later)
    let pre_config = match cli.command { Commands::Init => None, _ => Config::load(&cli.config).await.ok() };
    init_logging(&pre_config, cli.verbose);

    info!("Starting Meshbbs v{}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Commands::Start { port } => {
            let config = pre_config.unwrap_or(Config::load(&cli.config).await?);
            // Capture configured port before moving config into server
            let configured_port = config.meshtastic.port.clone();
            let mut bbs = BbsServer::new(config).await?;

            // Determine which port to use: CLI overrides config; fallback to config when CLI absent
            let chosen_port = match port {
                Some(cli_port) => Some(cli_port),
                None => if !configured_port.is_empty() { Some(configured_port) } else { None },
            };

            if let Some(port_path) = chosen_port {
                match bbs.connect_device(&port_path).await {
                    Ok(_) => info!("Connected to Meshtastic device on {}", port_path),
                    Err(e) => {
                        // Fail fast? For now we warn and continue so the BBS can still run (e.g., for web or offline ops)
                        warn!("Failed to connect to device on {}: {} (BBS continuing without device)", port_path, e);
                    }
                }
            } else {
                info!("No --port specified and no configured device port set; starting without device.");
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
            let config = pre_config.unwrap_or(Config::load(&cli.config).await?);
            let bbs = BbsServer::new(config).await?;
            bbs.show_status().await?;
        }
        Commands::SysopPasswd => {
            use password_hash::{PasswordHasher, SaltString};
            use argon2::Argon2;
            // Read existing config
            let mut config = pre_config.unwrap_or(Config::load(&cli.config).await?);
            println!("Setting sysop password for '{}'.", config.bbs.sysop);
            // Prompt twice without echo
            let pass1 = rpassword::prompt_password("New password: ")?;
            if pass1.len() < 8 { println!("Error: password too short (min 8)." ); return Ok(()); }
            if pass1.len() > 128 { println!("Error: password too long." ); return Ok(()); }
            let pass2 = rpassword::prompt_password("Confirm password: ")?;
            if pass1 != pass2 { println!("Error: passwords do not match." ); return Ok(()); }
            // Hash
            let salt = SaltString::generate(&mut rand::thread_rng());
            let argon = Argon2::default();
            let hash = match argon.hash_password(pass1.as_bytes(), &salt) { Ok(h) => h.to_string(), Err(e) => { println!("Hash error: {e}" ); return Ok(()); } };
            config.bbs.sysop_password_hash = Some(hash);
            // Persist updated config (overwrite file)
            let serialized = toml::to_string_pretty(&config)?;
            tokio::fs::write(&cli.config, serialized).await?;
            println!("Sysop password updated successfully.");
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
                use meshbbs::meshtastic::MeshtasticDevice;
                let mut device = MeshtasticDevice::new(&port, baud).await?;
                info!("Starting smoke test on {} @ {} baud", port, baud);
                let mut last_hb = Instant::now();
                let start = Instant::now();
                let deadline = start + Duration::from_secs(timeout);
                // Send initial want_config request once, then periodic heartbeats with retries
                let _ = device.ensure_want_config();
                while Instant::now() < deadline {
                    // Periodic heartbeat and config retry every 10 seconds (less aggressive)
                    if last_hb.elapsed() >= Duration::from_secs(10) {
                        let _ = device.send_heartbeat();
                        let _ = device.ensure_want_config();
                        last_hb = Instant::now();
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
                    println!("{}", payload);
                    std::process::exit(if status_ok { 0 } else { 1 });
                }
            }
        }
    }

    Ok(())
}

fn init_logging(config: &Option<Config>, verbosity: u8) {
    use std::io::Write;
    let mut builder = env_logger::Builder::new();
    // Base level from CLI verbosity overrides config
    let base_level = match verbosity { 0 => log::LevelFilter::Info, 1 => log::LevelFilter::Debug, _ => log::LevelFilter::Trace };
    builder.filter_level(base_level);
    if let Some(cfg) = config {
        let security_path = cfg.logging.security_file.clone();
        if let Some(ref file) = cfg.logging.file {
            if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
                let mutex = std::sync::Arc::new(std::sync::Mutex::new(f));
                let write_mutex = mutex.clone();
                builder.format(move |fmt, record| {
                    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
                    let line = format!("{} [{}] {}", ts, record.level(), record.args());
                    if let Ok(mut guard) = write_mutex.lock() { let _ = writeln!(guard, "{}", line); }
                    if record.target() == "security" {
                        if let Some(ref sec_path) = security_path {
                            if let Ok(mut sf) = std::fs::OpenOptions::new().create(true).append(true).open(sec_path) {
                                let _ = writeln!(sf, "{}", line);
                            }
                        }
                    }
                    writeln!(fmt, "{}", line)
                });
            } else {
                builder.format(|fmt, record| {
                    writeln!(fmt, "{} [{}] {}", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"), record.level(), record.args())
                });
            }
        } else {
            builder.format(|fmt, record| {
                let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
                writeln!(fmt, "{} [{}] {}", ts, record.level(), record.args())
            });
        }
    } else {
        builder.format(|fmt, record| {
            writeln!(fmt, "{} [{}] {}", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"), record.level(), record.args())
        });
    }
    let _ = builder.try_init();
}