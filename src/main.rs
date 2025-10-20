//! Binary entrypoint for the MeshBBS CLI.
//!
//! Commands:
//! - `start [--port <path>] [--daemon] [--pid-file <path>]` - run the BBS server with optional daemon mode
//! - `status` - print current status and a brief summary
//! - `check-device --port <path> [-b <baud>] [--timeout <s>]` - verify Meshtastic device connectivity
//! - `sysop-passwd` - interactively set the sysop password (argon2 hashed)
//!
//! ## Installation
//!
//! Use the provided `install.sh` script for production deployments. It will:
//! - Create all necessary directories
//! - Generate configuration with prompted passwords
//! - Set up systemd service
//! - Configure permissions
//!
//! ## Daemon Mode (Linux/macOS)
//!
//! Run meshbbs as a background service with `--daemon` flag. Features:
//! - Custom fork-based implementation (no external dependencies)
//! - PID file management
//! - TTY-aware logging (file-only in daemon, console+file in foreground)
//! - Cross-platform graceful shutdown (SIGTERM, SIGHUP, SIGINT)
//!
//! See the library crate docs for module‑level details: `meshbbs::`.
use anyhow::Result;
use clap::{Parser, Subcommand};
use log::{error, info, warn};

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

        /// Run as a background daemon (Unix only)
        #[arg(short, long)]
        daemon: bool,

        /// PID file location (for daemon mode)
        #[arg(long, default_value = "/tmp/meshbbs.pid")]
        pid_file: String,
    },
    /// Show BBS status and statistics
    Status,
    /// Check Meshtastic device connectivity and configuration over serial
    CheckDevice {
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
    /// Hash a password for use in config.toml (reads password from stdin)
    HashPassword,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config early to configure logging (except for Init which writes default later)
    // Also skip early logging for daemon mode Start command - it will init after forking
    let pre_config = match cli.command {
        Commands::Start { daemon, .. } if daemon => {
            // Daemon mode: load config but don't initialize logging yet
            Config::load(&cli.config).await.ok()
        }
        _ => Config::load(&cli.config).await.ok(),
    };

    // Initialize logging for non-daemon commands
    match &cli.command {
        Commands::Start { daemon, .. } if *daemon => {
            // Skip logging init - will happen after fork in child process
        }
        _ => {
            // All other commands: initialize logging normally
            init_logging(&pre_config, cli.verbose);
        }
    }

    match cli.command {
        Commands::Start {
            port,
            daemon,
            pid_file,
        } => {
            // Handle daemon mode FIRST - before loading config or doing anything else
            #[cfg(all(unix, feature = "daemon"))]
            if daemon {
                // Load minimal config just for daemonization
                let config = pre_config.unwrap_or(Config::load(&cli.config).await?);
                // Daemonize immediately - parent exits, child continues
                daemonize_process(&config, &pid_file)?;
                // Now we're in the child process - initialize logging
                init_logging(&Some(config.clone()), cli.verbose);
                info!("Starting Meshbbs v{}", env!("CARGO_PKG_VERSION"));

                // Continue with normal startup
                let configured_port = config.meshtastic.port.clone();
                let require_device = config.meshtastic.require_device_at_startup;
                let mut bbs = BbsServer::new(config).await?;

                // Determine which port to use
                let chosen_port = match port {
                    Some(cli_port) => Some(cli_port),
                    None => {
                        if !configured_port.is_empty() {
                            Some(configured_port)
                        } else {
                            None
                        }
                    }
                };

                if let Some(port_path) = chosen_port {
                    match bbs.connect_device(&port_path).await {
                        Ok(_) => info!("Connected to Meshtastic device on {}", port_path),
                        Err(e) => {
                            error!("Failed to connect to device on {}: {}", port_path, e);

                            // Check if device is required at startup
                            if require_device {
                                error!("Device connection required but failed - exiting");
                                error!("To allow BBS to start without a device, set:");
                                error!("  [meshtastic]");
                                error!("  require_device_at_startup = false");
                                std::process::exit(2);
                            } else {
                                warn!("BBS continuing without device connection");
                            }
                        }
                    }
                } else {
                    // No port specified
                    if require_device {
                        error!("Device connection required but no port specified");
                        error!("Either specify --port or set 'port' in config.toml");
                        error!("Or set require_device_at_startup = false to start without device");
                        std::process::exit(2);
                    } else {
                        info!("No --port specified and no configured device port set; starting without device.");
                    }
                }

                info!("BBS server starting...");
                bbs.run().await?;
                return Ok(());
            }

            #[cfg(not(all(unix, feature = "daemon")))]
            if daemon {
                let _ = pid_file; // Suppress unused warning
                eprintln!("Error: Daemon mode requires Unix platform and 'daemon' feature.");
                eprintln!("Compile with: cargo build --features daemon");
                std::process::exit(1);
            }

            // Non-daemon mode: normal startup
            let config = pre_config.unwrap_or(Config::load(&cli.config).await?);
            init_logging(&Some(config.clone()), cli.verbose);
            info!("Starting Meshbbs v{}", env!("CARGO_PKG_VERSION"));

            // Capture configured port and device requirement before moving config into server
            let configured_port = config.meshtastic.port.clone();
            let require_device = config.meshtastic.require_device_at_startup;
            let mut bbs = BbsServer::new(config).await?;

            // Determine which port to use: CLI overrides config; fallback to config when CLI absent
            let chosen_port = match port {
                Some(cli_port) => Some(cli_port),
                None => {
                    if !configured_port.is_empty() {
                        Some(configured_port)
                    } else {
                        None
                    }
                }
            };

            if let Some(port_path) = chosen_port {
                match bbs.connect_device(&port_path).await {
                    Ok(_) => info!("Connected to Meshtastic device on {}", port_path),
                    Err(e) => {
                        error!("Failed to connect to device on {}: {}", port_path, e);

                        // Check if device is required at startup
                        if require_device {
                            error!("Device connection required but failed - exiting");
                            error!("To allow BBS to start without a device, set:");
                            error!("  [meshtastic]");
                            error!("  require_device_at_startup = false");
                            std::process::exit(2);
                        } else {
                            warn!("BBS continuing without device connection");
                        }
                    }
                }
            } else {
                // No port specified
                if require_device {
                    error!("Device connection required but no port specified");
                    error!("Either specify --port or set 'port' in config.toml");
                    error!("Or set require_device_at_startup = false to start without device");
                    std::process::exit(2);
                } else {
                    info!("No --port specified and no configured device port set; starting without device.");
                }
            }

            info!("BBS server starting...");
            bbs.run().await?;
        }
        Commands::Status => {
            init_logging(&pre_config, cli.verbose);
            let config = pre_config.unwrap_or(Config::load(&cli.config).await?);
            let bbs = BbsServer::new(config).await?;
            bbs.show_status().await?;
        }
        Commands::SysopPasswd => {
            init_logging(&pre_config, cli.verbose);
            use argon2::Argon2;
            use password_hash::{PasswordHasher, SaltString};
            // Read existing config
            let mut config = pre_config.unwrap_or(Config::load(&cli.config).await?);
            println!("Setting sysop password for '{}'.", config.bbs.sysop);
            // Prompt twice without echo
            let pass1 = rpassword::prompt_password("New password: ")?;
            if pass1.len() < 8 {
                println!("Error: password too short (min 8).");
                return Ok(());
            }
            if pass1.len() > 128 {
                println!("Error: password too long.");
                return Ok(());
            }
            let pass2 = rpassword::prompt_password("Confirm password: ")?;
            if pass1 != pass2 {
                println!("Error: passwords do not match.");
                return Ok(());
            }
            // Hash
            let salt = SaltString::generate(&mut rand::thread_rng());
            let argon = Argon2::default();
            let hash = match argon.hash_password(pass1.as_bytes(), &salt) {
                Ok(h) => h.to_string(),
                Err(e) => {
                    println!("Hash error: {e}");
                    return Ok(());
                }
            };
            config.bbs.sysop_password_hash = Some(hash);
            // Persist updated config (overwrite file)
            let serialized = toml::to_string_pretty(&config)?;
            tokio::fs::write(&cli.config, serialized).await?;
            println!("Sysop password updated successfully.");
        }
        Commands::HashPassword => {
            // Read password from stdin (one line)
            use argon2::Argon2;
            use password_hash::{PasswordHasher, SaltString};
            use std::io::{self, BufRead};

            let stdin = io::stdin();
            let mut handle = stdin.lock();
            let mut password = String::new();
            handle.read_line(&mut password)?;

            // Remove trailing newline
            password = password.trim_end().to_string();

            if password.len() < 8 {
                eprintln!("Error: password too short (min 8 characters)");
                std::process::exit(1);
            }
            if password.len() > 128 {
                eprintln!("Error: password too long (max 128 characters)");
                std::process::exit(1);
            }

            // Hash the password
            let salt = SaltString::generate(&mut rand::thread_rng());
            let argon = Argon2::default();
            let hash = match argon.hash_password(password.as_bytes(), &salt) {
                Ok(h) => h.to_string(),
                Err(e) => {
                    eprintln!("Hash error: {e}");
                    std::process::exit(1);
                }
            };

            // Output only the hash (no extra text for easy parsing)
            println!("{}", hash);
        }
        Commands::CheckDevice {
            port,
            baud,
            timeout,
        } => {
            init_logging(&pre_config, cli.verbose);
            #[cfg(not(all(feature = "serial", feature = "meshtastic-proto")))]
            {
                error!("CheckDevice requires 'serial' and 'meshtastic-proto' features");
                std::process::exit(2);
            }
            #[cfg(all(feature = "serial", feature = "meshtastic-proto"))]
            {
                use meshbbs::meshtastic::MeshtasticDevice;
                use tokio::time::{sleep, Duration, Instant};
                let mut device = MeshtasticDevice::new(&port, baud).await?;
                info!("Checking device on {} @ {} baud", port, baud);
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
                        if device.initial_sync_complete() {
                            break;
                        }
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
    let base_level = match verbosity {
        0 => log::LevelFilter::Info,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    builder.filter_level(base_level);
    if let Some(cfg) = config {
        let security_path = cfg.logging.security_file.clone();
        if let Some(ref file) = cfg.logging.file {
            if let Ok(f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
            {
                let mutex = std::sync::Arc::new(std::sync::Mutex::new(f));
                let write_mutex = mutex.clone();

                // Check if stdout is a terminal (TTY) - if so, write to both file and console
                // In daemon mode, stdout is redirected so this will be false
                let is_tty = atty::is(atty::Stream::Stdout);

                builder.format(move |fmt, record| {
                    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
                    let line = format!("{} [{}] {}", ts, record.level(), record.args());

                    // Always write to log file
                    if let Ok(mut guard) = write_mutex.lock() {
                        let _ = writeln!(guard, "{}", line);
                    }

                    // Write to security log if needed
                    if record.target() == "security" {
                        if let Some(ref sec_path) = security_path {
                            if let Ok(mut sf) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(sec_path)
                            {
                                let _ = writeln!(sf, "{}", line);
                            }
                        }
                    }

                    // If stdout is a TTY (foreground mode), also write to console
                    if is_tty {
                        writeln!(fmt, "{}", line)
                    } else {
                        // Daemon mode: don't write to fmt to avoid duplicates
                        Ok(())
                    }
                });
            } else {
                builder.format(|fmt, record| {
                    writeln!(
                        fmt,
                        "{} [{}] {}",
                        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                        record.level(),
                        record.args()
                    )
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
            writeln!(
                fmt,
                "{} [{}] {}",
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                record.level(),
                record.args()
            )
        });
    }
    let _ = builder.try_init();
}

/// Daemonize the process (Unix only)
///
/// Forks the process, writes PID file, redirects I/O to log files,
/// and detaches from the controlling terminal.
#[cfg(all(unix, feature = "daemon"))]
fn daemonize_process(config: &Config, pid_file: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::process::Command;

    // Determine log file path
    let log_path = config.logging.file.as_deref().unwrap_or("meshbbs.log");

    // Spawn ourselves as a background process
    let current_exe = std::env::current_exe()?;
    let mut args: Vec<String> = std::env::args().collect();

    // Remove the --daemon flag to prevent infinite loop
    if let Some(pos) = args.iter().position(|arg| arg == "--daemon" || arg == "-d") {
        args.remove(pos);
    }

    // Skip the program name (args[0])
    let child_args = &args[1..];

    // Open log file for stdout/stderr
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    // Spawn child process
    let child = Command::new(&current_exe)
        .args(child_args)
        .stdin(std::process::Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()?;

    // Write PID file
    std::fs::write(pid_file, format!("{}", child.id()))?;

    // Parent process exits here - child continues as daemon
    std::process::exit(0);
}
