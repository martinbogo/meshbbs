#[cfg(feature = "meshtastic-proto")]
use anyhow::anyhow;
use anyhow::Result;
#[cfg(feature = "meshtastic-proto")]
use chrono::Utc;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use super::games::{self, GameDoorKind};
#[cfg(feature = "meshtastic-proto")]
use super::public::PublicCommand;
use super::public::{PublicCommandParser, PublicState};
#[cfg(feature = "meshtastic-proto")]
use super::roles::{role_name, LEVEL_MODERATOR, LEVEL_USER};
use super::session::Session;
#[cfg(feature = "weather")]
use super::weather::WeatherService;
use crate::config::Config;
use crate::logutil::escape_log;
use crate::meshtastic::MeshtasticDevice;
#[cfg(feature = "meshtastic-proto")]
use crate::meshtastic::TextEvent;
#[cfg(feature = "meshtastic-proto")]
use crate::meshtastic::{ControlMessage, MessagePriority, OutgoingMessage};
use crate::storage::Storage;
use crate::validation::validate_sysop_name;

macro_rules! sec_log {
    ($($arg:tt)*) => { log::warn!(target: "security", $($arg)*); };
}
#[allow(unused_imports)]
pub(crate) use sec_log;

/// # BBS Server - Core Application Controller
///
/// The `BbsServer` is the main orchestrator for the Meshbbs system, coordinating
/// all components and managing the overall application lifecycle.
///
/// ## Responsibilities
///
/// - **Device Management**: Controls Meshtastic device communication
/// - **Session Coordination**: Manages active user sessions and state
/// - **Message Routing**: Routes messages between public and private channels
/// - **Storage Integration**: Coordinates with the storage layer for persistence
/// - **Weather Services**: Provides proactive weather updates (when enabled)
/// - **Security Enforcement**: Implements authentication and authorization
///
/// ## Architecture
///
/// The server implements an event-driven architecture using Tokio async/await:
///
/// ```text
/// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
/// │  Meshtastic     │───→│   BbsServer     │───→│    Storage      │
/// │  Device         │    │   (Core)        │    │    Layer        │
/// └─────────────────┘    └─────────────────┘    └─────────────────┘
///                               │
/// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
/// │  Session        │←───│                 │───→│   Public        │
/// │  Manager        │    │                 │    │   Commands      │
/// └─────────────────┘    └─────────────────┘    └─────────────────┘
/// ```
///
/// ## Usage
///
/// ```rust,no_run
/// use meshbbs::bbs::BbsServer;
/// use meshbbs::config::Config;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     // Load configuration
///     let config = Config::load("config.toml").await?;
///
///     // Create and initialize server
///     let mut server = BbsServer::new(config).await?;
///
///     // Run the server (blocks until shutdown)
///     server.run().await?;
///
///     Ok(())
/// }
/// ```
///
/// ## Features
///
/// - **Async/Await**: Full async support for high concurrency
/// - **Hot Configuration**: Runtime configuration updates
/// - **Session Management**: Automatic timeout and cleanup
/// - **Weather Integration**: Proactive weather updates every 5 minutes
/// - **Audit Logging**: Comprehensive security and administrative logging
/// - **Protocol Support**: Both text and protobuf Meshtastic protocols
///
/// ## Thread Safety
///
/// The `BbsServer` is designed for single-threaded async operation within a Tokio runtime.
/// Internal state is managed safely through async coordination patterns.
pub struct BbsServer {
    config: Config,
    storage: Storage,
    device: Option<MeshtasticDevice>,
    sessions: HashMap<String, Session>,
    message_tx: Option<mpsc::UnboundedSender<String>>,
    #[cfg(feature = "meshtastic-proto")]
    text_event_rx: Option<mpsc::UnboundedReceiver<TextEvent>>,
    #[cfg(feature = "meshtastic-proto")]
    node_detection_rx: Option<mpsc::UnboundedReceiver<crate::meshtastic::NodeDetectionEvent>>,
    #[cfg(feature = "meshtastic-proto")]
    outgoing_tx: Option<mpsc::UnboundedSender<OutgoingMessage>>,
    #[cfg(feature = "meshtastic-proto")]
    scheduler: Option<crate::bbs::dispatch::SchedulerHandle>,
    #[cfg(feature = "meshtastic-proto")]
    reader_control_tx: Option<mpsc::UnboundedSender<ControlMessage>>,
    #[cfg(feature = "meshtastic-proto")]
    writer_control_tx: Option<mpsc::UnboundedSender<ControlMessage>>,
    #[cfg(feature = "meshtastic-proto")]
    node_id_rx: Option<mpsc::UnboundedReceiver<u32>>, // reader-provided node ID notifications
    public_state: PublicState,
    public_parser: PublicCommandParser,
    #[cfg(feature = "weather")]
    weather_service: WeatherService,
    #[cfg(feature = "weather")]
    weather_last_poll: Instant, // track when we last attempted proactive weather refresh
    housing_cleanup_last_check: Instant, // track when we last ran housing cleanup
    housing_payment_last_check: Instant, // track when we last processed recurring payments
    backup_scheduler: Option<crate::storage::backup_scheduler::BackupScheduler>, // automatic backup scheduler
    #[cfg(feature = "meshtastic-proto")]
    pending_direct: Vec<(u32, u32, String)>, // queue of (dest_node_id, channel, message) awaiting our node id
    #[cfg(feature = "meshtastic-proto")]
    node_cache_last_cleanup: Instant, // track when we last cleaned up stale nodes
    #[cfg(feature = "meshtastic-proto")]
    our_node_id: Option<u32>, // our node ID for ident broadcasts
    #[cfg(feature = "meshtastic-proto")]
    startup_instant: Instant, // grace period start for ident when using reader/writer
    #[cfg(feature = "meshtastic-proto")]
    last_ident_time: Instant, // track when we last sent an ident beacon
    #[cfg(feature = "meshtastic-proto")]
    last_ident_boundary_minute: Option<i64>, // dedupe key: unix-minute of last ident send
    #[cfg(feature = "meshtastic-proto")]
    welcome_state: Option<crate::bbs::welcome::WelcomeState>,
    #[cfg(feature = "meshtastic-proto")]
    startup_welcome_queue: Vec<(tokio::time::Instant, crate::meshtastic::NodeDetectionEvent)>, // (when_to_send, event)
    #[cfg(feature = "meshtastic-proto")]
    startup_welcomes_queued: bool, // track if we've already queued startup welcomes
    /// Game registry holding all door game resources
    game_registry: crate::bbs::GameRegistry,
    #[allow(dead_code)]
    #[doc(hidden)]
    pub(crate) test_messages: Vec<(String, String)>, // collected outbound messages (testing)
    #[cfg(feature = "meshtastic-proto")]
    last_scheduler_stats_check: Instant, // track when we last checked scheduler health
    #[cfg(feature = "meshtastic-proto")]
    scheduler_drops_last_count: u64, // track drops to detect new drops
    #[cfg(feature = "meshtastic-proto")]
    scheduler_queue_high_warnings: u64, // count high queue warnings
}

// Verbose HELP material & chunker (outside impl so usable without Self scoping issues during compilation ordering)
fn verbose_help_with_prefix(pfx: char, help_cmd: &str) -> String {
    format!(
        concat!(
        "Meshbbs Extended Help\n",
        "Authentication:\n  REGISTER <name> <pass>  Create account\n  LOGIN <name> <pass>     Log in\n  SETPASS <new>           Set first password\n  CHPASS <old> <new>      Change password\n  LOGOUT                  End session\n\n",
        "Compact Navigation:\n  M       Topics menu (paged)\n  1-9     Pick item on page\n  L       More items\n  U/B     Up/back (to parent)\n  X       Exit\n  WHERE/W Where am I breadcrumb\n\n",
    "Topics → Subtopics → Threads → Read:\n  In Subtopics: 1-9 pick, U up\n  In Threads:   1-9 read, N new, F <text> filter, U up\n  In Read:      + next, - prev, Y reply\n\n",
    "Posting:\n  From Topics:  R recent messages  P compose  L list\n  While posting: type message text, '.' to finish or cancel\n\n",
        "Moderator (level 5+):\n  Threads:  D<n> delete  P<n> pin/unpin  R<n> <title> rename  K lock/unlock area\n  Read:     D delete     P pin/unpin     R <title>            K lock/unlock area\n\n",
        "Sysop (level 10):\n  G @user=LEVEL|ROLE      Grant level (1/5/10) or USER/MOD/SYSOP\n  SYSLOG LEVEL <msg>      Log message (INFO/WARN/ERROR)\n\n",
        "Administration (mod/sysop):\n  USERS [pattern]         List users (filter optional)\n  WHO                     Show logged-in users\n  USERINFO <user>         Detailed user info\n  SESSIONS                List all sessions\n  KICK <user>             Force logout user\n  BROADCAST <msg>         Broadcast to all\n  ADMIN / DASHBOARD       System overview\n\n",
        "Misc:\n  {h}        Compact help\n  {h}+ / {h} V  Verbose help (this)\n  Weather (public)       Send WEATHER on public channel\n  Slot Machine (public)  {p}SLOT or {p}SLOTMACHINE to play\n  Slot Stats (public)    {p}SLOTSTATS\n  Magic 8-Ball (public)  {p}8BALL\n  Fortune (public)       {p}FORTUNE for classic Unix wisdom\n\n",
        "Limits:\n  Max frame ~230 bytes; verbose help auto-splits.\n"),
        p = pfx,
        h = help_cmd
    )
}

fn chunk_verbose_help_with_prefix(pfx: char, help_cmd: &str) -> Vec<String> {
    const MAX: usize = 230;
    let mut chunks = Vec::new();
    let verbose = verbose_help_with_prefix(pfx, help_cmd);
    let mut current = String::new();
    for line in verbose.lines() {
        let candidate_len = current.len() + line.len() + 1;
        if candidate_len > MAX && !current.is_empty() {
            chunks.push(current);
            current = String::new();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

impl BbsServer {
    #[inline]
    fn primary_channel(&self) -> u32 {
        self.config.meshtastic.channel as u32
    }

    /// Return the configured primary Meshtastic channel (as u32).
    /// Chunk a UTF-8 string into <= max_bytes segments without splitting codepoints.
    /// Attempts to split on newline boundaries preferentially, then falls back to byte slicing.
    pub fn chunk_utf8(&self, text: &str, max_bytes: usize) -> Vec<String> {
        if text.len() <= max_bytes {
            return vec![text.to_string()];
        }
        let mut chunks = Vec::new();
        let mut remaining = text;
        while !remaining.is_empty() {
            if remaining.len() <= max_bytes {
                chunks.push(remaining.to_string());
                break;
            }
            // Take up to max_bytes, then retreat to last newline or char boundary
            let mut end = max_bytes;
            let bytes = remaining.as_bytes();
            if end > bytes.len() {
                end = bytes.len();
            }
            // Retreat until on UTF-8 boundary using Rust's built-in check
            while end > 0 && !remaining.is_char_boundary(end) {
                end -= 1;
            }
            // Try newline preference within this slice (find last '\n')
            let slice = &remaining[..end];
            if let Some(pos) = slice.rfind('\n') {
                if pos > 0 && pos + 1 >= end / 2 {
                    // keep heuristic simple
                    let piece = &slice[..=pos];
                    chunks.push(piece.to_string());
                    remaining = &remaining[pos + 1..];
                    continue;
                }
            }
            chunks.push(slice.to_string());
            remaining = &remaining[end..];
        }
        chunks
    }
    #[inline]
    fn lookup_short_name_from_cache(&self, id: u32) -> Option<String> {
        #[derive(serde::Deserialize)]
        struct CachedNodeInfo {
            short_name: String,
            #[allow(dead_code)]
            long_name: String,
        }
        #[derive(serde::Deserialize)]
        struct NodeCache {
            nodes: std::collections::HashMap<u32, CachedNodeInfo>,
        }
        let path = "data/node_cache.json";
        let content = std::fs::read_to_string(path).ok()?;
        let cleaned = content.trim_start_matches('\0');
        let cache: NodeCache = serde_json::from_str(cleaned).ok()?;
        cache.nodes.get(&id).and_then(|n| {
            let sn = n.short_name.trim();
            if sn.is_empty() {
                None
            } else {
                Some(sn.to_string())
            }
        })
    }

    /// Write the current welcome queue status to a JSON file for monitoring
    #[cfg(feature = "meshtastic-proto")]
    fn write_welcome_queue_status(&self) {
        use serde::Serialize;

        #[derive(Serialize)]
        struct QueueEntry {
            node_id: String,
            long_name: String,
            short_name: String,
            sends_in_seconds: i64,
        }

        #[derive(Serialize)]
        struct QueueStatus {
            queue_length: usize,
            entries: Vec<QueueEntry>,
        }

        let now = tokio::time::Instant::now();
        let entries: Vec<QueueEntry> = self
            .startup_welcome_queue
            .iter()
            .map(|(send_time, event)| {
                let delay = send_time.saturating_duration_since(now);
                QueueEntry {
                    node_id: format!("0x{:08X}", event.node_id),
                    long_name: event.long_name.clone(),
                    short_name: event.short_name.clone(),
                    sends_in_seconds: delay.as_secs() as i64,
                }
            })
            .collect();

        let status = QueueStatus {
            queue_length: entries.len(),
            entries,
        };

        let path = "data/welcome_queue.json";
        match serde_json::to_string_pretty(&status) {
            Ok(json) => match std::fs::write(path, json) {
                Ok(_) => debug!(
                    "Wrote welcome queue status: {} entries",
                    status.queue_length
                ),
                Err(e) => warn!("Failed to write welcome queue status file: {}", e),
            },
            Err(e) => warn!("Failed to serialize welcome queue status: {}", e),
        }
    }

    #[inline]
    fn lookup_long_name_from_cache(&self, id: u32) -> Option<String> {
        #[derive(serde::Deserialize)]
        struct CachedNodeInfo {
            #[allow(dead_code)]
            short_name: String,
            long_name: String,
        }
        #[derive(serde::Deserialize)]
        struct NodeCache {
            nodes: std::collections::HashMap<u32, CachedNodeInfo>,
        }
        let path = "data/node_cache.json";
        let content = std::fs::read_to_string(path).ok()?;
        let cleaned = content.trim_start_matches('\0');
        let cache: NodeCache = serde_json::from_str(cleaned).ok()?;
        cache.nodes.get(&id).and_then(|n| {
            let ln = n.long_name.trim();
            if ln.is_empty() {
                None
            } else {
                Some(ln.to_string())
            }
        })
    }

    /// Creates a new BBS server instance with the provided configuration.
    ///
    /// This function initializes all components of the BBS system including storage,
    /// device communication, session management, and configuration validation.
    ///
    /// # Arguments
    ///
    /// * `config` - A validated [`Config`] instance containing all system settings
    ///
    /// # Returns
    ///
    /// Returns a `Result<BbsServer>` that contains the initialized server on success,
    /// or an error describing what went wrong during initialization.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The sysop name in the configuration is invalid
    /// - The storage system cannot be initialized
    /// - The data directory cannot be created or accessed
    /// - Argon2 parameters are invalid (if custom security config provided)
    /// - Message topic configuration is malformed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use meshbbs::bbs::BbsServer;
    /// use meshbbs::config::Config;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let config = Config::load("config.toml").await?;
    ///     let server = BbsServer::new(config).await?;
    ///     // Server is now ready to run
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Configuration Validation
    ///
    /// The function performs comprehensive validation including:
    /// - Sysop name format and reserved name checking
    /// - Message topic access level validation
    /// - Storage directory permissions
    /// - Security parameter validation
    pub async fn new(config: Config) -> Result<Self> {
        // Validate sysop name before starting BBS
        if let Err(e) = validate_sysop_name(&config.bbs.sysop) {
            return Err(anyhow::anyhow!(
                "Invalid sysop name '{}': {}\n\n\
                SOLUTION: Edit your config.toml file and change the 'sysop' field to a valid name.\n\
                Valid sysop names must:\n\
                • Be 2-20 characters long\n\
                • Contain only letters, numbers, spaces, underscores, hyphens, and periods\n\
                • Not start or end with spaces\n\
                • Not be a reserved system name\n\
                • Not contain path separators or special filesystem characters\n\n\
                Examples of valid sysop names:\n\
                • sysop = \"admin\"\n\
                • sysop = \"John Smith\"\n\
                • sysop = \"BBS_Operator\"\n\
                • sysop = \"station-1\"",
                config.bbs.sysop, e
            ));
        }

        // Build optional Argon2 params from config
        let storage = {
            use argon2::Params;
            if let Some(sec) = &config.security {
                if let Some(a) = &sec.argon2 {
                    let builder = Params::DEFAULT;
                    let mem = a.memory_kib.unwrap_or(builder.m_cost());
                    let time = a.time_cost.unwrap_or(builder.t_cost());
                    let para = a.parallelism.unwrap_or(builder.p_cost());
                    let params = Params::new(mem, time, para, None).ok();
                    Storage::new_with_params(&config.storage.data_dir, params).await?
                } else {
                    Storage::new(&config.storage.data_dir).await?
                }
            } else {
                Storage::new(&config.storage.data_dir).await?
            }
        };

        let mut server = Self {
            config: config.clone(),
            storage,
            device: None,
            sessions: HashMap::new(),
            message_tx: None,
            #[cfg(feature = "meshtastic-proto")]
            text_event_rx: None,
            #[cfg(feature = "meshtastic-proto")]
            node_detection_rx: None,
            #[cfg(feature = "meshtastic-proto")]
            outgoing_tx: None,
            #[cfg(feature = "meshtastic-proto")]
            scheduler: None,
            #[cfg(feature = "meshtastic-proto")]
            reader_control_tx: None,
            #[cfg(feature = "meshtastic-proto")]
            writer_control_tx: None,
            #[cfg(feature = "meshtastic-proto")]
            node_id_rx: None,
            public_state: PublicState::new(
                std::time::Duration::from_secs(20),
                std::time::Duration::from_secs(300),
            ),
            public_parser: PublicCommandParser::new_with_prefix(
                config.bbs.public_command_prefix.clone(),
                Some(config.bbs.help_command.clone()),
            ),
            #[cfg(feature = "weather")]
            weather_service: WeatherService::new(config.apps.weather.clone()),
            #[cfg(feature = "weather")]
            weather_last_poll: Instant::now() - Duration::from_secs(301),
            // Initialize housing cleanup timers to run immediately on first tick
            housing_cleanup_last_check: Instant::now() - Duration::from_secs(86401), // More than 1 day ago
            housing_payment_last_check: Instant::now() - Duration::from_secs(86401), // More than 1 day ago
            // Initialize backup scheduler
            backup_scheduler: {
                use crate::storage::backup_scheduler::{BackupScheduler, BackupSchedulerConfig};
                let scheduler_config = BackupSchedulerConfig::load().unwrap_or_else(|e| {
                    warn!(
                        "Failed to load backup scheduler config, using defaults: {}",
                        e
                    );
                    BackupSchedulerConfig::default()
                });
                info!(
                    "Backup scheduler initialized: enabled={}, frequency={:?}",
                    scheduler_config.enabled, scheduler_config.frequency
                );
                Some(BackupScheduler::new(scheduler_config))
            },
            #[cfg(feature = "meshtastic-proto")]
            pending_direct: Vec::new(),
            #[cfg(feature = "meshtastic-proto")]
            node_cache_last_cleanup: Instant::now() - Duration::from_secs(3601),
            #[cfg(feature = "meshtastic-proto")]
            our_node_id: None,
            #[cfg(feature = "meshtastic-proto")]
            startup_instant: Instant::now(),
            #[cfg(feature = "meshtastic-proto")]
            last_ident_time: Instant::now() - Duration::from_secs(901), // Start ready to send
            #[cfg(feature = "meshtastic-proto")]
            last_ident_boundary_minute: None,
            #[cfg(feature = "meshtastic-proto")]
            welcome_state: if config.apps.welcome.enabled {
                Some(crate::bbs::welcome::WelcomeState::new(
                    &config.storage.data_dir,
                ))
            } else {
                None
            },
            #[cfg(feature = "meshtastic-proto")]
            startup_welcome_queue: Vec::new(),
            #[cfg(feature = "meshtastic-proto")]
            startup_welcomes_queued: false,
            game_registry: crate::bbs::GameRegistry::new(),
            test_messages: Vec::new(),
            #[cfg(feature = "meshtastic-proto")]
            last_scheduler_stats_check: Instant::now(),
            #[cfg(feature = "meshtastic-proto")]
            scheduler_drops_last_count: 0,
            #[cfg(feature = "meshtastic-proto")]
            scheduler_queue_high_warnings: 0,
        };
        // Legacy compatibility: previously, topics could be defined in TOML.
        // New behavior initializes topics in data/topics.json during `meshbbs init`.
        // We keep a soft-compat path for existing configs that still have message_topics.
        if !server.config.message_topics.is_empty() {
            Self::merge_toml_topics_to_runtime(&mut server.storage, &server.config).await?;
        }

        // Initialize door game resources in the registry
        if server.config.apps.tinymush.enabled {
            let db_path = server
                .config
                .apps
                .tinymush
                .db_path
                .as_deref()
                .unwrap_or("data/tinymush");

            // Use BBS sysop username as TinyMUSH admin
            let admin_username = server.config.bbs.sysop.clone();

            match crate::tmush::storage::TinyMushStoreBuilder::new(db_path)
                .with_admin_username(admin_username)
                .open()
            {
                Ok(store) => {
                    info!("[games] TinyMUSH store initialized at: {}", db_path);
                    server.game_registry = server.game_registry.with_tinymush(store);
                }
                Err(e) => {
                    warn!("[games] TinyMUSH store initialization failed: {}", e);
                }
            }
        }

        // Initialize fortune data from JSON file
        if server.config.apps.fortune.enabled {
            use std::path::Path;
            let data_dir = Path::new(&server.config.storage.data_dir);
            match crate::bbs::fortune::initialize(data_dir) {
                crate::bbs::fortune::FortuneStatus::Ready(count) => {
                    info!("[apps] Fortune feature initialized with {} fortunes", count);
                }
                crate::bbs::fortune::FortuneStatus::Disabled(reason) => {
                    warn!("[apps] Fortune feature disabled: {}", reason);
                    // Feature will be unavailable but BBS continues running
                }
            }
        }

        // Initialize 8-ball feature
        if server.config.apps.eightball.enabled {
            let data_dir = std::path::Path::new(&server.config.storage.data_dir);
            match crate::bbs::eightball::initialize(data_dir) {
                crate::bbs::eightball::EightballStatus::Available(count) => {
                    info!("[apps] 8-Ball feature initialized with {} responses", count);
                }
                crate::bbs::eightball::EightballStatus::FileNotFound(path) => {
                    warn!(
                        "[apps] 8-Ball feature unavailable: data file not found at {}",
                        path
                    );
                }
                crate::bbs::eightball::EightballStatus::ParseError(e) => {
                    warn!("[apps] 8-Ball feature unavailable: {}", e);
                }
                crate::bbs::eightball::EightballStatus::ValidationError(e) => {
                    warn!("[apps] 8-Ball feature unavailable: {}", e);
                }
            }
        }

        // Announce enabled games at startup
        if server.config.apps.tinyhack.enabled {
            info!(
                "[games] TinyHack enabled: DM games menu (G -> 1) active; saves at {}/tinyhack",
                server.storage.base_dir()
            );
        }
        if server.config.apps.tinymush.enabled {
            info!("[games] TinyMUSH enabled: available in Games menu");
        }

        // Start admin web dashboard if enabled
        #[cfg(feature = "webui")]
        if server.config.admin_dashboard.enabled {
            let config_clone = server.config.clone();
            let data_dir = server.config.storage.data_dir.clone();
            let tinymush_store = server.game_registry.get_tinymush_store().cloned();
            tokio::spawn(async move {
                // Create a separate Storage instance for the web UI
                let storage = match Storage::new(&data_dir).await {
                    Ok(s) => Some(s),
                    Err(e) => {
                        error!("[webui] Failed to initialize storage for web UI: {}", e);
                        None
                    }
                };

                if let Err(e) =
                    crate::webui::start_webui_server(config_clone, storage, tinymush_store).await
                {
                    error!("[webui] Failed to start admin dashboard: {}", e);
                } else {
                    info!("[webui] Admin dashboard started successfully");
                }
            });
        }

        Ok(server)
    }

    /// Connect to a Meshtastic device using the new reader/writer pattern
    #[cfg(feature = "meshtastic-proto")]
    pub async fn connect_device(&mut self, port: &str) -> Result<()> {
        info!(
            "Connecting to Meshtastic device on {} using reader/writer pattern",
            port
        );

        // Build writer tuning from config (with enforced 2s minimum)
        let mcfg = &self.config.meshtastic;
        let mut min_send_gap_ms = mcfg.min_send_gap_ms.unwrap_or(2000);
        if min_send_gap_ms < 2000 {
            warn!(
                "Configured min_send_gap_ms={}ms is below 2000ms; clamping to 2000ms",
                min_send_gap_ms
            );
            min_send_gap_ms = 2000;
        }
        let mut backoffs = mcfg
            .dm_resend_backoff_seconds
            .clone()
            .unwrap_or_else(|| vec![4, 8, 16]);
        if backoffs.is_empty() {
            backoffs = vec![4, 8, 16];
        }
        // sanitize non-positive entries
        backoffs.retain(|&s| s > 0);
        if backoffs.is_empty() {
            backoffs = vec![4, 8, 16];
        }
        let tuning = crate::meshtastic::WriterTuning {
            min_send_gap_ms,
            dm_resend_backoff_seconds: backoffs,
            post_dm_broadcast_gap_ms: mcfg.post_dm_broadcast_gap_ms.unwrap_or(1200),
            dm_to_dm_gap_ms: mcfg.dm_to_dm_gap_ms.unwrap_or(600),
        };

        // Create the reader/writer system
        let tuning_clone = tuning.clone();
        let (
            reader,
            writer,
            text_event_rx,
            node_detection_rx,
            outgoing_tx,
            reader_control_tx,
            writer_control_tx,
            node_id_rx,
        ) = crate::meshtastic::create_reader_writer_system(
            port,
            self.config.meshtastic.baud_rate,
            tuning_clone,
        )
        .await?;

        // Store the channels in the server
        self.text_event_rx = Some(text_event_rx);
        self.node_detection_rx = Some(node_detection_rx);
        // Start scheduler (phase 1) before storing outgoing for general use
        let help_delay_ms = mcfg.help_broadcast_delay_ms.unwrap_or(3500);
        let sched_cfg = crate::bbs::dispatch::SchedulerConfig {
            min_send_gap_ms: tuning.min_send_gap_ms,
            post_dm_broadcast_gap_ms: tuning.post_dm_broadcast_gap_ms,
            help_broadcast_delay_ms: help_delay_ms,
            max_queue: mcfg.scheduler_max_queue.unwrap_or(512),
            aging_threshold_ms: mcfg.scheduler_aging_threshold_ms.unwrap_or(5000),
            stats_interval_ms: mcfg.scheduler_stats_interval_ms.unwrap_or(10000),
        };
        let scheduler_handle =
            crate::bbs::dispatch::start_scheduler(sched_cfg, outgoing_tx.clone());
        self.scheduler = Some(scheduler_handle);
        self.outgoing_tx = Some(outgoing_tx);
        self.reader_control_tx = Some(reader_control_tx);
        self.writer_control_tx = Some(writer_control_tx);
        self.node_id_rx = Some(node_id_rx);

        // Provide scheduler handle to writer for retry scheduling (best-effort)
        if let (Some(sched), Some(ctrl)) = (&self.scheduler, &self.writer_control_tx) {
            let _ = ctrl.send(crate::meshtastic::ControlMessage::SetSchedulerHandle(
                sched.clone(),
            ));
        }

        // Spawn the reader and writer tasks
        tokio::spawn(async move {
            if let Err(e) = reader.run().await {
                error!("Reader task failed: {}", e);
            }
        });

        tokio::spawn(async move {
            if let Err(e) = writer.run().await {
                error!("Writer task failed: {}", e);
            }
        });

        info!("Meshtastic reader/writer tasks spawned successfully");
        Ok(())
    }

    /// Scan node cache on startup and queue welcomes for recently active unwelcomed nodes
    #[cfg(feature = "meshtastic-proto")]
    pub async fn queue_startup_welcomes(&mut self) -> Result<()> {
        // Only proceed if welcome system is enabled
        if !self.config.apps.welcome.enabled || self.welcome_state.is_none() {
            return Ok(());
        }

        // Load node cache
        let cache_path = format!("{}/node_cache.json", self.config.storage.data_dir);
        let cache = match crate::meshtastic::NodeCache::load_from_file(&cache_path) {
            Ok(c) => c,
            Err(e) => {
                debug!("No node cache to scan for startup welcomes: {}", e);
                return Ok(());
            }
        };

        info!(
            "Scanning {} cached nodes for startup welcomes",
            cache.nodes.len()
        );

        // Get current time for age check (within last hour)
        let now = chrono::Utc::now();
        let one_hour_ago = now - chrono::Duration::hours(1);

        // Collect nodes that need welcoming
        let mut to_welcome = Vec::new();

        for (node_id, cached_node) in &cache.nodes {
            // Skip if not recent (last seen must be within 1 hour)
            if cached_node.last_seen < one_hour_ago {
                continue;
            }

            // Skip if not a default name
            if !crate::bbs::welcome::is_default_name(&cached_node.long_name) {
                continue;
            }

            // Check if already welcomed (use true since we'll queue these for staggered sending)
            let should_welcome = {
                let state = self.welcome_state.as_ref().unwrap();
                state.should_welcome(
                    *node_id,
                    &cached_node.long_name,
                    &self.config.apps.welcome,
                    true,
                )
            };

            if should_welcome {
                to_welcome.push((
                    *node_id,
                    cached_node.long_name.clone(),
                    cached_node.short_name.clone(),
                ));
            }
        }

        if to_welcome.is_empty() {
            info!("No cached nodes need startup welcomes");
            return Ok(());
        }

        info!(
            "Queuing {} startup welcomes (30s intervals)",
            to_welcome.len()
        );

        // Queue the welcomes with 30-second stagger
        let base_time = tokio::time::Instant::now();
        for (delay_index, (node_id, long_name, short_name)) in to_welcome.into_iter().enumerate() {
            let delay_secs = delay_index as u64 * 30; // 0s, 30s, 60s, 90s...
            let send_time = base_time + tokio::time::Duration::from_secs(delay_secs);

            let event = crate::meshtastic::NodeDetectionEvent {
                node_id,
                long_name,
                short_name,
                is_from_startup_queue: true, // Startup queue - skip rate limit
            };

            self.startup_welcome_queue.push((send_time, event));
        }

        // Write queue status to file for monitoring
        self.write_welcome_queue_status();

        Ok(())
    }

    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn test_messages(&self) -> &Vec<(String, String)> {
        &self.test_messages
    }
    // Expose scheduler handle for tests (used by scheduler_overflow.rs). Keep public until a dedicated test API is introduced.
    #[cfg(feature = "meshtastic-proto")]
    #[allow(dead_code)] // Used only in tests (scheduler_overflow); suppress warning in release builds.
    pub fn scheduler_handle(&self) -> Option<crate::bbs::dispatch::SchedulerHandle> {
        self.scheduler.clone()
    }
    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn test_insert_session(&mut self, session: Session) {
        self.sessions.insert(session.node_id.clone(), session);
    }
    #[cfg(feature = "meshtastic-proto")]
    #[doc(hidden)]
    #[allow(dead_code)] // Used only by integration tests (help_broadcast_delay) to inject an outgoing channel
    pub fn test_set_outgoing(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::meshtastic::OutgoingMessage>,
    ) {
        self.outgoing_tx = Some(tx);
    }

    /// Check if it's time to send an ident beacon and send it if so.
    ///
    /// Behavior:
    /// - Respects `[ident_beacon]` configuration (`enabled`, `frequency`).
    /// - Scheduling occurs on UTC boundaries for the configured cadence (5/15/30 minutes, 1/2/4 hours).
    /// - If a device instance is present, waits for initial radio sync to complete before sending.
    /// - If using the reader/writer pattern with no device instance, applies a short startup grace period,
    ///   then proceeds to send on the next UTC boundary. Prevents duplicate sends within the same minute.
    #[cfg(feature = "meshtastic-proto")]
    async fn check_and_send_ident(&mut self) -> Result<()> {
        // Check if ident beacon is enabled
        if !self.config.apps.ident_beacon.enabled {
            return Ok(());
        }

        // Check if initial radio configuration is complete (or allow a short startup grace period)
        if let Some(ref device) = self.device {
            if !device.initial_sync_complete() {
                debug!("Ident beacon waiting for initial radio config to complete");
                return Ok(());
            }
        } else {
            // In reader/writer mode, BbsServer.device is not used for sending.
            // Prefer to proceed once we've learned our node ID from the reader.
            if self.our_node_id.is_none() {
                let since_start = self.startup_instant.elapsed();
                if since_start < Duration::from_secs(120) {
                    debug!("Ident beacon waiting for our node ID");
                    return Ok(());
                }
                // Fallback: after grace period, proceed even without node ID (will show 'Unknown').
            }
            // Proceed based on time boundary so idents don't get stuck forever.
        }

        use chrono::{Timelike, Utc};

        let now = Utc::now();
        let minutes = now.minute();
        let frequency_minutes = self.config.apps.ident_beacon.frequency_minutes();

        // Check if we're at the right time boundary based on configured frequency
        let should_send = match frequency_minutes {
            5 => minutes % 5 == 0,   // Every 5 minutes (:00, :05, :10, ...)
            15 => minutes % 15 == 0, // Every 15 minutes (0, 15, 30, 45)
            30 => minutes % 30 == 0, // Every 30 minutes (0, 30)
            60 => minutes == 0,      // Every hour (0)
            120 => minutes == 0 && now.hour() % 2 == 0, // Every 2 hours (0, 2, 4, ...)
            240 => minutes == 0 && now.hour() % 4 == 0, // Every 4 hours (0, 4, 8, ...)
            _ => minutes % 15 == 0,  // Default to 15 minutes for invalid config
        };

        if !should_send {
            return Ok(());
        }

        // Compute boundary minute key (Unix epoch minutes) for dedupe within the same scheduled minute
        let boundary_minute = now.timestamp() / 60; // i64 minutes since epoch
        if let Some(last_min) = self.last_ident_boundary_minute {
            if last_min == boundary_minute {
                // Already sent an ident in this minute boundary; skip duplicate
                return Ok(());
            }
        }

        // Determine identifier display: prefer short name, fallback to 4-hex-digit short ID
        let id_display = if let Some(node_id) = self.our_node_id {
            if let Some(sn) = self.lookup_short_name_from_cache(node_id) {
                sn
            } else {
                format!("0x{:04X}", node_id & 0xFFFF)
            }
        } else {
            // our_node_id unknown: try config node_id for a hex fallback
            let nid = if !self.config.meshtastic.node_id.is_empty() {
                if let Ok(id) = self.config.meshtastic.node_id.parse::<u32>() {
                    Some(id)
                } else if let Some(hex) = self
                    .config
                    .meshtastic
                    .node_id
                    .strip_prefix("0x")
                    .or_else(|| self.config.meshtastic.node_id.strip_prefix("0X"))
                {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(id) = nid {
                format!("0x{:04X}", id & 0xFFFF)
            } else {
                "Unknown".to_string()
            }
        };

        let ident_prefix = self.public_parser.primary_prefix_char();
        let help_cmd = self.public_parser.help_command();
        let ident_msg = format!(
            "[IDENT] {} ({}) - {} UTC - Type {}{} for commands",
            self.config.bbs.name,
            id_display,
            now.format("%Y-%m-%d %H:%M:%S"),
            ident_prefix,
            help_cmd
        );

        // Send to public channel
        if let Err(e) = self.send_broadcast(&ident_msg).await {
            warn!("Failed to send ident beacon: {}", e);
        } else {
            info!("Sent ident beacon: {}", ident_msg);
            self.last_ident_time = Instant::now();
            self.last_ident_boundary_minute = Some(boundary_minute);
        }

        Ok(())
    }

    /// Check if it's time to create an automatic backup and create one if so.
    ///
    /// This method should be called periodically from the main event loop.
    /// The scheduler determines if a backup is needed based on UTC time boundaries
    /// and the configured frequency.
    ///
    /// # Behavior
    /// - Respects automatic backup configuration (enabled/disabled, frequency)
    /// - Scheduling occurs on UTC boundaries (hourly, 2h, 4h, 6h, 12h, daily)
    /// - Automatically applies retention policy after creating backups
    /// - Prevents duplicate backups within the same minute boundary
    /// - Reloads configuration from disk periodically (respects in-game changes)
    async fn check_and_backup(&mut self) -> Result<()> {
        if let Some(ref mut scheduler) = self.backup_scheduler {
            // Reload config from disk to pick up changes from /BACKUPCONFIG commands
            let _ = scheduler.reload_config();

            match scheduler.check_and_backup() {
                Ok(Some(backup_id)) => {
                    info!("Automatic backup created: {}", backup_id);
                }
                Ok(None) => {
                    // No backup needed at this time
                }
                Err(e) => {
                    warn!("Failed to create automatic backup: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Check scheduler health and log warnings about queue saturation
    ///
    /// This monitors the message scheduler for signs of trouble:
    /// - High queue depth (>80% capacity)
    /// - Message drops due to overflow
    /// - Escalations due to aging
    ///
    /// Called every 30 seconds from the main event loop.
    #[cfg(feature = "meshtastic-proto")]
    async fn check_scheduler_health(&mut self) -> Result<()> {
        if let Some(ref scheduler) = self.scheduler {
            if let Some(stats) = scheduler.snapshot().await {
                let max_queue = self.config.meshtastic.scheduler_max_queue.unwrap_or(512);
                let queue_pct = (stats.queued as f64 / max_queue as f64) * 100.0;

                // Warn if queue is >80% full
                if stats.queued > (max_queue * 80 / 100) {
                    self.scheduler_queue_high_warnings += 1;
                    warn!(
                        "Scheduler queue high: {}/{} messages ({:.1}%) - drops={} escalations={} (warning #{})",
                        stats.queued, max_queue, queue_pct, stats.dropped_total,
                        stats.escalations, self.scheduler_queue_high_warnings
                    );
                }

                // Alert if new drops detected
                if stats.dropped_total > self.scheduler_drops_last_count {
                    let new_drops = stats.dropped_total - self.scheduler_drops_last_count;
                    error!(
                        "Message scheduler dropping messages! New drops: {} (total: {}, overflow: {})",
                        new_drops, stats.dropped_total, stats.dropped_overflow
                    );
                    self.scheduler_drops_last_count = stats.dropped_total;
                }

                // Periodic info log when things are healthy
                if stats.queued == 0 && self.scheduler_queue_high_warnings > 0 {
                    info!("Scheduler queue recovered: 0 messages queued");
                    self.scheduler_queue_high_warnings = 0;
                }
            }
        }
        Ok(())
    }

    /// 4. **Session Management**: Handles user session lifecycle and timeouts
    /// 5. **Weather Updates**: Provides proactive weather information (if enabled)
    /// 6. **Audit Logging**: Records security and administrative events
    ///
    /// # Event Loop
    ///
    /// The server operates on an event-driven model:
    /// - **Device Events**: Messages from the Meshtastic network
    /// - **Internal Messages**: Commands from active sessions
    /// - **Timer Events**: Periodic tasks like weather updates and session cleanup
    /// - **System Events**: Configuration changes and administrative actions
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use meshbbs::bbs::BbsServer;
    /// use meshbbs::config::Config;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let config = Config::load("config.toml").await?;
    ///     let mut server = BbsServer::new(config).await?;
    ///
    ///     // This will run until the server is shut down
    ///     server.run().await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Error Handling
    ///
    /// The method handles various error conditions gracefully:
    /// - Device communication failures are logged and retried
    /// - Session errors are isolated and don't affect other users
    /// - Storage errors are logged and operations are retried when possible
    /// - Configuration errors cause clean shutdown with descriptive messages
    ///
    /// # Shutdown
    ///
    /// The server can be shut down through:
    /// - SIGINT/SIGTERM signals (handled by the runtime)
    /// - Fatal device communication errors
    /// - Storage system failures
    /// - Administrative shutdown commands
    pub async fn run(&mut self) -> Result<()> {
        info!(
            "BBS '{}' started by {}",
            self.config.bbs.name, self.config.bbs.sysop
        );
        self.seed_sysop().await?;

        let (tx, mut rx) = mpsc::unbounded_channel();
        self.message_tx = Some(tx);

        // Setup cross-platform signal handlers for graceful shutdown
        #[cfg(unix)]
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        #[cfg(unix)]
        let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

        #[cfg(windows)]
        let mut ctrl_break = tokio::signal::windows::ctrl_break()?;

        #[cfg(not(any(unix, windows)))]
        let mut _dummy_signal = std::future::pending::<()>();

        // Periodic tick to drive housekeeping even without incoming events
        #[cfg(feature = "meshtastic-proto")]
        let mut periodic = tokio::time::interval(Duration::from_secs(1));
        #[cfg(feature = "meshtastic-proto")]
        periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Fallback interval for non-meshtastic-proto builds
        #[cfg(not(feature = "meshtastic-proto"))]
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        // Main message processing loop
        loop {
            #[cfg(feature = "meshtastic-proto")]
            {
                tokio::select! {
                    // Housekeeping tick
                    _ = periodic.tick() => {
                        // Process startup welcome queue
                        let now = tokio::time::Instant::now();
                        let mut queue_changed = false;
                        while !self.startup_welcome_queue.is_empty() {
                            if let Some((send_time, _)) = self.startup_welcome_queue.first() {
                                if *send_time <= now {
                                    let (_, event) = self.startup_welcome_queue.remove(0);
                                    queue_changed = true;
                                    debug!("Processing startup welcome for node 0x{:08X} ({})", event.node_id, event.long_name);
                                    if let Err(e) = self.handle_node_detection(event).await {
                                        warn!("Startup welcome error: {}", e);
                                    }
                                } else {
                                    break; // Not time yet
                                }
                            } else {
                                break;
                            }
                        }

                        // Update queue status file if we processed any welcomes
                        if queue_changed {
                            self.write_welcome_queue_status();
                        }

                        #[cfg(feature = "weather")]
                        if self.weather_last_poll.elapsed() >= Duration::from_secs(300) {
                            let _ = self.fetch_weather().await;
                            self.weather_last_poll = Instant::now();
                        }

                        // Housing cleanup check (once per day: 86400 seconds)
                        if self.housing_cleanup_last_check.elapsed() >= Duration::from_secs(86400) {
                            if let Err(e) = self.check_housing_cleanup().await {
                                warn!("Housing cleanup error: {}", e);
                            }
                            self.housing_cleanup_last_check = Instant::now();
                        }

                        // Housing recurring payments (once per day: 86400 seconds)
                        if self.housing_payment_last_check.elapsed() >= Duration::from_secs(86400) {
                            if let Err(e) = self.process_housing_payments().await {
                                warn!("Housing payment processing error: {}", e);
                            }
                            self.housing_payment_last_check = Instant::now();
                        }

                        if self.node_cache_last_cleanup.elapsed() >= Duration::from_secs(3600) {
                            self.node_cache_last_cleanup = Instant::now();
                        }
                        if let Err(e) = self.check_and_send_ident().await {
                            debug!("Ident beacon error: {}", e);
                        }

                        // Check if automatic backup is needed
                        if let Err(e) = self.check_and_backup().await {
                            debug!("Backup scheduler error: {}", e);
                        }

                        // Health monitoring: check scheduler status every 30 seconds
                        if self.last_scheduler_stats_check.elapsed() >= Duration::from_secs(30) {
                            if let Err(e) = self.check_scheduler_health().await {
                                warn!("Scheduler health check error: {}", e);
                            }
                            self.last_scheduler_stats_check = Instant::now();
                        }
                    },

                    // Receive our node id from reader (so ident shows actual ID)
                    node_id = async { if let Some(ref mut rx) = self.node_id_rx { rx.recv().await } else { std::future::pending().await } } => {
                        if let Some(id) = node_id {
                            self.our_node_id = Some(id);
                            debug!("Server received our node ID: {}", id);

                            // Queue startup welcomes now that we have our node ID
                            if !self.startup_welcomes_queued {
                                self.startup_welcomes_queued = true;
                                if let Err(e) = self.queue_startup_welcomes().await {
                                    warn!("Failed to queue startup welcomes: {}", e);
                                }
                            }
                        }
                    },
                    // Receive TextEvents from the reader task
                    text_event = async {
                        if let Some(ref mut rx) = self.text_event_rx {
                            rx.recv().await
                        } else {
                            std::future::pending().await
                        }
                    } => {
                        if let Some(event) = text_event {
                            if let Err(e) = self.route_text_event(event).await {
                                warn!("route_text_event error: {e:?}");
                            }
                        } else {
                            warn!("Text event channel closed");
                        }
                    }

                    // Receive NodeDetectionEvents for welcome system
                    node_det = async {
                        if let Some(ref mut rx) = self.node_detection_rx {
                            rx.recv().await
                        } else {
                            std::future::pending().await
                        }
                    } => {
                        if let Some(event) = node_det {
                            if let Err(e) = self.handle_node_detection(event).await {
                                warn!("Welcome system error: {e:?}");
                            }
                        }
                    }

                    msg = rx.recv() => {
                        if let Some(internal_msg) = msg {
                            debug!("Processing internal message: {}", internal_msg);
                        }
                    }

                    // Cross-platform signal handling for graceful shutdown
                    _ = tokio::signal::ctrl_c() => {
                        info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
                        break;
                    }

                    _ = async {
                        #[cfg(unix)]
                        {
                            sigterm.recv().await
                        }
                        #[cfg(not(unix))]
                        {
                            std::future::pending::<Option<()>>().await
                        }
                    } => {
                        info!("Received SIGTERM, initiating graceful shutdown...");
                        break;
                    }

                    _ = async {
                        #[cfg(unix)]
                        {
                            sighup.recv().await
                        }
                        #[cfg(not(unix))]
                        {
                            std::future::pending::<Option<()>>().await
                        }
                    } => {
                        info!("Received SIGHUP, initiating graceful shutdown...");
                        break;
                    }

                    _ = async {
                        #[cfg(windows)]
                        {
                            ctrl_break.recv().await
                        }
                        #[cfg(not(windows))]
                        {
                            std::future::pending::<Option<()>>().await
                        }
                    } => {
                        info!("Received Ctrl+Break, initiating graceful shutdown...");
                        break;
                    }
                }
            }

            #[cfg(not(feature = "meshtastic-proto"))]
            {
                tokio::select! {
                    _ = interval.tick() => {
                        // Fallback for when meshtastic-proto feature is disabled
                        if let Some(ref mut device) = self.device {
                            if let Ok(Some(summary)) = device.receive_message().await {
                                debug!("Legacy summary: {}", summary);
                            }
                        }
                    }

                    msg = rx.recv() => {
                        if let Some(internal_msg) = msg {
                            debug!("Processing internal message: {}", internal_msg);
                        }
                    }

                    // Cross-platform signal handling for graceful shutdown
                    _ = tokio::signal::ctrl_c() => {
                        info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
                        break;
                    }

                    _ = async {
                        #[cfg(unix)]
                        {
                            sigterm.recv().await
                        }
                        #[cfg(not(unix))]
                        {
                            std::future::pending::<Option<()>>().await
                        }
                    } => {
                        info!("Received SIGTERM, initiating graceful shutdown...");
                        break;
                    }

                    _ = async {
                        #[cfg(unix)]
                        {
                            sighup.recv().await
                        }
                        #[cfg(not(unix))]
                        {
                            std::future::pending::<Option<()>>().await
                        }
                    } => {
                        info!("Received SIGHUP, initiating graceful shutdown...");
                        break;
                    }

                    _ = async {
                        #[cfg(windows)]
                        {
                            ctrl_break.recv().await
                        }
                        #[cfg(not(windows))]
                        {
                            std::future::pending::<Option<()>>().await
                        }
                    } => {
                        info!("Received Ctrl+Break, initiating graceful shutdown...");
                        break;
                    }
                }
            }

            // Flush any queued direct messages (legacy support)
            #[cfg(feature = "meshtastic-proto")]
            if !self.pending_direct.is_empty() {
                let mut still_pending = Vec::new();
                for (dest, channel, msg) in self.pending_direct.drain(..) {
                    if let Some(scheduler) = &self.scheduler {
                        let outgoing = OutgoingMessage {
                            to_node: Some(dest),
                            channel,
                            content: msg.clone(),
                            priority: MessagePriority::High,
                            kind: crate::meshtastic::OutgoingKind::Normal,
                            request_ack: false,
                        };
                        let env = crate::bbs::dispatch::MessageEnvelope::new(
                            crate::bbs::dispatch::MessageCategory::Direct,
                            crate::bbs::dispatch::Priority::High,
                            Duration::from_millis(0),
                            outgoing,
                        );
                        scheduler.enqueue(env);
                        debug!("Flushed pending DM to {dest} via scheduler on channel {channel}");
                    } else if let Some(ref tx) = self.outgoing_tx {
                        let outgoing = OutgoingMessage {
                            to_node: Some(dest),
                            channel,
                            content: msg.clone(),
                            priority: MessagePriority::High,
                            kind: crate::meshtastic::OutgoingKind::Normal,
                            request_ack: false,
                        };
                        if tx.send(outgoing).is_err() {
                            warn!("Failed to send pending DM to {dest} on channel {channel}");
                            still_pending.push((dest, channel, msg));
                        }
                    } else {
                        still_pending.push((dest, channel, msg));
                    }
                }
                self.pending_direct = still_pending;
            }
        }

        self.shutdown().await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn test_get_session(&self, node: &str) -> Option<&Session> {
        self.sessions.get(node)
    }

    fn logged_in_session_count(&self) -> usize {
        self.sessions.values().filter(|s| s.is_logged_in()).count()
    }

    async fn prune_idle_sessions(&mut self) {
        let timeout_min = self.config.bbs.session_timeout as i64;
        if timeout_min == 0 {
            return;
        }
        let mut to_logout = Vec::new();
        for (k, s) in &self.sessions {
            if s.is_logged_in() && s.is_inactive(timeout_min) {
                to_logout.push(k.clone());
            }
        }
        for k in to_logout {
            // Capture username without holding mutable borrow over await
            let username = if let Some(s) = self.sessions.get(&k) {
                s.display_name()
            } else {
                continue;
            };
            let _ = self
                .send_message(&k, "You have been logged out due to inactivity.")
                .await;
            if let Some(s) = self.sessions.get_mut(&k) {
                let _ = s.logout().await;
            }
            info!(
                "Session {} (user {}) logged out due to inactivity",
                k, username
            );
        }
    }

    #[allow(dead_code)]
    pub fn test_logged_in_count(&self) -> usize {
        self.logged_in_session_count()
    }
    #[allow(dead_code)]
    pub async fn test_prune_idle(&mut self) {
        self.prune_idle_sessions().await;
    }

    /// Get list of all active sessions for administrative commands
    pub fn get_active_sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }

    /// Get list of currently logged-in users for WHO command
    pub fn get_logged_in_users(&self) -> Vec<&Session> {
        self.sessions
            .values()
            .filter(|s| s.is_logged_in())
            .collect()
    }

    /// Force logout a specific user (KICK command)
    pub async fn force_logout_user(&mut self, username: &str) -> Result<bool> {
        let mut target_node = None;

        // Find the session for this username
        for (node_id, session) in &self.sessions {
            if session.username.as_deref() == Some(username) && session.is_logged_in() {
                target_node = Some(node_id.clone());
                break;
            }
        }

        if let Some(node_id) = target_node {
            let _ = self
                .send_message(&node_id, "You have been disconnected by an administrator.")
                .await;
            if let Some(session) = self.sessions.get_mut(&node_id) {
                let _ = session.logout().await;
            }
            info!("User {} forcibly logged out by administrator", username);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Send broadcast message to all logged-in users
    pub async fn broadcast_message(&mut self, message: &str, sender: &str) -> Result<usize> {
        let mut sent_count = 0;
        let broadcast_msg = format!("*** SYSTEM MESSAGE from {}: {} ***", sender, message);

        let logged_in_nodes: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.is_logged_in())
            .map(|(node_id, _)| node_id.clone())
            .collect();

        for node_id in logged_in_nodes {
            if let Err(e) = self.send_message(&node_id, &broadcast_msg).await {
                log::warn!("Failed to send broadcast to {}: {}", node_id, e);
            } else {
                sent_count += 1;
            }
        }

        info!(
            "Broadcast message sent to {} users by {}",
            sent_count, sender
        );
        Ok(sent_count)
    }
    // test helpers declared earlier
    /// Format the unread summary line according to spec.
    /// When unread == 0 -> "There are no new messages.\n"
    /// When unread == 1 -> "1 new message since your last login.\n"
    /// When unread > 1 -> "<n> new messages since your last login.\n"
    fn format_unread_line(unread: u32) -> String {
        match unread {
            0 => "There are no new messages.\n".to_string(),
            1 => "1 new message since your last login.\n".to_string(),
            n => format!("{} new messages since your last login.\n", n),
        }
    }

    fn format_main_menu(apps_config: &crate::config::AppsConfig) -> String {
        let mut line = String::from("Main Menu:\n[M]essages ");
        if games::has_enabled_doors(apps_config) {
            line.push_str("[G]ames ");
        }
        line.push_str("[P]references [Q]uit\n");
        line
    }

    fn format_hint_line(
        apps_config: &crate::config::AppsConfig,
        include_topic_digits: bool,
    ) -> String {
        let mut hint = String::from("Hint: M=messages ");
        if include_topic_digits {
            hint.push_str("1-9=select ");
        }
        if games::has_enabled_doors(apps_config) {
            hint.push_str("G=games ");
        }
        hint.push_str("H=help\n");
        hint
    }

    /// Ensure sysop user exists / synchronized with config (extracted for testability)
    pub async fn seed_sysop(&mut self) -> Result<()> {
        if let Some(hash) = &self.config.bbs.sysop_password_hash {
            let sysop_name = self.config.bbs.sysop.clone();
            let _ = self
                .storage
                .upsert_user_with_hash(&sysop_name, hash, 10)
                .await?;
            info!("Sysop user '{}' ensured from config.", sysop_name);
        }
        Ok(())
    }

    /// Merge TOML topic configurations into runtime storage (legacy/backwards compatibility)
    async fn merge_toml_topics_to_runtime(storage: &mut Storage, config: &Config) -> Result<()> {
        for (topic_id, topic_config) in &config.message_topics {
            // Only create topics that don't already exist in runtime config
            if !storage.topic_exists(topic_id) {
                storage
                    .create_topic(
                        topic_id,
                        &topic_config.name,
                        &topic_config.description,
                        topic_config.read_level,
                        topic_config.post_level,
                        "system", // Creator for TOML-migrated topics
                    )
                    .await?;
            }
        }
        Ok(())
    }

    /// Test/helper accessor: fetch user record
    #[allow(dead_code)]
    pub async fn get_user(&self, username: &str) -> Result<Option<crate::storage::User>> {
        self.storage.get_user(username).await
    }

    // Test & moderation helpers (public so integration tests can invoke)
    #[allow(dead_code)]
    pub async fn test_register(&mut self, username: &str, pass: &str) -> Result<()> {
        self.storage.register_user(username, pass, None).await
    }
    #[allow(dead_code)]
    pub async fn test_update_level(&mut self, username: &str, lvl: u8) -> Result<()> {
        if username == self.config.bbs.sysop {
            return Err(anyhow::anyhow!("Cannot modify sysop level"));
        }
        self.storage
            .update_user_level(username, lvl, "test")
            .await
            .map(|_| ())
    }
    /// Test helper to create a TinyMUSH player WITH admin level in one atomic operation.
    ///
    /// This helper must be called BEFORE entering TinyMUSH with "G1". It creates the player
    /// record directly in the database with admin privileges, avoiding the Sled caching issue
    /// where different handles see stale data.
    ///
    /// # Arguments
    /// * `node_key` - The session node key
    /// * `level` - Admin level (1=Moderator, 2=Admin, 3=Sysop)
    ///
    /// # Returns
    /// * `Ok(())` if player was created successfully
    /// * `Err` if the session wasn't found or creation failed
    #[allow(dead_code)]
    pub async fn test_tmush_create_player_with_admin(
        &mut self,
        node_key: &str,
        level: u8,
    ) -> Result<()> {
        use crate::tmush::types::PlayerRecord;

        let session = self
            .sessions
            .get(node_key)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", node_key))?;
        let username = session.display_name();

        let store = self
            .game_registry
            .get_tinymush_store()
            .ok_or_else(|| anyhow::anyhow!("TinyMUSH store not available in game registry"))?;

        // Create player with admin level in one atomic operation using transaction
        let result = store.primary_tree().transaction(|tx_tree| {
            let key = format!("players:{}", username.to_ascii_lowercase());

            // Check if player already exists
            if tx_tree.get(key.as_bytes())?.is_some() {
                return Err(sled::transaction::ConflictableTransactionError::Abort(
                    anyhow::anyhow!("Player {} already exists", username),
                ));
            }

            // Create new player with admin privileges
            let mut player = PlayerRecord::new(
                &username,
                &username,
                crate::tmush::state::REQUIRED_LANDING_LOCATION_ID,
            );
            player.grant_admin(level);
            player.touch();

            // Serialize and insert
            let player_bytes = bincode::serialize(&player).map_err(|e| {
                sled::transaction::ConflictableTransactionError::Abort(anyhow::anyhow!(
                    "Failed to serialize player: {}",
                    e
                ))
            })?;

            tx_tree.insert(key.as_bytes(), player_bytes.as_slice())?;

            Ok(())
        });

        result.map_err(|e| anyhow::anyhow!("Transaction failed: {:?}", e))?;

        // Flush the entire database to ensure cross-handle visibility
        store.db().flush()?;

        // Ensure landing instance exists and update player record if needed
        let landing_id = store
            .ensure_personal_landing_room(&username)
            .map_err(|e| anyhow::anyhow!("Failed to provision landing gazebo: {}", e))?;

        let mut verify_player = store
            .get_player(&username)
            .map_err(|e| anyhow::anyhow!("Failed to verify player creation: {}", e))?;
        if verify_player.current_room != landing_id {
            verify_player.current_room = landing_id;
            store
                .put_player(verify_player.clone())
                .map_err(|e| anyhow::anyhow!("Failed to update player landing room: {}", e))?;
        }

        if !verify_player.is_admin() {
            return Err(anyhow::anyhow!(
                "Player created but admin flag not set! Level={:?}",
                verify_player.admin_level
            ));
        }
        eprintln!(
            "✓ Verified player {} has admin level {:?}",
            username, verify_player.admin_level
        );

        Ok(())
    }

    /// Test helper to grant admin privileges using Sled transaction for atomicity.
    ///
    /// This uses a transaction to ensure the write is immediately visible to all
    /// other handles/processes reading the same database, avoiding cache issues.
    #[allow(dead_code)]
    pub async fn test_tmush_grant_admin(&mut self, username: &str, level: u8) -> Result<()> {
        use crate::tmush::types::PlayerRecord;

        // Use the shared store from game_registry instead of opening a new connection
        let store = self
            .game_registry
            .get_tinymush_store()
            .ok_or_else(|| anyhow::anyhow!("TinyMUSH store not available in game registry"))?;

        // Use Tree transaction for atomic read-modify-write
        let result = store.primary_tree().transaction(|tx_tree| {
            // Construct the key (same as TinyMushStore::players_key)
            let key = format!("players:{}", username.to_ascii_lowercase());

            // Read current player record
            let player_bytes = tx_tree.get(key.as_bytes())?.ok_or(
                sled::transaction::ConflictableTransactionError::Abort(anyhow::anyhow!(
                    "Player not found: {}",
                    username
                )),
            )?;

            // Deserialize
            let mut player: PlayerRecord = bincode::deserialize(&player_bytes).map_err(|e| {
                sled::transaction::ConflictableTransactionError::Abort(anyhow::anyhow!(
                    "Failed to deserialize player: {}",
                    e
                ))
            })?;

            // Modify
            player.grant_admin(level);
            player.touch();

            // Serialize
            let new_bytes = bincode::serialize(&player).map_err(|e| {
                sled::transaction::ConflictableTransactionError::Abort(anyhow::anyhow!(
                    "Failed to serialize player: {}",
                    e
                ))
            })?;

            // Write back atomically
            tx_tree.insert(key.as_bytes(), new_bytes.as_slice())?;

            Ok(())
        });

        result.map_err(|e| anyhow::anyhow!("Transaction failed: {:?}", e))?;

        // Flush the entire database to ensure cross-handle visibility
        store.db().flush()?;

        Ok(())
    }

    /// Test helper to create a TinyMUSH player WITH admin level in one operation.
    /// This avoids Sled caching issues that occur when creating and then modifying a player.
    #[allow(dead_code)]
    pub async fn test_tmush_create_admin_player(
        &mut self,
        node_key: &str,
        level: u8,
    ) -> Result<()> {
        use crate::tmush::types::PlayerRecord;

        let session = self
            .sessions
            .get(node_key)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", node_key))?;
        let username = session.display_name();

        let store = self
            .game_registry
            .get_tinymush_store()
            .ok_or_else(|| anyhow::anyhow!("TinyMUSH store not available in game registry"))?;

        // Check if player already exists
        if store.get_player(&username).is_ok() {
            return Err(anyhow::anyhow!("Player {} already exists", username));
        }

        // Create player with admin level in one operation
        let mut player = PlayerRecord::new(
            &username,
            &username,
            crate::tmush::state::REQUIRED_LANDING_LOCATION_ID,
        );
        player.grant_admin(level);

        store
            .put_player(player)
            .map_err(|e| anyhow::anyhow!("Failed to create admin player: {}", e))?;

        let landing_id = store
            .ensure_personal_landing_room(&username)
            .map_err(|e| anyhow::anyhow!("Failed to provision landing gazebo: {}", e))?;
        let mut verify_player = store
            .get_player(&username)
            .map_err(|e| anyhow::anyhow!("Failed to verify admin player creation: {}", e))?;
        if verify_player.current_room != landing_id {
            verify_player.current_room = landing_id;
            store
                .put_player(verify_player)
                .map_err(|e| anyhow::anyhow!("Failed to update player landing room: {}", e))?;
        }

        Ok(())
    }
    /// Test helper to force TinyMUSH player record creation.
    ///
    /// Call this IMMEDIATELY after route_test_text_direct("G2") to directly
    /// create a TinyMUSH player record. This bypasses lazy initialization
    /// so subsequent operations (like test_tmush_grant_admin) can find it.
    ///
    /// # Arguments
    /// * `node_key` - The session node key
    ///
    /// # Returns
    /// * `Ok(())` if player record was created successfully
    /// * `Err` if the session wasn't found or creation failed
    #[allow(dead_code)]
    pub async fn test_tmush_ensure_player_exists(&mut self, node_key: &str) -> Result<()> {
        use crate::tmush::types::PlayerRecord;

        // Get the username from the session
        let session = self
            .sessions
            .get(node_key)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", node_key))?;
        let username = session.display_name();

        // Get TinyMUSH store from game registry (shared cache)
        let store = self
            .game_registry
            .get_tinymush_store()
            .ok_or_else(|| anyhow::anyhow!("TinyMUSH store not available in game registry"))?;

        // Check if player already exists
        if store.get_player(&username).is_ok() {
            return Ok(()); // Player already exists, nothing to do
        }

        // Create new player at landing location (gazebo)
        let player = PlayerRecord::new(
            &username,
            &username, // Use username as display name
            crate::tmush::state::REQUIRED_LANDING_LOCATION_ID,
        );

        store
            .put_player(player)
            .map_err(|e| anyhow::anyhow!("Failed to create player: {}", e))?;

        let landing_id = store
            .ensure_personal_landing_room(&username)
            .map_err(|e| anyhow::anyhow!("Failed to provision landing gazebo: {}", e))?;
        let mut verify_player = store
            .get_player(&username)
            .map_err(|e| anyhow::anyhow!("Failed to verify player creation: {}", e))?;
        if verify_player.current_room != landing_id {
            verify_player.current_room = landing_id;
            store
                .put_player(verify_player)
                .map_err(|e| anyhow::anyhow!("Failed to update player landing room: {}", e))?;
        }

        Ok(())
    }
    #[allow(dead_code)]
    pub async fn test_create_topic(
        &mut self,
        topic_id: &str,
        name: &str,
        description: &str,
        read_level: u8,
        post_level: u8,
        creator: &str,
    ) -> Result<()> {
        if self.storage.topic_exists(topic_id) {
            return Ok(());
        }
        self.storage
            .create_topic(topic_id, name, description, read_level, post_level, creator)
            .await
    }
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub async fn test_create_subtopic(
        &mut self,
        topic_id: &str,
        parent_id: &str,
        name: &str,
        description: &str,
        read_level: u8,
        post_level: u8,
        creator: &str,
    ) -> Result<()> {
        if self.storage.topic_exists(topic_id) {
            return Ok(());
        }
        self.storage
            .create_subtopic(
                topic_id,
                parent_id,
                name,
                description,
                read_level,
                post_level,
                creator,
            )
            .await
    }
    #[allow(dead_code)]
    pub async fn test_store_message(
        &mut self,
        topic: &str,
        author: &str,
        content: &str,
    ) -> Result<String> {
        self.storage.store_message(topic, author, content).await
    }
    #[allow(dead_code)]
    pub async fn test_get_messages(
        &self,
        topic: &str,
        limit: usize,
    ) -> Result<Vec<crate::storage::Message>> {
        self.storage.get_messages(topic, limit).await
    }
    #[allow(dead_code)]
    pub async fn test_list_topics(&self) -> Result<Vec<String>> {
        self.storage.list_message_topics().await
    }
    #[allow(dead_code)]
    pub fn test_is_locked(&self, topic: &str) -> bool {
        self.storage.is_topic_locked(topic)
    }
    #[allow(dead_code)]
    pub async fn test_deletion_page(
        &self,
        page: usize,
        size: usize,
    ) -> Result<Vec<crate::storage::DeletionAuditEntry>> {
        self.storage.get_deletion_audit_page(page, size).await
    }
    // (duplicate definition removed; consolidated above)

    // Moderator / sysop internal helpers
    pub async fn moderator_delete_message(
        &mut self,
        topic: &str,
        id: &str,
        actor: &str,
    ) -> Result<bool> {
        let deleted = self.storage.delete_message(topic, id).await?;
        if deleted {
            sec_log!("DELETE by {}: {}/{}", actor, topic, id);
            // Fire and forget audit append; if it fails, surface as error to caller
            self.storage.append_deletion_audit(topic, id, actor).await?;
        }
        Ok(deleted)
    }
    pub async fn moderator_lock_topic(&mut self, topic: &str, actor: &str) -> Result<()> {
        self.storage.lock_topic_persist(topic).await?;
        sec_log!("LOCK by {}: {}", actor, topic);
        Ok(())
    }
    pub async fn moderator_unlock_topic(&mut self, topic: &str, actor: &str) -> Result<()> {
        self.storage.unlock_topic_persist(topic).await?;
        sec_log!("UNLOCK by {}: {}", actor, topic);
        Ok(())
    }

    #[allow(dead_code)]
    async fn process_help_text(
        &mut self,
        session: &mut crate::bbs::session::Session,
        storage: &mut crate::storage::Storage,
        config: &crate::config::Config,
    ) -> Result<String> {
        // Process help text via command processor and append one-time shortcuts hint
        let mut help_text = session
            .process_command("H", storage, config, &self.game_registry)
            .await?;
        if !session.help_seen {
            session.help_seen = true;
            help_text.push_str("Shortcuts: M=areas U=user Q=quit\n");
        }
        Ok(help_text)
    }

    #[cfg(feature = "meshtastic-proto")]
    async fn handle_node_detection(
        &mut self,
        event: crate::meshtastic::NodeDetectionEvent,
    ) -> Result<()> {
        use crate::bbs::welcome;

        // Check if welcome system is enabled
        if !self.config.apps.welcome.enabled {
            return Ok(());
        }
        // Return early if welcome system not configured
        if self.welcome_state.is_none() {
            return Ok(());
        }

        // Check if should welcome this node (immutable borrow)
        // Skip rate limit for startup queue items
        let should_welcome = {
            let state = self.welcome_state.as_ref().unwrap();
            state.should_welcome(
                event.node_id,
                &event.long_name,
                &self.config.apps.welcome,
                event.is_from_startup_queue,
            )
        };

        if !should_welcome {
            return Ok(());
        }

        info!(
            "Welcome system triggered for node 0x{:08X} ({})",
            event.node_id, event.long_name
        );

        // Get values we need from self before any mut borrows
        let primary_channel = self.primary_channel();

        // Generate fun call sign suggestion with emoji
        let (suggested_callsign, emoji) = welcome::generate_callsign();

        // Strategy: PING the node first to verify reachability
        // Only send welcome messages if ping succeeds

        // Send ping to node
        let ping_success = if let Some(writer_tx) = &self.writer_control_tx {
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let ping_msg = crate::meshtastic::ControlMessage::SendPing {
                to: event.node_id,
                channel: primary_channel,
                response_tx,
            };

            if writer_tx.send(ping_msg).is_ok() {
                // Wait for ping response with timeout (2 minutes for slow mesh routing)
                match tokio::time::timeout(std::time::Duration::from_secs(120), response_rx).await {
                    Ok(Ok(true)) => {
                        debug!("Ping successful for node 0x{:08X}", event.node_id);
                        true
                    }
                    Ok(Ok(false)) => {
                        info!(
                            "Ping failed for node 0x{:08X}, skipping welcome",
                            event.node_id
                        );
                        false
                    }
                    Ok(Err(_)) => {
                        warn!(
                            "Ping response channel closed for node 0x{:08X}",
                            event.node_id
                        );
                        false
                    }
                    Err(_) => {
                        warn!(
                            "Ping timeout for node 0x{:08X}, skipping welcome",
                            event.node_id
                        );
                        false
                    }
                }
            } else {
                warn!(
                    "Failed to send ping command for node 0x{:08X}",
                    event.node_id
                );
                false
            }
        } else {
            warn!("No writer control channel available for ping");
            false
        };

        // Only proceed if ping succeeded
        if !ping_success {
            return Ok(());
        }

        // Only proceed if ping succeeded
        if !ping_success {
            return Ok(());
        }

        // Node is reachable! Send welcome messages
        // Send private guide if enabled
        if self.config.apps.welcome.private_guide {
            let cmd_prefix = self.public_parser.primary_prefix_char();
            let help_cmd = self.public_parser.help_command();
            let guide = welcome::private_guide(
                &event.long_name,
                &suggested_callsign,
                &emoji,
                cmd_prefix,
                help_cmd,
            );

            // Send via scheduler as a reliable DM
            if let Some(scheduler) = &self.scheduler {
                use crate::bbs::dispatch::{MessageCategory, MessageEnvelope, Priority};
                use crate::meshtastic::{MessagePriority, OutgoingKind, OutgoingMessage};

                // Chunk the guide if needed - use 200 byte limit to leave room for protocol overhead
                let chunks = self.chunk_utf8(&guide, 200);
                for (idx, chunk) in chunks.iter().enumerate() {
                    // Add 5-second delay between chunks to avoid rate limiting
                    let delay = std::time::Duration::from_secs(idx as u64 * 5);
                    let msg = OutgoingMessage {
                        to_node: Some(event.node_id),
                        channel: primary_channel,
                        content: chunk.clone(),
                        priority: MessagePriority::Normal,
                        kind: OutgoingKind::Normal,
                        request_ack: true,
                    };
                    let envelope =
                        MessageEnvelope::new(MessageCategory::System, Priority::Normal, delay, msg);
                    scheduler.enqueue(envelope);
                }
            }
        }

        // Send public greeting if enabled
        // Delay it to allow private guide to complete first and avoid rate limiting
        if self.config.apps.welcome.public_greeting {
            let greeting = welcome::public_greeting(&event.long_name);
            if let Some(scheduler) = &self.scheduler {
                use crate::bbs::dispatch::{MessageCategory, MessageEnvelope, Priority};
                use crate::meshtastic::{MessagePriority, OutgoingKind, OutgoingMessage};

                // If private_guide is enabled, delay public greeting to avoid rate limit conflicts
                // With 2 chunks at 5-second spacing, wait 11 seconds (last chunk at 5s + 6s buffer)
                let delay_secs = if self.config.apps.welcome.private_guide {
                    11
                } else {
                    0
                };

                // Chunk public greeting too
                let chunks = self.chunk_utf8(&greeting, 200);
                for (idx, chunk) in chunks.iter().enumerate() {
                    let chunk_delay = delay_secs + (idx as u64 * 5);
                    let msg = OutgoingMessage {
                        to_node: None, // broadcast
                        channel: primary_channel,
                        content: chunk.clone(),
                        priority: MessagePriority::Normal,
                        kind: OutgoingKind::Normal,
                        request_ack: false,
                    };
                    let envelope = MessageEnvelope::new(
                        MessageCategory::System,
                        Priority::Normal,
                        std::time::Duration::from_secs(chunk_delay),
                        msg,
                    );
                    scheduler.enqueue(envelope);
                }
            }
        }

        // Record that we welcomed this node
        if let Some(welcome_state) = &mut self.welcome_state {
            welcome_state.record_welcome(event.node_id, &event.long_name);
        }

        Ok(())
    }

    #[cfg(feature = "meshtastic-proto")]
    #[cfg_attr(test, allow(dead_code))]
    #[allow(clippy::ifs_same_cond)] // Help condition legitimately appears in both DM and broadcast paths
    pub async fn route_text_event(&mut self, ev: TextEvent) -> Result<()> {
        // Trace-log every text event for debugging purposes
        trace!(
            "TextEvent BEGIN src={} direct={} channel={:?} content='{}'",
            ev.source,
            ev.is_direct,
            ev.channel,
            ev.content
        );
        // Source node id string form
        let node_key = ev.source.to_string();
        if ev.is_direct {
            // Direct (private) path: ensure session exists, finalize pending login if any
            if !self.sessions.contains_key(&node_key) {
                trace!("Creating new session for direct node {}", node_key);
                let mut session = Session::new(node_key.clone(), node_key.clone());
                // Pending public login auto-apply path
                if let Some(username) = self.public_state.take_pending(&node_key) {
                    let current = self.logged_in_session_count();
                    if (current as u32) >= self.config.bbs.max_users {
                        let _ = self.send_message(&node_key, "All available sessions are in use, please wait and try again later.").await;
                    } else {
                        // Security check: verify if user has a password set
                        if let Ok(Some(user)) = self.storage.get_user(&username).await {
                            if user.password_hash.is_some() {
                                // User has a password - require proper authentication
                                trace!("User '{}' has password, requiring authentication via DM for node {}", username, node_key);
                                let _ = self.send_message(&node_key, &format!("Welcome! To complete login as '{}', please enter: LOGIN {} <password>", username, username)).await;
                                // Put the pending login back so they can complete it with password
                                self.public_state.set_pending(&node_key, username);
                            } else {
                                // User has no password - allow auto-login for backward compatibility
                                trace!("Auto-applying pending public login '{}' (no password) to new DM session {}", username, node_key);
                                session.login(username.clone(), 1).await?;
                                let prev_last = user.last_login;
                                if let Some(sess) = self.sessions.get_mut(&node_key) {
                                    sess.unread_since = Some(prev_last);
                                }
                                let unread = self
                                    .storage
                                    .count_messages_since(prev_last)
                                    .await
                                    .unwrap_or(0);
                                let _ = self.storage.record_user_login(&username).await; // update last_login
                                let summary = Self::format_unread_line(unread);
                                let hint = if unread == 0 {
                                    Self::format_hint_line(&self.config.apps, false)
                                } else {
                                    String::new()
                                };
                                let menu = Self::format_main_menu(&self.config.apps);
                                let _ = self
                                    .send_session_message(
                                        &node_key,
                                        &format!(
                                            "Welcome, {} you are now logged in.\n{}{}{}",
                                            username, summary, hint, menu
                                        ),
                                        true,
                                    )
                                    .await;
                            }
                        } else {
                            // New user case - create user without password (they can set one later)
                            trace!("Auto-applying pending public login '{}' (new user) to new DM session {}", username, node_key);
                            session.login(username.clone(), 1).await?;
                            if let Some(sess) = self.sessions.get_mut(&node_key) {
                                sess.unread_since = Some(Utc::now());
                            }
                            self.storage
                                .create_or_update_user(&username, &node_key)
                                .await?;
                            let summary = Self::format_unread_line(0);
                            let hint = Self::format_hint_line(&self.config.apps, false);
                            let menu = Self::format_main_menu(&self.config.apps);
                            let _ = self
                                .send_session_message(
                                    &node_key,
                                    &format!(
                                        "Welcome, {} you are now logged in.\n{}{}{}",
                                        username, summary, hint, menu
                                    ),
                                    true,
                                )
                                .await;
                        }
                    }
                } else {
                    // Removed first-contact guidance banner (Option B) to avoid duplicate initial messages.
                }
                self.sessions.insert(node_key.clone(), session);
            }
            // New consolidated DM command handling with max_users and idle pruning
            self.prune_idle_sessions().await; // always prune first
            let raw_content = ev.content.trim().to_string();
            let upper = raw_content.to_uppercase();
            // Count current logged in sessions (excluding the session for this node if it is not yet logged in)
            let logged_in_count = self.sessions.values().filter(|s| s.is_logged_in()).count();
            enum PostAction {
                None,
                Delete {
                    area: String,
                    id: String,
                    actor: String,
                },
                Lock {
                    area: String,
                    actor: String,
                },
                Unlock {
                    area: String,
                    actor: String,
                },
                Broadcast {
                    message: String,
                    sender: String,
                },
            }
            let mut post_action = PostAction::None;
            let mut deferred_reply: Option<String> = None;
            let mut deferred_chunks: Option<Vec<String>> = None; // For multi-part messages like HELP+

            // Track if this message was fully handled by registration logic to avoid re-processing.
            let mut handled_registration = false;
            // Handle REGISTER early without holding mutable borrow on session to simplify chunking logic.
            if upper.starts_with("REGISTER ") {
                let parts: Vec<&str> = raw_content.split_whitespace().collect();
                if parts.len() < 3 {
                    deferred_reply = Some("Usage: REGISTER <username> <password>\n".into());
                } else {
                    let user = parts[1];
                    let pass = parts[2];
                    if pass.len() < 8 {
                        deferred_reply =
                            Some("Password too short (minimum 8 characters).\n".into());
                    } else {
                        match self
                            .storage
                            .register_user(user, pass, Some(&node_key))
                            .await
                        {
                            Ok(_) => {
                                if let Some(session) = self.sessions.get_mut(&node_key) {
                                    session.login(user.to_string(), 1).await?;
                                    session.unread_since = Some(Utc::now());
                                }
                                let summary = Self::format_unread_line(0);
                                let hint = Self::format_hint_line(&self.config.apps, true);
                                let menu = Self::format_main_menu(&self.config.apps);
                                let full_welcome = format!(
                                    "Registered as {u}.\n{summary}{hint}{menu}",
                                    u = user,
                                    summary = summary,
                                    hint = hint,
                                    menu = menu
                                );
                                let max_bytes = self.config.storage.max_message_size;
                                let parts_vec = self.chunk_utf8(&full_welcome, max_bytes);
                                if parts_vec.len() > 1 {
                                    warn!("Registration welcome chunked into {} parts ({} bytes total)", parts_vec.len(), full_welcome.len());
                                }
                                if parts_vec.len() == 1 {
                                    deferred_reply = Some(parts_vec[0].clone());
                                } else {
                                    let total = parts_vec.len();
                                    for (i, chunk) in parts_vec.into_iter().enumerate() {
                                        let last = i + 1 == total; // only append prompt after final
                                        self.send_session_message(&node_key, &chunk, last).await?;
                                    }
                                }
                                if let Err(e) =
                                    self.storage.mark_welcome_shown(user, true, false).await
                                {
                                    eprintln!("Failed to mark welcome shown for {}: {}", user, e);
                                }
                                handled_registration = true;
                            }
                            Err(e) => {
                                deferred_reply = Some(format!("Register failed: {}\n", e));
                            }
                        }
                    }
                }
            }

            // If registration fully handled (multi-part or single reply prepared), skip further command processing
            if handled_registration {
                if let Some(msg) = deferred_reply {
                    self.send_session_message(&node_key, &msg, true).await?;
                }
                return Ok(());
            }

            if let Some(session) = self.sessions.get_mut(&node_key) {
                session.update_activity();
                #[cfg(feature = "meshtastic-proto")]
                if let (Some(dev), Ok(idnum)) = (&self.device, node_key.parse::<u32>()) {
                    let (short, long) = dev.format_node_combined(idnum);
                    session.update_labels(Some(short), Some(long));
                }
                if upper == "HELP+" || upper == "HELP V" || upper == "HELP  V" || upper == "HELP  +"
                {
                    // tolerate minor spacing variants - store chunks to send after borrow ends
                    let chunks = chunk_verbose_help_with_prefix(
                        self.public_parser.primary_prefix_char(),
                        self.public_parser.help_command(),
                    );
                    deferred_chunks = Some(chunks);
                } else if upper == "H"
                    || (upper == "?" && session.state != super::session::SessionState::TinyHack)
                {
                    // Process help command and send response
                    let mut help_text = session
                        .process_command("H", &mut self.storage, &self.config, &self.game_registry)
                        .await?;
                    if !session.help_seen {
                        session.help_seen = true;
                        help_text.push_str("Shortcuts: M=areas U=user Q=quit\n");
                    }
                    // Store response to send after borrow ends
                    deferred_reply = Some(help_text);
                } else if upper.starts_with("LOGIN ") {
                    // Enforce max_users only if this session is not yet logged in
                    if !session.is_logged_in()
                        && (logged_in_count as u32) >= self.config.bbs.max_users
                    {
                        deferred_reply = Some(
                            "All available sessions are in use, please wait and try again later.\n"
                                .into(),
                        );
                    } else if session.is_logged_in() {
                        deferred_reply = Some(format!(
                            "Already logged in as {}.\n",
                            session.display_name()
                        ));
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: LOGIN <username> [password]\n".into());
                        } else {
                            let user = parts[1];
                            let password_opt = if parts.len() >= 3 {
                                Some(parts[2])
                            } else {
                                None
                            };
                            match self.storage.get_user(user).await? {
                                None => {
                                    deferred_reply =
                                        Some("No such user. Use REGISTER <u> <p>.\n".into())
                                }
                                Some(u) => {
                                    let has_password = u.password_hash.is_some();
                                    let node_bound = u.node_id.as_deref() == Some(&node_key);
                                    if !has_password {
                                        // User must set a password on first login attempt
                                        if let Some(pass) = password_opt {
                                            if pass.len() < 8 {
                                                deferred_reply = Some(
                                                    "Password too short (minimum 8 characters).\n"
                                                        .into(),
                                                );
                                            } else {
                                                let updated_user = self
                                                    .storage
                                                    .set_user_password(user, pass)
                                                    .await?;
                                                let updated = if !node_bound {
                                                    self.storage
                                                        .bind_user_node(user, &node_key)
                                                        .await?
                                                } else {
                                                    updated_user
                                                };
                                                session
                                                    .login(
                                                        updated.username.clone(),
                                                        updated.user_level,
                                                    )
                                                    .await?;
                                                if let Some(sess) = self.sessions.get_mut(&node_key)
                                                {
                                                    sess.unread_since = Some(Utc::now());
                                                }
                                                // First-time password set; unread messages prior to this first authenticated login are based on prior last_login value.
                                                // set_user_password already bumped last_login, so computing unread would yield zero. This is acceptable; show none.
                                                let _ = self.storage.record_user_login(user).await; // ensure fresh timestamp after full login
                                                                                                    // No unread count expected here (legacy first login)
                                                let summary = Self::format_unread_line(0); // first login after setting password shows no unread

                                                // Check if this is the first login after registration and show follow-up welcome
                                                let hint = Self::format_hint_line(
                                                    &self.config.apps,
                                                    false,
                                                );
                                                let menu =
                                                    Self::format_main_menu(&self.config.apps);
                                                let login_msg = format!("Password set. Welcome, {} you are now logged in.\n{}{}{}", updated.username, summary, hint, menu);
                                                if updated.welcome_shown_on_registration
                                                    && !updated.welcome_shown_on_first_login
                                                {
                                                    // Mark first login welcome as shown
                                                    if let Err(e) = self
                                                        .storage
                                                        .mark_welcome_shown(user, false, true)
                                                        .await
                                                    {
                                                        eprintln!("Failed to mark first login welcome shown for {}: {}", user, e);
                                                    }
                                                }
                                                deferred_reply = Some(login_msg);
                                            }
                                        } else {
                                            deferred_reply = Some("Password not set. LOGIN <user> <newpass> to set your password.\n".into());
                                        }
                                    } else {
                                        // Has password: require it if not bound or if password provided
                                        if let Some(pass) = password_opt {
                                            let (_maybe, ok) = self
                                                .storage
                                                .verify_user_password(user, pass)
                                                .await?;
                                            if !ok {
                                                deferred_reply = Some("Invalid password.\n".into());
                                            } else {
                                                let updated = if !node_bound {
                                                    self.storage
                                                        .bind_user_node(user, &node_key)
                                                        .await?
                                                } else {
                                                    u
                                                };
                                                session
                                                    .login(
                                                        updated.username.clone(),
                                                        updated.user_level,
                                                    )
                                                    .await?;
                                                let prev_last = updated.last_login; // captured before we update last_login again
                                                if let Some(sess) = self.sessions.get_mut(&node_key)
                                                {
                                                    sess.unread_since = Some(prev_last);
                                                }
                                                let unread = self
                                                    .storage
                                                    .count_messages_since(prev_last)
                                                    .await
                                                    .unwrap_or(0);
                                                let updated2 = self
                                                    .storage
                                                    .record_user_login(user)
                                                    .await
                                                    .unwrap_or(updated);
                                                let summary = Self::format_unread_line(unread);

                                                // Check if this is the first login after registration and show follow-up welcome
                                                let hint = if unread == 0 {
                                                    Self::format_hint_line(&self.config.apps, false)
                                                } else {
                                                    String::new()
                                                };
                                                let menu =
                                                    Self::format_main_menu(&self.config.apps);
                                                let login_msg = format!(
                                                    "Welcome, {} you are now logged in.\n{}{}{}",
                                                    updated2.username, summary, hint, menu
                                                );
                                                if updated2.welcome_shown_on_registration
                                                    && !updated2.welcome_shown_on_first_login
                                                {
                                                    // Mark first login welcome as shown
                                                    if let Err(e) = self
                                                        .storage
                                                        .mark_welcome_shown(user, false, true)
                                                        .await
                                                    {
                                                        eprintln!("Failed to mark first login welcome shown for {}: {}", user, e);
                                                    }
                                                }
                                                deferred_reply = Some(login_msg);
                                            }
                                        } else {
                                            deferred_reply = Some(
                                                "Password required: LOGIN <user> <pass>\n".into(),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if upper.starts_with("CHPASS ") {
                    if session.username.as_deref() == Some(&self.config.bbs.sysop) {
                        deferred_reply = Some(
                            "Sysop password managed externally. Use sysop-passwd CLI.\n".into(),
                        );
                    } else if session.is_logged_in() {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 3 {
                            deferred_reply = Some("Usage: CHPASS <old> <new>\n".into());
                        } else {
                            let old = parts[1];
                            let newp = parts[2];
                            if newp.len() < 8 {
                                deferred_reply = Some("New password too short (min 8).\n".into());
                            } else if newp.len() > 128 {
                                deferred_reply = Some("New password too long.\n".into());
                            } else if let Some(user_name) = &session.username {
                                match self.storage.get_user(user_name).await? {
                                    Some(u) => {
                                        if u.password_hash.is_none() {
                                            deferred_reply = Some(
                                                "No existing password. Use SETPASS <new>.\n".into(),
                                            );
                                        } else {
                                            let (_u2, ok) = self
                                                .storage
                                                .verify_user_password(user_name, old)
                                                .await?;
                                            if !ok {
                                                deferred_reply = Some("Invalid password.\n".into());
                                            } else if old == newp {
                                                deferred_reply =
                                                    Some("New password must differ.\n".into());
                                            } else {
                                                self.storage
                                                    .update_user_password(user_name, newp)
                                                    .await?;
                                                deferred_reply = Some("Password changed.\n".into());
                                            }
                                        }
                                    }
                                    None => deferred_reply = Some("Session user missing.\n".into()),
                                }
                            } else {
                                deferred_reply = Some("Not logged in.\n".into());
                            }
                        }
                    } else {
                        deferred_reply = Some("Not logged in.\n".into());
                    }
                } else if upper.starts_with("SETPASS ") {
                    if session.username.as_deref() == Some(&self.config.bbs.sysop) {
                        deferred_reply = Some(
                            "Sysop password managed externally. Use sysop-passwd CLI.\n".into(),
                        );
                    } else if session.is_logged_in() {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: SETPASS <new>\n".into());
                        } else {
                            let newp = parts[1];
                            if newp.len() < 8 {
                                deferred_reply = Some("New password too short (min 8).\n".into());
                            } else if newp.len() > 128 {
                                deferred_reply = Some("New password too long.\n".into());
                            } else if let Some(user_name) = &session.username {
                                match self.storage.get_user(user_name).await? {
                                    Some(u) => {
                                        if u.password_hash.is_some() {
                                            deferred_reply = Some(
                                                "Password already set. Use CHPASS <old> <new>.\n"
                                                    .into(),
                                            );
                                        } else {
                                            self.storage
                                                .update_user_password(user_name, newp)
                                                .await?;
                                            deferred_reply = Some("Password set.\n".into());
                                        }
                                    }
                                    None => deferred_reply = Some("Session user missing.\n".into()),
                                }
                            } else {
                                deferred_reply = Some("Not logged in.\n".into());
                            }
                        }
                    } else {
                        deferred_reply = Some("Not logged in.\n".into());
                    }
                } else if upper.starts_with("PROMOTE ") {
                    if session.username.as_deref() != Some(&self.config.bbs.sysop) {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: PROMOTE <user>\n".into());
                        } else {
                            let target = parts[1];
                            match self.storage.get_user(target).await? {
                                None => deferred_reply = Some("User not found.\n".into()),
                                Some(u) => {
                                    if u.username == self.config.bbs.sysop {
                                        deferred_reply = Some("Cannot modify sysop.\n".into());
                                    } else if u.user_level >= LEVEL_MODERATOR {
                                        deferred_reply =
                                            Some("Already moderator or higher.\n".into());
                                    } else {
                                        self.storage
                                            .update_user_level(
                                                &u.username,
                                                LEVEL_MODERATOR,
                                                session.username.as_deref().unwrap_or("unknown"),
                                            )
                                            .await?;
                                        deferred_reply = Some(format!(
                                            "{} promoted to {}.\n",
                                            u.username,
                                            role_name(LEVEL_MODERATOR)
                                        ));
                                    }
                                }
                            }
                        }
                    }
                } else if upper.starts_with("DEMOTE ") {
                    if session.username.as_deref() != Some(&self.config.bbs.sysop) {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: DEMOTE <user>\n".into());
                        } else {
                            let target = parts[1];
                            match self.storage.get_user(target).await? {
                                None => deferred_reply = Some("User not found.\n".into()),
                                Some(u) => {
                                    if u.username == self.config.bbs.sysop {
                                        deferred_reply = Some("Cannot modify sysop.\n".into());
                                    } else if u.user_level <= LEVEL_USER {
                                        deferred_reply = Some("Already at base level.\n".into());
                                    } else {
                                        self.storage
                                            .update_user_level(
                                                &u.username,
                                                LEVEL_USER,
                                                session.username.as_deref().unwrap_or("unknown"),
                                            )
                                            .await?;
                                        deferred_reply = Some(format!(
                                            "{} demoted to {}.\n",
                                            u.username,
                                            role_name(LEVEL_USER)
                                        ));
                                    }
                                }
                            }
                        }
                    }
                } else if upper.starts_with("CREATETOPIC ") {
                    if session.username.as_deref() != Some(&self.config.bbs.sysop) {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 4 {
                            deferred_reply = Some("Usage: CREATETOPIC <id> <name> <description> [read_level] [post_level]\n".into());
                        } else {
                            let topic_id = parts[1].to_lowercase();
                            let name = parts[2];
                            let description = parts[3..].join(" ");
                            let read_level = 0u8; // Default read level
                            let post_level = 0u8; // Default post level
                            let creator = session.username.as_deref().unwrap_or("sysop");

                            match self
                                .storage
                                .create_topic(
                                    &topic_id,
                                    name,
                                    &description,
                                    read_level,
                                    post_level,
                                    creator,
                                )
                                .await
                            {
                                Ok(()) => {
                                    deferred_reply = Some(format!(
                                        "Topic '{}' created successfully.\n",
                                        topic_id
                                    ))
                                }
                                Err(e) => {
                                    deferred_reply =
                                        Some(format!("Failed to create topic: {}\n", e))
                                }
                            }
                        }
                    }
                } else if upper.starts_with("MODIFYTOPIC ") {
                    if session.username.as_deref() != Some(&self.config.bbs.sysop) {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 3 {
                            deferred_reply = Some("Usage: MODIFYTOPIC <id> name=<name> | desc=<desc> | read=<level> | post=<level>\n".into());
                        } else {
                            let topic_id = parts[1].to_lowercase();
                            let mut name: Option<&str> = None;
                            let mut description: Option<String> = None;
                            let mut read_level: Option<u8> = None;
                            let mut post_level: Option<u8> = None;

                            // Parse key=value pairs
                            for part in &parts[2..] {
                                if let Some((key, value)) = part.split_once('=') {
                                    match key.to_lowercase().as_str() {
                                        "name" => name = Some(value),
                                        "desc" | "description" => {
                                            description = Some(value.to_string())
                                        }
                                        "read" => read_level = value.parse().ok(),
                                        "post" => post_level = value.parse().ok(),
                                        _ => {}
                                    }
                                }
                            }

                            match self
                                .storage
                                .modify_topic(
                                    &topic_id,
                                    name,
                                    description.as_deref(),
                                    read_level,
                                    post_level,
                                )
                                .await
                            {
                                Ok(()) => {
                                    deferred_reply = Some(format!(
                                        "Topic '{}' modified successfully.\n",
                                        topic_id
                                    ))
                                }
                                Err(e) => {
                                    deferred_reply =
                                        Some(format!("Failed to modify topic: {}\n", e))
                                }
                            }
                        }
                    }
                } else if upper.starts_with("DELETETOPIC ") {
                    if session.username.as_deref() != Some(&self.config.bbs.sysop) {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: DELETETOPIC <id>\n".into());
                        } else {
                            let topic_id = parts[1].to_lowercase();

                            match self.storage.delete_topic(&topic_id).await {
                                Ok(()) => {
                                    deferred_reply = Some(format!(
                                        "Topic '{}' deleted successfully.\n",
                                        topic_id
                                    ))
                                }
                                Err(e) => {
                                    deferred_reply =
                                        Some(format!("Failed to delete topic: {}\n", e))
                                }
                            }
                        }
                    }
                } else if upper.starts_with("DELETE ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 3 {
                            deferred_reply = Some("Usage: DELETE <area> <id>\n".into());
                        } else {
                            let area = parts[1].to_lowercase();
                            let id = parts[2].to_string();
                            let actor = session.username.clone().unwrap_or("?".into());
                            post_action = PostAction::Delete { area, id, actor };
                        }
                    }
                } else if upper.starts_with("LOCK ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: LOCK <area>\n".into());
                        } else {
                            let area = parts[1].to_lowercase();
                            let actor = session.username.clone().unwrap_or("?".into());
                            post_action = PostAction::Lock {
                                area: area.clone(),
                                actor,
                            };
                            deferred_reply = Some(format!("Area {} locked.\n", area));
                        }
                    }
                } else if upper.starts_with("UNLOCK ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: UNLOCK <area>\n".into());
                        } else {
                            let area = parts[1].to_lowercase();
                            let actor = session.username.clone().unwrap_or("?".into());
                            post_action = PostAction::Unlock {
                                area: area.clone(),
                                actor,
                            };
                            deferred_reply = Some(format!("Area {} unlocked.\n", area));
                        }
                    }
                } else if upper == "DL" || upper.starts_with("DL ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        let page = if parts.len() >= 2 {
                            parts[1].parse::<usize>().unwrap_or(1)
                        } else {
                            1
                        };
                        match self.storage.get_deletion_audit_page(page, 10).await {
                            Ok(entries) => {
                                if entries.is_empty() {
                                    deferred_reply = Some("No entries.\n".into());
                                } else {
                                    let mut out = String::from("Deletion Log:\n");
                                    for e in entries {
                                        out.push_str(&format!(
                                            "{} {} {} {}\n",
                                            e.timestamp, e.actor, e.topic, e.id
                                        ));
                                    }
                                    deferred_reply = Some(out);
                                }
                            }
                            Err(e) => deferred_reply = Some(format!("Failed: {}\n", e)),
                        }
                    }
                } else if upper.starts_with("ADMINLOG") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        let page = if parts.len() >= 2 {
                            parts[1].parse::<usize>().unwrap_or(1)
                        } else {
                            1
                        };
                        match self.storage.get_admin_audit_page(page, 10).await {
                            Ok(entries) => {
                                if entries.is_empty() {
                                    deferred_reply = Some("No admin audit entries.\n".into());
                                } else {
                                    let mut out = String::from("Admin Audit Log:\n");
                                    for e in entries {
                                        let target_str = e.target.as_deref().unwrap_or("-");
                                        let details_str = e.details.as_deref().unwrap_or("");
                                        out.push_str(&format!(
                                            "{} {} {} {} {}\n",
                                            e.timestamp.format("%m/%d %H:%M"),
                                            e.actor,
                                            e.action,
                                            target_str,
                                            details_str
                                        ));
                                    }
                                    deferred_reply = Some(out);
                                }
                            }
                            Err(e) => deferred_reply = Some(format!("Failed: {}\n", e)),
                        }
                    }
                } else if upper.starts_with("USERS") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        let pattern = if parts.len() >= 2 {
                            Some(parts[1].to_lowercase())
                        } else {
                            None
                        };

                        match self.storage.list_all_users().await {
                            Ok(mut users) => {
                                // Filter users by pattern if provided
                                if let Some(ref p) = pattern {
                                    users.retain(|u| u.username.to_lowercase().contains(p));
                                }

                                let logged_in_usernames: std::collections::HashSet<&str> = self
                                    .get_logged_in_users()
                                    .iter()
                                    .filter_map(|s| s.username.as_deref())
                                    .collect();

                                let mut response = if let Some(ref p) = pattern {
                                    format!("Users matching '{}' ({} found):\n", p, users.len())
                                } else {
                                    format!(
                                        "Registered Users ({}/{}):\n",
                                        users.len(),
                                        self.config.bbs.max_users
                                    )
                                };

                                for user in users {
                                    let status =
                                        if logged_in_usernames.contains(user.username.as_str()) {
                                            "Online"
                                        } else {
                                            "Offline"
                                        };
                                    let role = super::roles::role_name(user.user_level);
                                    response.push_str(&format!(
                                        "  {} ({}, Level {}) - {}\n",
                                        user.username, role, user.user_level, status
                                    ));
                                }

                                if pattern.is_none() {
                                    response.push_str(&format!(
                                        "\nActive Sessions: {}\n",
                                        self.logged_in_session_count()
                                    ));
                                }
                                deferred_reply = Some(response);
                            }
                            Err(e) => {
                                deferred_reply = Some(format!("Failed to list users: {}\n", e))
                            }
                        }
                    }
                } else if upper == "WHO" {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let logged_in = self.get_logged_in_users();
                        if logged_in.is_empty() {
                            deferred_reply = Some("No users currently logged in.\n".into());
                        } else {
                            let mut response = format!("Logged In Users ({}):\n", logged_in.len());
                            for session in logged_in {
                                let username = session.username.as_deref().unwrap_or("Guest");
                                let role = super::roles::role_name(session.user_level);
                                let duration = session.session_duration().num_minutes();
                                let state = match session.state {
                                    super::session::SessionState::MainMenu => "Main Menu",
                                    super::session::SessionState::MessageTopics => "Message Areas",
                                    super::session::SessionState::ReadingMessages => "Reading",
                                    super::session::SessionState::PostingMessage => "Posting",
                                    super::session::SessionState::UserMenu => "User Menu",
                                    super::session::SessionState::TinyHack => "TinyHack",
                                    _ => "Other",
                                };
                                response.push_str(&format!(
                                    "  {} ({}) - {} - {}m - {}\n",
                                    username, role, session.node_id, duration, state
                                ));
                            }
                            deferred_reply = Some(response);
                        }
                    }
                } else if upper.starts_with("USERINFO ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: USERINFO <user>\n".into());
                        } else {
                            let target = parts[1];
                            match self.storage.get_user_details(target).await? {
                                None => deferred_reply = Some("User not found.\n".into()),
                                Some(user) => {
                                    let post_count = self
                                        .storage
                                        .count_user_posts(&user.username)
                                        .await
                                        .unwrap_or(0);
                                    let is_online = self
                                        .get_logged_in_users()
                                        .iter()
                                        .any(|s| s.username.as_deref() == Some(&user.username));
                                    let role = super::roles::role_name(user.user_level);

                                    let mut response =
                                        format!("User Information for {}:\n", user.username);
                                    response.push_str(&format!(
                                        "  Role: {} (Level {})\n",
                                        role, user.user_level
                                    ));
                                    response.push_str(&format!(
                                        "  Status: {}\n",
                                        if is_online { "Online" } else { "Offline" }
                                    ));
                                    response.push_str(&format!(
                                        "  First Login: {}\n",
                                        user.first_login.format("%Y-%m-%d %H:%M UTC")
                                    ));
                                    response.push_str(&format!(
                                        "  Last Login: {}\n",
                                        user.last_login.format("%Y-%m-%d %H:%M UTC")
                                    ));
                                    response.push_str(&format!("  Total Posts: {}\n", post_count));
                                    if let Some(node_id) = &user.node_id {
                                        response.push_str(&format!("  Node ID: {}\n", node_id));
                                    }
                                    deferred_reply = Some(response);
                                }
                            }
                        }
                    }
                } else if upper == "SESSIONS" {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let all_sessions = self.get_active_sessions();
                        let mut response = format!("Active Sessions ({}):\n", all_sessions.len());
                        for s in all_sessions {
                            let username = s.username.as_deref().unwrap_or("Guest");
                            let role = super::roles::role_name(s.user_level);
                            let duration = s.session_duration().num_minutes();
                            let logged_in = if s.is_logged_in() { "Yes" } else { "No" };
                            let state = match s.state {
                                super::session::SessionState::Connected => "Connected",
                                super::session::SessionState::LoggingIn => "Logging In",
                                super::session::SessionState::MainMenu => "Main Menu",
                                super::session::SessionState::MessageTopics => "Message Areas",
                                super::session::SessionState::ReadingMessages => "Reading",
                                super::session::SessionState::PostingMessage => "Posting",
                                super::session::SessionState::Topics => "Topics",
                                super::session::SessionState::Subtopics => "Subtopics",
                                super::session::SessionState::Threads => "Threads",
                                super::session::SessionState::ThreadRead => "Read",
                                super::session::SessionState::ComposeNewTitle => "Compose Title",
                                super::session::SessionState::ComposeNewBody => "Compose Body",
                                super::session::SessionState::ComposeReply => "Compose Reply",
                                super::session::SessionState::ConfirmDelete => "Confirm Delete",
                                super::session::SessionState::UserMenu => "User Menu",
                                super::session::SessionState::UserChangePassCurrent => {
                                    "Pass Verify"
                                }
                                super::session::SessionState::UserChangePassNew => "Pass Update",
                                super::session::SessionState::UserSetPassNew => "Pass Set",
                                super::session::SessionState::TinyHack => "TinyHack",
                                super::session::SessionState::TinyMush => "TinyMUSH",
                                super::session::SessionState::Disconnected => "Disconnected",
                            };
                            response.push_str(&format!(
                                "  {} ({}) | {} | {}m | Login: {} | {}\n",
                                username, role, s.node_id, duration, logged_in, state
                            ));
                        }
                        deferred_reply = Some(response);
                    }
                } else if upper.starts_with("KICK ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 2 {
                            deferred_reply = Some("Usage: KICK <user>\n".into());
                        } else {
                            let target = parts[1];
                            let actor =
                                session.username.as_deref().unwrap_or("unknown").to_string();
                            if target == actor {
                                deferred_reply = Some("Cannot kick yourself.\n".into());
                            } else if target == self.config.bbs.sysop {
                                deferred_reply = Some("Cannot kick sysop.\n".into());
                            } else {
                                match self.force_logout_user(target).await? {
                                    true => {
                                        // Log the administrative action
                                        if let Err(e) = self
                                            .storage
                                            .log_admin_action("KICK", Some(target), &actor, None)
                                            .await
                                        {
                                            warn!("Failed to log admin action: {}", e);
                                        }
                                        deferred_reply =
                                            Some(format!("User {} has been kicked.\n", target));
                                    }
                                    false => {
                                        deferred_reply =
                                            Some("User not found or not logged in.\n".into())
                                    }
                                }
                            }
                        }
                    }
                } else if upper.starts_with("BROADCAST ") {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let message = raw_content.strip_prefix("BROADCAST ").unwrap_or("").trim();
                        if message.is_empty() {
                            deferred_reply = Some("Usage: BROADCAST <message>\n".into());
                        } else {
                            let sender =
                                session.username.as_deref().unwrap_or("System").to_string();
                            let message = message.to_string();
                            post_action = PostAction::Broadcast { message, sender };
                        }
                    }
                } else if upper == "ADMIN" || upper == "DASHBOARD" {
                    if session.user_level < LEVEL_MODERATOR {
                        deferred_reply = Some("Permission denied.\n".into());
                    } else {
                        let stats = self.storage.get_statistics().await?;
                        let active_count = self.get_active_sessions().len();
                        let logged_in_count = self.logged_in_session_count();

                        let mut response = String::from("=== ADMINISTRATIVE DASHBOARD ===\n");
                        response.push_str("System Status:\n");
                        response.push_str(&format!("  Total Users: {}\n", stats.total_users));
                        response.push_str(&format!("  Total Messages: {}\n", stats.total_messages));
                        response.push_str(&format!("  Active Sessions: {}\n", active_count));
                        response.push_str(&format!("  Logged In Users: {}\n", logged_in_count));
                        response.push_str(&format!("  Max Users: {}\n", self.config.bbs.max_users));
                        response.push_str(&format!(
                            "  Session Timeout: {} min\n",
                            self.config.bbs.session_timeout
                        ));
                        response.push_str("\nCommands: USERS, WHO, USERINFO <user>, SESSIONS, KICK <user>, BROADCAST <msg>\n");
                        deferred_reply = Some(response);
                    }
                } else if upper == "LOGOUT" {
                    if session.is_logged_in() {
                        let name = session.display_name();
                        session.logout().await?;
                        deferred_reply = Some(format!("User {} logged out.\n", name));
                    } else {
                        deferred_reply = Some("Not logged in.\n".into());
                    }
                } else if upper == "H"
                    || (upper == "?" && session.state != super::session::SessionState::TinyHack)
                {
                    // Process help command and send response (inlined to avoid borrowing conflicts)
                    let mut help_text = session
                        .process_command("H", &mut self.storage, &self.config, &self.game_registry)
                        .await?;
                    if !session.help_seen {
                        session.help_seen = true;
                        help_text.push_str("Shortcuts: M=areas U=user Q=quit\n");
                    }
                    self.send_session_message(&node_key, &help_text, true)
                        .await?;
                    return Ok(());
                } else {
                    let redact = ["REGISTER ", "LOGIN ", "SETPASS ", "CHPASS "];
                    let log_snippet = if redact.iter().any(|p| upper.starts_with(p)) {
                        "<redacted>"
                    } else {
                        raw_content.as_str()
                    };
                    trace!("Session {} generic command '{}'", node_key, log_snippet);
                    let response = session
                        .process_command(
                            &raw_content,
                            &mut self.storage,
                            &self.config,
                            &self.game_registry,
                        )
                        .await?;
                    if !response.is_empty() {
                        deferred_reply = Some(response);
                    }
                }
            }
            match post_action {
                PostAction::None => {}
                PostAction::Delete { area, id, actor } => {
                    match self.moderator_delete_message(&area, &id, &actor).await {
                        Ok(true) => {
                            deferred_reply
                                .get_or_insert(format!("Deleted message {} in {}.\n", id, area));
                        }
                        Ok(false) => {
                            deferred_reply.get_or_insert("Not found.\n".into());
                        }
                        Err(e) => {
                            deferred_reply.get_or_insert(format!("Delete failed: {}\n", e));
                        }
                    }
                }
                PostAction::Lock { area, actor } => {
                    if let Err(e) = self.moderator_lock_topic(&area, &actor).await {
                        deferred_reply.get_or_insert(format!("Lock failed: {}\n", e));
                    }
                }
                PostAction::Unlock { area, actor } => {
                    if let Err(e) = self.moderator_unlock_topic(&area, &actor).await {
                        deferred_reply.get_or_insert(format!("Unlock failed: {}\n", e));
                    }
                }
                PostAction::Broadcast { message, sender } => {
                    match self.broadcast_message(&message, &sender).await {
                        Ok(0) => {
                            deferred_reply
                                .get_or_insert("No users online to receive broadcast.\n".into());
                        }
                        Ok(count) => {
                            // Log the administrative action
                            if let Err(e) = self
                                .storage
                                .log_admin_action("BROADCAST", None, &sender, Some(&message))
                                .await
                            {
                                warn!("Failed to log admin action: {}", e);
                            }
                            deferred_reply
                                .get_or_insert(format!("Broadcast sent to {} users.\n", count));
                        }
                        Err(e) => {
                            deferred_reply.get_or_insert(format!("Broadcast failed: {}\n", e));
                        }
                    }
                }
            } // End of session mutable borrow scope

            // Send deferred chunks (for HELP+) after releasing session borrow
            if let Some(chunks) = deferred_chunks {
                let total = chunks.len();
                for (i, chunk) in chunks.into_iter().enumerate() {
                    let last = i + 1 == total;
                    // For multi-part help, suppress prompt until final
                    self.send_session_message(&node_key, &chunk, last).await?;
                }
            }

            // Send deferred reply after releasing session borrow to allow send_session_message to access session
            if let Some(msg) = deferred_reply {
                self.send_session_message(&node_key, &msg, true).await?;
            }
            // end direct path handling (removed extra closing brace)
        } else {
            // Public channel event: parse lightweight commands
            self.public_state.prune_expired();

            // Check if this is a node with default name that should be welcomed
            // This catches nodes that chat publicly without us having seen their NODEINFO
            let node_id = ev.source;
            // Look up node name from cache first (before any mutable borrows)
            let long_name_opt = self.lookup_long_name_from_cache(node_id);

            if let Some(ref mut welcome_state) = self.welcome_state {
                if let Some(long_name) = long_name_opt {
                    if crate::bbs::welcome::is_default_name(&long_name) {
                        // Check if we should welcome this node (real-time detection - apply rate limit)
                        if welcome_state.should_welcome(
                            node_id,
                            &long_name,
                            &self.config.apps.welcome,
                            false,
                        ) {
                            debug!(
                                "Detected unwelcomed default-named node 0x{:08X} ({}) via public message, triggering welcome",
                                node_id, long_name
                            );
                            // Create a NodeDetectionEvent and handle welcome
                            let detection_event = crate::meshtastic::NodeDetectionEvent {
                                node_id,
                                long_name: long_name.clone(),
                                short_name: format!("{:04x}", node_id & 0xFFFF),
                                is_from_startup_queue: false, // Real-time detection - apply rate limit
                            };
                            if let Err(e) = self.handle_node_detection(detection_event).await {
                                warn!("Failed to welcome node from public message: {}", e);
                            }
                        }
                    }
                }
            }

            let cmd = self.public_parser.parse(&ev.content);
            trace!(
                "Public command parse result for node {} => {:?}",
                node_key,
                cmd
            );
            match cmd {
                PublicCommand::Help => {
                    if self.public_state.should_reply(&node_key) {
                        // Compose public commands list and detailed DM help
                        // Prefer a friendly node label (short label) if the protobuf node catalog knows it.
                        // Support node keys provided either as plain decimal or hex with 0x prefix.
                        // Prefer short name from node cache; fallback to hex/id
                        let friendly = {
                            let id_opt = if let Ok(id_dec) = node_key.parse::<u32>() {
                                Some(id_dec)
                            } else if let Some(hex) = node_key
                                .strip_prefix("0x")
                                .or_else(|| node_key.strip_prefix("0X"))
                            {
                                u32::from_str_radix(hex, 16).ok()
                            } else {
                                None
                            };
                            if let Some(id) = id_opt {
                                if let Some(sn) = self.lookup_short_name_from_cache(id) {
                                    debug!("Help request from node {} (0x{:08x}): using short name '{}'", id, id, sn);
                                    sn
                                } else {
                                    // Fallback to compact hex label like Meshtastic short style
                                    let fallback = format!("0x{:06X}", id & 0xFFFFFF);
                                    debug!("Help request from node {} (0x{:08x}): no short name in cache, using '{}'", id, id, fallback);
                                    fallback
                                }
                            } else {
                                debug!("Help request from unparseable node key: '{}'", node_key);
                                node_key.clone()
                            }
                        };

                        // Create broadcast message showing available public commands with chunking
                        let primary_prefix = self.public_parser.primary_prefix_char();
                        let help_cmd = self.public_parser.help_command();
                        let mut public_commands = vec![
                            format!("{p}{h} - Show this help", p = primary_prefix, h = help_cmd),
                            format!("{p}LOGIN <user> - Register for BBS", p = primary_prefix),
                        ];

                        // Add optional weather command if enabled
                        #[cfg(feature = "weather")]
                        public_commands.push(format!(
                            "{p}WEATHER - Current conditions",
                            p = primary_prefix
                        ));

                        // Add games and utilities
                        public_commands.extend_from_slice(&[
                            format!("{p}SLOT - Play slot machine", p = primary_prefix),
                            format!("{p}SLOTSTATS - Show your stats", p = primary_prefix),
                            format!("{p}8BALL - Magic 8-Ball oracle", p = primary_prefix),
                            format!("{p}FORTUNE - Random wisdom", p = primary_prefix),
                        ]);

                        // Send DM first, then chunked public notices. This reduces the chance of a transient rate limit
                        // affecting the DM, since the DM is more time-sensitive for onboarding.
                        debug!("Processing HELP DM for node {} (0x{:08x}). Raw ev.source={}, node_key='{}'", node_key, ev.source, ev.source, node_key);

                        let help_text = format!(
                            "[{}] {}: REGISTER <user> <pass>; then LOGIN <user> <pass>. Type {} in DM for more.",
                            self.config.bbs.name,
                            help_cmd,
                            help_cmd
                        );

                        match self.send_message(&node_key, &help_text).await {
                            Ok(_) => debug!("Sent HELP DM to {}", ev.source),
                            Err(e) => warn!("Failed to send HELP DM to {}: {}", ev.source, e),
                        }

                        // Create chunked public notices, ensuring each stays under 230 bytes
                        let mut chunks = Vec::new();
                        let max_chunk_size = 220; // Leave some margin below 230 bytes

                        // First chunk header
                        let first_header = format!(
                            "[{}] Public Commands (for {}): ",
                            self.config.bbs.name, friendly
                        );
                        let continuation_header = format!("[{}] More: ", self.config.bbs.name);

                        let mut current_chunk = first_header.clone();
                        let mut is_first_chunk = true;
                        let mut commands_added = 0;

                        for command in &public_commands {
                            let separator = if commands_added == 0 { "" } else { " | " };
                            let test_chunk = format!("{}{}{}", current_chunk, separator, command);

                            // Check if adding this command would exceed the limit
                            if test_chunk.len() > max_chunk_size {
                                // Finalize current chunk
                                if is_first_chunk {
                                    current_chunk.push_str(" | DM for BBS access");
                                }
                                chunks.push(current_chunk);

                                // Start new chunk
                                current_chunk = format!("{}{}", continuation_header, command);
                                is_first_chunk = false;
                                commands_added = 1;
                            } else {
                                current_chunk = test_chunk;
                                commands_added += 1;
                            }
                        }

                        // Finalize last chunk
                        if !current_chunk.is_empty() {
                            if is_first_chunk {
                                current_chunk.push_str(" | DM for BBS access");
                            }
                            chunks.push(current_chunk);
                        }

                        // Schedule all chunks with appropriate delays
                        let delay_ms = {
                            let cfg = &self.config.meshtastic;
                            let base = cfg.help_broadcast_delay_ms.unwrap_or(3500);
                            // Ensure it's at least the low-level post_dm_broadcast_gap_ms plus min send gap
                            let post_gap = cfg.post_dm_broadcast_gap_ms.unwrap_or(1200);
                            let min_gap = cfg.min_send_gap_ms.unwrap_or(2000);
                            let required = post_gap.saturating_add(min_gap);
                            if base < required {
                                required
                            } else {
                                base
                            }
                        };

                        if let Some(scheduler) = &self.scheduler {
                            for (i, chunk) in chunks.iter().enumerate() {
                                let chunk_delay = delay_ms + (i as u64 * 2500); // 2.5s between chunks
                                debug!(
                                    "Scheduling HELP public chunk {} in {}ms (text='{}')",
                                    i + 1,
                                    chunk_delay,
                                    escape_log(chunk)
                                );
                                let outgoing = crate::meshtastic::OutgoingMessage {
                                    to_node: None,
                                    channel: self.primary_channel(),
                                    content: chunk.clone(),
                                    priority: crate::meshtastic::MessagePriority::Normal,
                                    kind: crate::meshtastic::OutgoingKind::Normal,
                                    request_ack: true,
                                };
                                let env = crate::bbs::dispatch::MessageEnvelope::new(
                                    crate::bbs::dispatch::MessageCategory::HelpBroadcast,
                                    crate::bbs::dispatch::Priority::Low,
                                    Duration::from_millis(chunk_delay),
                                    outgoing,
                                );
                                scheduler.enqueue(env);
                            }
                        } else {
                            // Fallback legacy path - ensure tx is properly cloned for each spawn
                            if let Some(base_tx) = self.outgoing_tx.clone() {
                                // Avoid capturing &self inside spawned tasks; compute channel once.
                                let channel = self.primary_channel();
                                for (i, chunk) in chunks.iter().enumerate() {
                                    let chunk_content = chunk.clone();
                                    let tx_clone = base_tx.clone();
                                    let chunk_delay = delay_ms + (i as u64 * 2500);
                                    debug!("Scheduling HELP public chunk {} in {}ms (legacy path) text='{}'", i + 1, chunk_delay, escape_log(&chunk_content));
                                    tokio::spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(
                                            chunk_delay,
                                        ))
                                        .await;
                                        let outgoing = crate::meshtastic::OutgoingMessage {
                                            to_node: None,
                                            channel,
                                            content: chunk_content,
                                            priority: crate::meshtastic::MessagePriority::Normal,
                                            kind: crate::meshtastic::OutgoingKind::Normal,
                                            request_ack: true,
                                        };
                                        if let Err(e) = tx_clone.send(outgoing) {
                                            log::warn!(
                                                "Failed to queue scheduled HELP public chunk: {}",
                                                e
                                            );
                                        } else {
                                            log::debug!("Queued scheduled HELP public chunk after delay (legacy)");
                                        }
                                    });
                                }
                            } else {
                                warn!("Cannot schedule HELP public chunks: no outgoing path");
                            }
                        }
                    }
                }
                PublicCommand::Login(username) => {
                    // Check if public LOGIN is allowed by configuration
                    if !self.config.bbs.allow_public_login {
                        // Silently ignore public LOGIN when disabled - no reply to avoid spam/enumeration
                        trace!(
                            "Public LOGIN ignored (disabled by config): user={}, node={}",
                            username,
                            node_key
                        );
                    } else if self.public_state.should_reply(&node_key) {
                        self.public_state.set_pending(&node_key, username.clone());
                        let reply = format!("Login pending for '{}'. Open a direct message to this node and say HI or LOGIN <name> again to complete.", username);
                        self.send_message(&node_key, &reply).await?;
                    }
                }
                PublicCommand::Weather => {
                    if self.public_state.should_reply(&node_key) {
                        let weather = self.fetch_weather().await.unwrap_or_else(|| {
                            "Error fetching weather. Please try again later.".to_string()
                        });
                        let mut broadcasted = false;
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            match self.send_broadcast(&weather).await {
                                Ok(_) => {
                                    trace!("Broadcasted weather to public channel: '{}'", weather);
                                    broadcasted = true;
                                }
                                Err(e) => {
                                    warn!("Weather broadcast failed: {e:?} (will fallback DM)");
                                }
                            }
                        }
                        if !broadcasted {
                            // Fallback: send as direct message so user gets feedback instead of silence
                            if let Err(e) = self.send_message(&node_key, &weather).await {
                                warn!("Weather DM fallback failed: {e:?}");
                            }
                        }
                    }
                }
                PublicCommand::SlotMachine => {
                    // Use a lighter, per-node slot cooldown (does not block other public replies)
                    // Broadcast-only: do not DM slot results.
                    if self.public_state.allow_slot(&node_key) {
                        let base = self.storage.base_dir().to_string();
                        let p = self.public_parser.primary_prefix_char();
                        let (outcome, coins) =
                            crate::bbs::slotmachine::perform_spin(&base, &node_key);
                        let msg = if outcome.r1 == "⛔" {
                            let eta = crate::bbs::slotmachine::next_refill_eta(&base, &node_key)
                                .map(|(h, m)| {
                                    format!(" Next refill in ~{}h {}m.", h.max(0), m.max(0))
                                })
                                .unwrap_or_default();
                            format!(
                                "{p}SLOT ⟶ {} | {} | {}  - {}{}",
                                outcome.r1, outcome.r2, outcome.r3, outcome.description, eta
                            )
                        } else if outcome.multiplier > 0 {
                            format!(
                                "{p}SLOT ⟶ {} | {} | {}  - WIN x{} (+{} coins). Balance: {}",
                                outcome.r1,
                                outcome.r2,
                                outcome.r3,
                                outcome.multiplier,
                                outcome.winnings,
                                coins
                            )
                        } else {
                            format!(
                                "{p}SLOT ⟶ {} | {} | {}  - Loss (-{} coins). Balance: {}",
                                outcome.r1,
                                outcome.r2,
                                outcome.r3,
                                crate::bbs::slotmachine::BET_COINS,
                                coins
                            )
                        };
                        // Broadcast result for room visibility (best-effort)
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            if let Err(e) = self.send_broadcast(&msg).await {
                                warn!("Slot result broadcast failed (best-effort): {e:?}");
                            }
                        }
                    }
                }
                PublicCommand::EightBall => {
                    // Lightweight per-node cooldown similar to <prefix>SLOT; broadcast-only.
                    if self.public_state.allow_8ball(&node_key) {
                        match crate::bbs::eightball::ask() {
                            Some(answer) => {
                                let p = self.public_parser.primary_prefix_char();
                                let msg = format!("{p}8BALL ⟶ {}", answer);
                                #[cfg(feature = "meshtastic-proto")]
                                {
                                    if let Err(e) = self.send_broadcast(&msg).await {
                                        warn!("8BALL broadcast failed (best-effort): {e:?}");
                                    }
                                }
                            }
                            None => {
                                warn!("[apps] 8-Ball command received but responses not available");
                            }
                        }
                    }
                }
                PublicCommand::Fortune => {
                    // Lightweight per-node cooldown; broadcast-only like other games.
                    if self.public_state.allow_fortune(&node_key) {
                        // Check if fortune data is available
                        if let Some(fortune) = crate::bbs::fortune::random_fortune() {
                            let p = self.public_parser.primary_prefix_char();
                            let msg = format!("{p}FORTUNE ⟶ {}", fortune);
                            #[cfg(feature = "meshtastic-proto")]
                            {
                                if let Err(e) = self.send_broadcast(&msg).await {
                                    warn!("FORTUNE broadcast failed (best-effort): {e:?}");
                                }
                            }
                        } else {
                            // Fortune data not loaded - silently skip
                            warn!(
                                "Fortune requested but data not loaded for node {}",
                                node_key
                            );
                        }
                    }
                }
                PublicCommand::SlotStats => {
                    if self.public_state.should_reply(&node_key) {
                        let base = self.storage.base_dir().to_string();
                        let summary = crate::bbs::slotmachine::get_player_summary(&base, &node_key);
                        let j = crate::bbs::slotmachine::get_jackpot_summary(&base);
                        let jdate = j
                            .last_win_date
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_else(|| "-".into());
                        let jwinner_short = if let Some(id_str) = j.last_win_node.as_deref() {
                            self.lookup_short_name_from_cache(id_str.parse().ok().unwrap_or(0))
                                .unwrap_or_else(|| id_str.to_string())
                        } else {
                            "-".to_string()
                        };
                        let p = self.public_parser.primary_prefix_char();
                        let msg = if let Some(s) = summary {
                            let rate = if s.total_spins > 0 {
                                (s.total_wins as f32) * 100.0 / (s.total_spins as f32)
                            } else {
                                0.0
                            };
                            format!(
                                "{p}SLOTSTATS ⟶ Coins: {} | Spins: {} | Wins: {} ({:.1}%) | Jackpots: {} | Pot: {} | Last Win: {} by {}",
                                s.coins, s.total_spins, s.total_wins, rate, s.jackpots, j.amount, jdate, jwinner_short
                            )
                        } else {
                            format!("{p}SLOTSTATS ⟶ No stats yet. Spin with {p}SLOT to begin! | Pot: {} | Last Win: {} by {}", j.amount, jdate, jwinner_short)
                        };
                        let mut broadcasted = false;
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            if let Err(e) = self.send_broadcast(&msg).await {
                                warn!("Slot stats broadcast failed: {e:?} (will fallback DM)");
                            } else {
                                broadcasted = true;
                            }
                        }
                        if !broadcasted {
                            let _ = self.send_message(&node_key, &msg).await;
                        }
                    }
                }
                PublicCommand::Invalid(reason) => {
                    if self.public_state.should_reply(&node_key) {
                        let reply = format!("Invalid: {}", reason);
                        self.send_message(&node_key, &reply).await?;
                    }
                }
                PublicCommand::Unknown => {
                    // Ignore to reduce noise
                }
            }
        }
        Ok(())
    }

    /// Send a message to a specific node
    pub async fn send_message(&mut self, to_node: &str, message: &str) -> Result<()> {
        #[cfg(feature = "meshtastic-proto")]
        {
            // Circuit breaker: check scheduler health before enqueueing
            if let Some(scheduler) = &self.scheduler {
                // Check if we should throttle based on queue state
                if let Some(stats) = scheduler.snapshot().await {
                    let max_queue = self.config.meshtastic.scheduler_max_queue.unwrap_or(512);

                    // If queue is critically full (>95%), reject non-critical messages
                    if stats.queued > (max_queue * 95 / 100) {
                        warn!(
                            "Scheduler queue critically full ({}/{}), dropping message to {} to prevent total failure",
                            stats.queued, max_queue, to_node
                        );
                        return Err(anyhow::anyhow!("Message queue full - system overloaded"));
                    }
                }

                let node_id = if let Some(hex) = to_node
                    .strip_prefix("0x")
                    .or_else(|| to_node.strip_prefix("0X"))
                {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    to_node.parse::<u32>().ok()
                };
                if let Some(id) = node_id {
                    let outgoing = OutgoingMessage {
                        to_node: Some(id),
                        channel: self.primary_channel(),
                        content: message.to_string(),
                        priority: MessagePriority::High,
                        kind: crate::meshtastic::OutgoingKind::Normal,
                        request_ack: false,
                    };
                    let env = crate::bbs::dispatch::MessageEnvelope::new(
                        crate::bbs::dispatch::MessageCategory::Direct,
                        crate::bbs::dispatch::Priority::High,
                        Duration::from_millis(0),
                        outgoing,
                    );
                    scheduler.enqueue(env);
                } else {
                    warn!("Invalid node ID format: {}", to_node);
                    return Err(anyhow!("Invalid node ID format: {}", to_node));
                }
            } else if let Some(ref tx) = self.outgoing_tx {
                // Parse node ID from string when actually sending to radio
                let node_id = if let Some(hex) = to_node
                    .strip_prefix("0x")
                    .or_else(|| to_node.strip_prefix("0X"))
                {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    to_node.parse::<u32>().ok()
                };

                if let Some(id) = node_id {
                    let outgoing = OutgoingMessage {
                        to_node: Some(id),
                        channel: self.primary_channel(),
                        content: message.to_string(),
                        priority: MessagePriority::High,
                        kind: crate::meshtastic::OutgoingKind::Normal,
                        request_ack: false,
                    };

                    match tx.send(outgoing) {
                        Ok(_) => {
                            debug!("Queued message to {}: {}", to_node, escape_log(message));
                        }
                        Err(e) => {
                            warn!("Failed to queue message to {}: {}", to_node, e);
                            return Err(anyhow!("Failed to queue message: {}", e));
                        }
                    }
                } else {
                    warn!("Invalid node ID format: {}", to_node);
                    return Err(anyhow!("Invalid node ID format: {}", to_node));
                }
            } else {
                // No device connected; operate in mock/test mode and just record the message
                debug!(
                    "Mock send (no device) to {}: {}",
                    to_node,
                    escape_log(message)
                );
            }
        }

        #[cfg(not(feature = "meshtastic-proto"))]
        {
            // Fallback for when meshtastic-proto feature is disabled
            if let Some(ref mut device) = self.device {
                device.send_message(to_node, message).await?;
                debug!("Sent message to {}: {}", to_node, escape_log(message));
            } else {
                // No device connected in non-proto mode either; treat as mock send for tests
                debug!(
                    "Mock send (no device) to {}: {}",
                    to_node,
                    escape_log(message)
                );
            }
        }

        self.test_messages
            .push((to_node.to_string(), message.to_string()));
        Ok(())
    }

    /// Send a broadcast message to the public channel
    #[cfg(feature = "meshtastic-proto")]
    pub async fn send_broadcast(&mut self, message: &str) -> Result<()> {
        if let Some(scheduler) = &self.scheduler {
            let outgoing = OutgoingMessage {
                to_node: None,
                channel: self.primary_channel(),
                content: message.to_string(),
                priority: MessagePriority::Normal,
                kind: crate::meshtastic::OutgoingKind::Normal,
                request_ack: true,
            };
            let env = crate::bbs::dispatch::MessageEnvelope::new(
                crate::bbs::dispatch::MessageCategory::Broadcast,
                crate::bbs::dispatch::Priority::Low,
                Duration::from_millis(0),
                outgoing,
            );
            scheduler.enqueue(env);
            Ok(())
        } else {
            let outgoing = OutgoingMessage {
                to_node: None,
                channel: self.primary_channel(),
                content: message.to_string(),
                priority: MessagePriority::Normal,
                kind: crate::meshtastic::OutgoingKind::Normal,
                request_ack: true,
            };
            if let Some(ref tx) = self.outgoing_tx {
                match tx.send(outgoing) {
                    Ok(_) => {
                        debug!("Queued broadcast message: {}", escape_log(message));
                        Ok(())
                    }
                    Err(e) => {
                        warn!("Failed to queue broadcast message: {}", e);
                        Err(anyhow!("Failed to queue broadcast: {}", e))
                    }
                }
            } else {
                // Mock/test mode: record broadcast for assertions
                debug!("Mock broadcast (no device): {}", escape_log(message));
                self.test_messages
                    .push(("BCAST".to_string(), message.to_string()));
                Ok(())
            }
        }
    }

    /// Send a session-scoped reply, automatically appending a dynamic prompt unless suppressed.
    /// Ensures the combined body + optional newline + prompt is ≤ config.storage.max_message_size bytes.
    /// If chunked is true and not last_chunk, no prompt is appended (used for future multi-part HELP+).
    async fn send_session_message(
        &mut self,
        node_key: &str,
        body: &str,
        last_chunk: bool,
    ) -> Result<()> {
        // UTF-8 safe clamp to a given byte budget
        fn clamp_utf8(s: &str, max_bytes: usize) -> String {
            if s.len() <= max_bytes {
                return s.to_string();
            }
            let mut end = max_bytes.min(s.len());
            while end > 0 && !s.is_char_boundary(end) {
                end -= 1;
            }
            s[..end].to_string()
        }

        if let Some(session) = self.sessions.get(node_key) {
            if last_chunk {
                // Compute budget for body to leave space for optional newline + prompt
                let prompt = session.build_prompt();
                let max_total = self.config.storage.max_message_size;
                let prompt_len = prompt.len();
                let extra_nl = if body.ends_with('\n') { 0 } else { 1 };
                let budget = max_total.saturating_sub(prompt_len + extra_nl);

                debug!(
                    "send_session_message: body_len={} budget={} max_total={} prompt_len={} extra_nl={} state={:?}",
                    body.len(), budget, max_total, prompt_len, extra_nl, session.state
                );

                if body.len() > budget {
                    // Auto-chunk oversized body into UTF-8 safe segments of size <= budget.
                    // Send intermediate chunks without prompt; attach prompt only to the last one.

                    // Calculate chunk marker overhead if enabled
                    // Markers will be in format " [n/total]" where total <= 99
                    // Worst case: " [99/99]" = 9 bytes
                    let marker_overhead = if self.config.storage.show_chunk_markers {
                        9 // Reserve space for " [nn/nn]" marker
                    } else {
                        0
                    };

                    let chunk_budget = budget.saturating_sub(marker_overhead);
                    let parts = self.chunk_utf8(body, chunk_budget);
                    let total = parts.len();
                    debug!(
                        "Chunking message into {} parts (markers={}, overhead={})",
                        total, self.config.storage.show_chunk_markers, marker_overhead
                    );

                    for (i, chunk) in parts.into_iter().enumerate() {
                        let is_last = i + 1 == total;
                        let chunk_num = i + 1;

                        debug!("Chunk {}/{}: {} bytes", chunk_num, total, chunk.len());

                        if is_last {
                            let clamped = clamp_utf8(&chunk, chunk_budget);
                            let clamped_len = clamped.len();
                            let with_marker = if self.config.storage.show_chunk_markers {
                                format!("{} [{}/{}]", clamped, chunk_num, total)
                            } else {
                                clamped
                            };

                            let msg = if with_marker.ends_with('\n') {
                                format!("{}{}", with_marker, prompt)
                            } else {
                                format!("{}\n{}", with_marker, prompt)
                            };
                            debug!(
                                "Sending last chunk: {} bytes (clamped={}, marker={}, prompt={})",
                                msg.len(),
                                clamped_len,
                                if self.config.storage.show_chunk_markers {
                                    marker_overhead
                                } else {
                                    0
                                },
                                prompt.len()
                            );
                            self.send_message(node_key, &msg).await?;
                        } else {
                            // Send intermediate chunk with marker if enabled
                            let with_marker = if self.config.storage.show_chunk_markers {
                                format!("{} [{}/{}]", chunk, chunk_num, total)
                            } else {
                                chunk.to_string()
                            };
                            debug!("Sending intermediate chunk: {} bytes", with_marker.len());
                            self.send_message(node_key, &with_marker).await?;
                        }
                    }
                    return Ok(());
                } else {
                    // Fits in one frame; append prompt and send once
                    let clamped = clamp_utf8(body, budget);
                    let msg = if clamped.ends_with('\n') {
                        format!("{}{}", clamped, prompt)
                    } else {
                        format!("{}\n{}", clamped, prompt)
                    };
                    return self.send_message(node_key, &msg).await;
                }
            } else {
                // Non-final chunk: send body as-is (no prompt). Caller handles sequencing.
                return self.send_message(node_key, body).await;
            }
        } else {
            warn!(
                "send_session_message: No session found for node_key={} body_len={} - sending without chunking!",
                node_key, body.len()
            );
        }

        // No session (should be rare): just forward as-is
        self.send_message(node_key, body).await
    }

    // (legacy) exported_test_messages retained for backwards compatibility in tests
    #[cfg(test)]
    #[allow(dead_code)] // Accessed by some legacy / external integration harnesses; keep until removed.
    pub fn exported_test_messages(&self) -> &Vec<(String, String)> {
        &self.test_messages
    }

    /// Lightweight direct-message routing helper for tests (no meshtastic-proto TextEvent needed)
    #[allow(dead_code)]
    pub async fn route_test_text_direct(&mut self, node_key: &str, content: &str) -> Result<()> {
        // Minimal emulation of direct path portion of route_text_event without meshtastic-proto TextEvent struct
        if !self.sessions.contains_key(node_key) {
            let session = Session::new(node_key.to_string(), node_key.to_string());
            self.sessions.insert(node_key.to_string(), session);
        }
        // Inline simplified logic: replicate relevant subset from route_text_event for test
        let raw_content = content.trim().to_string();
        let upper = raw_content.to_uppercase();
        let logged_in_count = self.sessions.values().filter(|s| s.is_logged_in()).count();
        let mut deferred_reply: Option<String> = None;
        if let Some(session) = self.sessions.get_mut(node_key) {
            session.update_activity();
            if upper == "HELP+" || upper == "HELP V" || upper == "HELP  V" || upper == "HELP  +" {
                let chunks = chunk_verbose_help_with_prefix(
                    self.public_parser.primary_prefix_char(),
                    self.public_parser.help_command(),
                );
                let total = chunks.len();
                for (i, chunk) in chunks.into_iter().enumerate() {
                    let last = i + 1 == total;
                    self.send_session_message(node_key, &chunk, last).await?;
                }
                return Ok(());
            } else if upper == "H" || upper == "?" {
                let mut help_text = session
                    .process_command("H", &mut self.storage, &self.config, &self.game_registry)
                    .await?;
                if !session.help_seen {
                    session.help_seen = true;
                    help_text.push_str("Shortcuts: M=areas U=user Q=quit\n");
                }
                self.send_session_message(node_key, &help_text, true)
                    .await?;
                return Ok(());
            } else if upper.starts_with("LOGIN ") {
                if !session.is_logged_in() && (logged_in_count as u32) >= self.config.bbs.max_users
                {
                    deferred_reply = Some(
                        "All available sessions are in use, please wait and try again later.\n"
                            .into(),
                    );
                } else if session.is_logged_in() {
                    deferred_reply = Some(format!(
                        "Already logged in as {}.\n",
                        session.display_name()
                    ));
                } else {
                    let parts: Vec<&str> = raw_content.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let user = parts[1];
                        // For tests, initialize unread_since from stored user's last_login if present
                        let mut prev_last_opt = None;
                        let mut level = 1u8;
                        if let Ok(Some(u)) = self.storage.get_user(user).await {
                            prev_last_opt = Some(u.last_login);
                            level = u.user_level;
                        }
                        session.login(user.to_string(), level).await?;
                        if let Some(prev) = prev_last_opt {
                            if let Some(s2) = self.sessions.get_mut(node_key) {
                                s2.unread_since = Some(prev);
                            }
                        }
                        let hint = Self::format_hint_line(&self.config.apps, false);
                        let menu = Self::format_main_menu(&self.config.apps);
                        deferred_reply = Some(format!(
                            "Welcome, {} you are now logged in.\n{}{}{}",
                            user,
                            Self::format_unread_line(0),
                            hint,
                            menu
                        ));
                    }
                }
            } else {
                let game_doors = games::enabled_doors(&self.config.apps);
                if upper == "G" || upper == "GAMES" {
                    let msg = games::format_games_menu(&game_doors);
                    deferred_reply = Some(msg);
                } else if let Some(door) = games::resolve_games_command(&upper, &game_doors) {
                    match door.kind {
                        GameDoorKind::TinyHack => {
                            session.state = super::session::SessionState::TinyHack;
                            let username = session.display_name();
                            let (gs, screen, _is_new) = crate::bbs::tinyhack::load_or_new_with_flag(
                                self.storage.base_dir(),
                                &username,
                            );
                            session.filter_text =
                                Some(serde_json::to_string(&gs).unwrap_or_default());
                            self.send_session_message(
                                node_key,
                                crate::bbs::tinyhack::welcome_message(),
                                true,
                            )
                            .await?;
                            self.send_session_message(node_key, &screen, true).await?;
                            return Ok(());
                        }
                        GameDoorKind::TinyMush => {
                            // Handle TinyMUSH entry like production code does
                            session.state = super::session::SessionState::TinyMush;
                            session.current_game_slug = Some(door.slug.to_string());
                            let _username = session.display_name();

                            // Initialize TinyMUSH and send welcome using shared store from game registry
                            if let Some(store) = self.game_registry.get_tinymush_store() {
                                let mut processor =
                                    crate::tmush::commands::TinyMushProcessor::new(store.clone());
                                let welcome = processor
                                    .initialize_player(session, &mut self.storage, &self.config)
                                    .await?;
                                self.send_session_message(node_key, &welcome, true).await?;
                            } else {
                                self.send_session_message(
                                    node_key,
                                    "TinyMUSH is not available",
                                    true,
                                )
                                .await?;
                            }
                            return Ok(());
                        }
                    }
                } else {
                    let response = session
                        .process_command(
                            &raw_content,
                            &mut self.storage,
                            &self.config,
                            &self.game_registry,
                        )
                        .await?;
                    if !response.is_empty() {
                        deferred_reply = Some(response);
                    }
                }
            }
        }
        if let Some(msg) = deferred_reply {
            self.send_session_message(node_key, &msg, true).await?;
        }
        Ok(())
    }

    #[allow(unused)]
    #[cfg(feature = "weather")]
    async fn fetch_weather(&mut self) -> Option<String> {
        match self.weather_service.get_weather().await {
            Ok(weather) => Some(weather),
            Err(e) => {
                debug!("Weather service error: {}", e);
                None
            }
        }
    }

    #[allow(unused)]
    #[cfg(not(feature = "weather"))]
    async fn fetch_weather(&mut self) -> Option<String> {
        None
    }

    /// Check for abandoned housing and process cleanup
    async fn check_housing_cleanup(&mut self) -> Result<()> {
        use crate::tmush::housing_cleanup::{
            check_and_cleanup_housing, CleanupConfig, CleanupStats,
        };

        info!("Running daily housing cleanup check");

        // Get TinyMUSH store from game registry
        let Some(tmush_store) = self.game_registry.get_tinymush_store() else {
            debug!("TinyMUSH not available, skipping housing cleanup");
            return Ok(());
        };

        // Get world config for notifications
        let world_config = tmush_store.get_world_config()?;

        // Create notification callback that sends DMs to online users
        let sessions = self.sessions.clone();
        let notification_fn =
            std::sync::Arc::new(move |username: &str, housing_name: &str, message: &str| {
                // Check if user is online
                if let Some(_session) = sessions
                    .values()
                    .find(|s| s.username.as_deref() == Some(username))
                {
                    // User is online - notification would be sent here
                    // For now, just log it since we don't have direct access to send_message
                    info!(
                        "Housing notification for {}: {} - {}",
                        username,
                        housing_name,
                        message.lines().next().unwrap_or("")
                    );
                }
            });

        let config = CleanupConfig::default();
        let mut stats = CleanupStats::default();

        check_and_cleanup_housing(
            &tmush_store,
            &self.storage,
            &config,
            &mut stats,
            &world_config,
            Some(notification_fn),
        )
        .await?;

        info!(
            "Housing cleanup complete: {} checks, {} marked inactive, {} warnings, {} reclaim boxes deleted",
            stats.checks_performed,
            stats.items_moved_to_reclaim,
            stats.warnings_issued,
            stats.reclaim_boxes_deleted
        );

        Ok(())
    }

    /// Process recurring housing payments
    async fn process_housing_payments(&mut self) -> Result<()> {
        use crate::tmush::housing_cleanup::process_recurring_payments;

        info!("Processing daily housing recurring payments");

        // Get TinyMUSH store from game registry
        let Some(tmush_store) = self.game_registry.get_tinymush_store() else {
            debug!("TinyMUSH not available, skipping housing payments");
            return Ok(());
        };

        // Get world config for notifications
        let world_config = tmush_store.get_world_config()?;

        // Create notification callback that sends DMs to online users
        let sessions = self.sessions.clone();
        let notification_fn =
            std::sync::Arc::new(move |username: &str, housing_name: &str, message: &str| {
                // Check if user is online
                if let Some(_session) = sessions
                    .values()
                    .find(|s| s.username.as_deref() == Some(username))
                {
                    // User is online - notification would be sent here
                    // For now, just log it since we don't have direct access to send_message
                    info!(
                        "Housing payment notification for {}: {} - {}",
                        username,
                        housing_name,
                        message.lines().next().unwrap_or("")
                    );
                }
            });

        let (processed, failed) = process_recurring_payments(
            &tmush_store,
            &self.storage,
            &world_config,
            Some(notification_fn),
        )
        .await?;

        info!(
            "Housing payments complete: {} successful, {} failed",
            processed, failed
        );

        Ok(())
    }

    /// Show BBS status and statistics
    pub async fn show_status(&self) -> Result<()> {
        println!("=== Meshbbs Status ===");
        println!("BBS Name: {}", self.config.bbs.name);
        println!("Sysop: {}", self.config.bbs.sysop);
        println!("Location: {}", self.config.bbs.location);
        println!("Active Sessions: {}", self.sessions.len());

        if self.device.is_some() {
            println!("Meshtastic Device: Connected");
        } else {
            println!("Meshtastic Device: Not connected");
        }

        // Storage statistics
        let stats = self.storage.get_statistics().await?;
        println!("Total Messages: {}", stats.total_messages);
        println!("Total Users: {}", stats.total_users);

        Ok(())
    }

    /// Gracefully shutdown the BBS server
    ///
    /// Performs a clean shutdown by:
    /// - Notifying all active sessions
    /// - Closing sessions and flushing user state
    /// - Sending shutdown signals to reader/writer tasks
    /// - Disconnecting from Meshtastic device
    /// - Ensuring all pending writes are completed
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down BBS server...");

        // Close all sessions
        for (session_id, session) in &mut self.sessions {
            info!("Closing session: {}", session_id);
            session.logout().await?;
        }
        self.sessions.clear();

        // Send shutdown signals to reader and writer tasks
        #[cfg(feature = "meshtastic-proto")]
        {
            if let Some(ref tx) = self.reader_control_tx {
                let _ = tx.send(ControlMessage::Shutdown);
            }
            if let Some(ref tx) = self.writer_control_tx {
                let _ = tx.send(ControlMessage::Shutdown);
            }
        }

        // Disconnect device (fallback for non-proto mode)
        if let Some(device) = &mut self.device {
            device.disconnect().await?;
        }

        info!("BBS server shutdown complete");
        Ok(())
    }
}
