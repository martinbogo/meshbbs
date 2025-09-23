use anyhow::Result;
use log::{info, warn, debug, trace};
use tokio::time::sleep; // for short polling delay
use tokio::time::{Instant, Duration};
use tokio::sync::mpsc;
use std::collections::HashMap;

use crate::config::Config;
use crate::meshtastic::MeshtasticDevice;
#[cfg(feature = "meshtastic-proto")]
use crate::meshtastic::TextEvent;
use crate::storage::Storage;
use crate::validation::validate_sysop_name;
use super::session::Session;
use super::public::{PublicState, PublicCommandParser, PublicCommand};
use super::roles::{LEVEL_MODERATOR, LEVEL_USER, role_name};

macro_rules! sec_log {
    ($($arg:tt)*) => { log::warn!(target: "security", $($arg)*); };
}
#[allow(unused_imports)]
pub(crate) use sec_log;

/// Main BBS server that coordinates all operations
pub struct BbsServer {
    config: Config,
    storage: Storage,
    device: Option<MeshtasticDevice>,
    sessions: HashMap<String, Session>,
    message_tx: Option<mpsc::UnboundedSender<String>>,
    public_state: PublicState,
    public_parser: PublicCommandParser,
    #[cfg(feature = "weather")]
    weather_cache: Option<(Instant, String)>, // (fetched_at, value)
    #[cfg(feature = "weather")]
    weather_last_poll: Instant, // track when we last attempted proactive weather refresh
    #[cfg(feature = "meshtastic-proto")]
    pending_direct: Vec<(u32, String)>, // queue of (dest_node_id, message) awaiting our node id
    #[allow(dead_code)]
    #[doc(hidden)]
    pub(crate) test_messages: Vec<(String,String)>, // collected outbound messages (testing)
    // Track the last login banner for test assertions only.
    #[cfg(any(test, feature = "meshtastic-proto"))]
    last_banner: Option<String>,
}

// Verbose HELP material & chunker (outside impl so usable without Self scoping issues during compilation ordering)
const VERBOSE_HELP: &str = concat!(
    "MeshBBS Extended Help\n",
    "Authentication:\n  REGISTER <name> <pass>  Create account\n  LOGIN <name> <pass>     Log in\n  SETPASS <new>           Set first password\n  CHPASS <old> <new>      Change password\n  LOGOUT                  End session\n\n",
    "Messages & Areas:\n  AREAS / LIST            List areas\n  READ <area>             Read recent messages\n  POST <area> <text>      Post inline\n  POST then multiline '.' End with '.' line\n  DELETE <area> <id>      (mod) Delete message\n  LOCK/UNLOCK <area>      (mod) Lock area\n\n",
    "Navigation Shortcuts:\n  M   Message areas menu\n  U   User menu\n  Q   Quit\n  B   Back to previous menu\n\n",
    "User Info / Admin:\n  PROMOTE <user>          (sysop) Raise to moderator\n  DEMOTE <user>           (sysop) Lower to base user\n  DELLOG [page]           (mod) Deletion log\n  ADMINLOG [page]         (mod) Admin audit log\n  USERS [pattern]         (mod) List users, optional search\n  WHO                     (mod) Show logged in users\n  USERINFO <user>         (mod) Detailed user info\n  SESSIONS                (mod) List all sessions\n  KICK <user>             (mod) Force logout user\n  BROADCAST <msg>         (mod) Message all users\n  ADMIN/DASHBOARD         (mod) System overview\n\n",
    "Misc:\n  HELP        Compact help\n  HELP+ / HELP V  Verbose help (this)\n  Weather (public)  Send WEATHER on public channel\n\n",
    "Limits:\n  Max frame ~230 bytes; verbose help auto-splits.\n"
);

fn chunk_verbose_help() -> Vec<String> {
    const MAX: usize = 230;
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in VERBOSE_HELP.lines() {
        let candidate_len = current.as_bytes().len() + line.as_bytes().len() + 1;
        if candidate_len > MAX && !current.is_empty() {
            chunks.push(current);
            current = String::new();
        }
        current.push_str(line); current.push('\n');
    }
    if !current.is_empty() { chunks.push(current); }
    chunks
}

impl BbsServer {
    /// Create a new BBS server instance
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
        let mut storage = {
            use argon2::Params;
            if let Some(sec) = &config.security {
                if let Some(a) = &sec.argon2 {
                let builder = Params::DEFAULT;
                // Params::new(memory, time, parallelism, output_length) -> Result
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
        
        // Populate area level map from config.message_areas
        let mut level_map = std::collections::HashMap::new();
        for (k,v) in &config.message_areas { level_map.insert(k.clone(), (v.read_level, v.post_level)); }
        storage.set_area_levels(level_map);
    // Apply max message size clamp (protocol cap 230 bytes)
    storage.set_max_message_bytes(config.storage.max_message_size);

        Ok(BbsServer {
            config,
            storage,
            device: None,
            sessions: HashMap::new(),
            message_tx: None,
            public_state: PublicState::new(
                std::time::Duration::from_secs(20),
                std::time::Duration::from_secs(300)
            ),
            public_parser: PublicCommandParser::new(),
            #[cfg(feature = "weather")]
            weather_cache: None,
            #[cfg(feature = "weather")]
            weather_last_poll: Instant::now() - Duration::from_secs(301), // trigger immediate initial refresh
            #[cfg(feature = "meshtastic-proto")]
            pending_direct: Vec::new(),
            test_messages: Vec::new(),
            #[cfg(any(test, feature = "meshtastic-proto"))]
            last_banner: None,
        })
    }

    /// Connect to a Meshtastic device
    pub async fn connect_device(&mut self, port: &str) -> Result<()> {
        info!("Connecting to Meshtastic device on {}", port);
        
        let device = MeshtasticDevice::new(port, self.config.meshtastic.baud_rate).await?;
        self.device = Some(device);
        
        Ok(())
    }

    /// Start the BBS server main loop
    pub async fn run(&mut self) -> Result<()> {
        info!("BBS '{}' started by {}", self.config.bbs.name, self.config.bbs.sysop);
        self.seed_sysop().await?;
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.message_tx = Some(tx);
        // Heartbeat / want_config scheduling state (only meaningful with meshtastic-proto feature)
        #[cfg(feature = "meshtastic-proto")] 
        let mut last_hb = Instant::now();
        #[cfg(feature = "meshtastic-proto")] 
        let start_instant = Instant::now();
        #[cfg(feature = "meshtastic-proto")] 
    let ascii_lines: usize = 0; // count legacy ASCII summaries before binary detected
        #[cfg(feature = "meshtastic-proto")] 
        let mut ascii_warned = false;
        
        // Main message processing loop
        loop {
            // Periodic handshake maintenance (heartbeat + want_config) prior to draining events to minimize latency
            #[cfg(feature = "meshtastic-proto")]
            if let Some(dev) = &mut self.device {
                // Send heartbeat every 3s until initial sync complete, afterwards every ~30s (simple heuristic)
                let hb_interval = if dev.is_config_complete() { Duration::from_secs(30) } else { Duration::from_secs(3) };
                if last_hb.elapsed() >= hb_interval {
                    if let Err(e) = dev.send_heartbeat() { debug!("heartbeat send error: {e:?}"); }
                    last_hb = Instant::now();
                }
                // Always attempt want_config (function internally rate-limits and stops once complete)
                if let Err(e) = dev.ensure_want_config() { debug!("ensure_want_config error: {e:?}"); }
            }
            // First drain any text events outside the select to avoid borrowing self across await points in same branch.
            // Drain text events first collecting them to avoid holding device borrow across awaits
            #[cfg(feature = "meshtastic-proto")]
            {
                let mut drained_events = Vec::new();
                if let Some(dev) = &mut self.device {
                    while let Some(ev) = dev.next_text_event() { drained_events.push(ev); }
                }
                for ev in drained_events { if let Err(e) = self.route_text_event(ev).await { warn!("route_text_event error: {e:?}"); } }
            }
            
            // Proactive weather refresh every 5 minutes
            #[cfg(feature = "weather")]
            if self.weather_last_poll.elapsed() >= Duration::from_secs(300) {
                let _ = self.fetch_weather().await;
                self.weather_last_poll = Instant::now();
            }

            tokio::select! {
                msg = self.receive_message() => {
                    if let Ok(Some(summary)) = msg { debug!("Legacy summary: {}", summary); }
                }
                msg = rx.recv() => {
                    if let Some(internal_msg) = msg { debug!("Processing internal message: {}", internal_msg); }
                }
                _ = tokio::signal::ctrl_c() => { info!("Received shutdown signal"); break; }
                _ = sleep(std::time::Duration::from_millis(25)) => {}
            }
            // After select loop iteration, evaluate ASCII-only heuristic warning
            #[cfg(feature = "meshtastic-proto")] {
                if let Some(dev) = &self.device {
                    if !ascii_warned && !dev.binary_detected() && start_instant.elapsed() > Duration::from_secs(8) && ascii_lines > 15 {
                        warn!("No protobuf binary frames detected after 8s ({} ASCII lines seen). Device may still be in text console mode. Ensure: meshtastic --set serial.enabled true --set serial.mode PROTO", ascii_lines);
                        ascii_warned = true;
                    }
                }
                // Flush any queued direct messages once we have our node id (after processing events & reads)
                if let Some(dev_mut) = &mut self.device {
                    if dev_mut.our_node_id().is_some() && !self.pending_direct.is_empty() {
                        let mut still_pending = Vec::new();
                        for (dest, msg) in self.pending_direct.drain(..) {
                            match dev_mut.send_text_packet(Some(dest), 0, &msg) {
                                Ok(_) => debug!("Flushed pending DM to {dest}"),
                                Err(e) => { warn!("Pending DM send to {dest} failed: {e:?}"); still_pending.push((dest, msg)); }
                            }
                        }
                        self.pending_direct = still_pending;
                    }
                }
            }
        }

        
        self.shutdown().await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn test_get_session(&self, node: &str) -> Option<&Session> { self.sessions.get(node) }

    fn logged_in_session_count(&self) -> usize {
        self.sessions.values().filter(|s| s.is_logged_in()).count()
    }

    async fn prune_idle_sessions(&mut self) {
        let timeout_min = self.config.bbs.session_timeout as i64;
        if timeout_min == 0 { return; }
        let mut to_logout = Vec::new();
        for (k,s) in &self.sessions {
            if s.is_logged_in() && s.is_inactive(timeout_min) { to_logout.push(k.clone()); }
        }
        for k in to_logout {
            // Capture username without holding mutable borrow over await
            let username = if let Some(s) = self.sessions.get(&k) { s.display_name() } else { continue };
            let _ = self.send_message(&k, "You have been logged out due to inactivity.").await;
            if let Some(s) = self.sessions.get_mut(&k) { let _ = s.logout().await; }
            info!("Session {} (user {}) logged out due to inactivity", k, username);
        }
    }

    #[allow(dead_code)]
    pub fn test_logged_in_count(&self) -> usize { self.logged_in_session_count() }
    #[allow(dead_code)]
    pub async fn test_prune_idle(&mut self) { self.prune_idle_sessions().await; }

    /// Get list of all active sessions for administrative commands
    pub fn get_active_sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }

    /// Get list of currently logged-in users for WHO command
    pub fn get_logged_in_users(&self) -> Vec<&Session> {
        self.sessions.values().filter(|s| s.is_logged_in()).collect()
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
            let _ = self.send_message(&node_id, "You have been disconnected by an administrator.").await;
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
        
        let logged_in_nodes: Vec<String> = self.sessions
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
        
        info!("Broadcast message sent to {} users by {}", sent_count, sender);
        Ok(sent_count)
    }
    #[cfg(any(test, feature = "meshtastic-proto"))]
    #[allow(dead_code)]
    pub fn last_banner(&self) -> Option<&String> { self.last_banner.as_ref() }
    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn test_messages(&self) -> &Vec<(String,String)> { &self.test_messages }
    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn test_insert_session(&mut self, session: Session) { self.sessions.insert(session.node_id.clone(), session); }

        fn build_banner(&self) -> String {
            let mut banner = format!("{}\n{}", self.config.bbs.welcome_message, self.config.bbs.description);
            if banner.as_bytes().len() > 230 { let mut truncated = banner.into_bytes(); truncated.truncate(230); banner = String::from_utf8_lossy(&truncated).to_string(); }
            if !banner.ends_with('\n') { banner.push('\n'); }
            banner
        }

        /// Prepare the login banner, recording the base (without unread line) to `last_banner`.
        /// If `unread > 0`, appends the unread message count line.
        fn prepare_login_banner(&mut self, unread: u32) -> String {
            let mut banner = self.build_banner();
            // Store the base banner before mutation so tests can assert core content
            #[cfg(any(test, feature = "meshtastic-proto"))]
            { self.last_banner = Some(banner.clone()); }
            if unread > 0 { banner.push_str(&format!("{} new messages since your last login.\n", unread)); }
            banner
        }
        /// Format the unread summary line according to spec.
        /// When unread == 0 -> "There are no new messages.\n"
        /// When unread == 1 -> "1 new message since your last login.\n"
        /// When unread > 1 -> "<n> new messages since your last login.\n"
        fn format_unread_line(unread: u32) -> String {
            match unread {
                0 => "There are no new messages.\n".to_string(),
                1 => "1 new message since your last login.\n".to_string(),
                n => format!("{} new messages since your last login.\n", n)
            }
        }

    /// Ensure sysop user exists / synchronized with config (extracted for testability)
    pub async fn seed_sysop(&mut self) -> Result<()> {
        if let Some(hash) = &self.config.bbs.sysop_password_hash {
            let sysop_name = self.config.bbs.sysop.clone();
            match self.storage.get_user(&sysop_name).await? {
                Some(mut u) => {
                    let mut needs_write = false;
                    if u.user_level < 10 { u.user_level = 10; needs_write = true; }
                    if u.password_hash.as_deref() != Some(hash.as_str()) { u.password_hash = Some(hash.clone()); needs_write = true; }
                    if needs_write {
                        let users_dir = std::path::Path::new(self.storage.base_dir()).join("users");
                        let user_file = users_dir.join(format!("{}.json", sysop_name));
                        let json_content = serde_json::to_string_pretty(&u)?;
                        tokio::fs::write(user_file, json_content).await?;
                        info!("Sysop user '{}' synchronized from config.", sysop_name);
                    }
                }
                None => {
                    let now = chrono::Utc::now();
                    let user = crate::storage::User {
                        username: sysop_name.clone(),
                        node_id: None,
                        user_level: 10,
                        password_hash: Some(hash.clone()),
                        first_login: now,
                        last_login: now,
                        total_messages: 0,
                        welcome_shown_on_registration: true,  // Sysop doesn't need welcome messages
                        welcome_shown_on_first_login: true,
                    };
                    let users_dir = std::path::Path::new(self.storage.base_dir()).join("users");
                    tokio::fs::create_dir_all(&users_dir).await?;
                    let user_file = users_dir.join(format!("{}.json", sysop_name));
                    let json_content = serde_json::to_string_pretty(&user)?;
                    tokio::fs::write(user_file, json_content).await?;
                    info!("Sysop user '{}' created from config.", sysop_name);
                }
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
    pub async fn test_register(&mut self, username: &str, pass: &str) -> Result<()> { self.storage.register_user(username, pass, None).await }
    #[allow(dead_code)]
    pub async fn test_update_level(&mut self, username: &str, lvl: u8) -> Result<()> { if username == self.config.bbs.sysop { return Err(anyhow::anyhow!("Cannot modify sysop level")); } self.storage.update_user_level(username, lvl, "test").await.map(|_| ()) }
    #[allow(dead_code)]
    pub async fn test_store_message(&mut self, area: &str, author: &str, content: &str) -> Result<String> { self.storage.store_message(area, author, content).await }
    #[allow(dead_code)]
    pub async fn test_get_messages(&self, area: &str, limit: usize) -> Result<Vec<crate::storage::Message>> { self.storage.get_messages(area, limit).await }
    #[allow(dead_code)]
    pub fn test_is_locked(&self, area: &str) -> bool { self.storage.is_area_locked(area) }
    #[allow(dead_code)]
    pub async fn test_deletion_page(&self, page: usize, size: usize) -> Result<Vec<crate::storage::DeletionAuditEntry>> { self.storage.get_deletion_audit_page(page, size).await }
    // (duplicate definition removed; consolidated above)

    // Moderator / sysop internal helpers
    pub async fn moderator_delete_message(&mut self, area: &str, id: &str, actor: &str) -> Result<bool> {
        let deleted = self.storage.delete_message(area, id).await?;
        if deleted {
            sec_log!("DELETE by {}: {}/{}", actor, area, id);
            // Fire and forget audit append; if it fails, surface as error to caller
            self.storage.append_deletion_audit(area, id, actor).await?;
        }
        Ok(deleted)
    }
    pub async fn moderator_lock_area(&mut self, area: &str, actor: &str) -> Result<()> {
        self.storage.lock_area_persist(area).await?;
        sec_log!("LOCK by {}: {}", actor, area);
        Ok(())
    }
    pub async fn moderator_unlock_area(&mut self, area: &str, actor: &str) -> Result<()> {
        self.storage.unlock_area_persist(area).await?;
        sec_log!("UNLOCK by {}: {}", actor, area);
        Ok(())
    }


    /// Receive a message from the Meshtastic device
    async fn receive_message(&mut self) -> Result<Option<String>> {
        if let Some(ref mut device) = self.device {
            device.receive_message().await
        } else {
            // No device connected, wait a bit
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            Ok(None)
        }
    }


    #[cfg(all(feature = "meshtastic-proto"))]
    #[cfg_attr(test, allow(dead_code))]
    pub async fn route_text_event(&mut self, ev: TextEvent) -> Result<()> {
        // Trace-log every text event for debugging purposes
    trace!("TextEvent BEGIN src={} direct={} channel={:?} content='{}'", ev.source, ev.is_direct, ev.channel, ev.content);
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
                        trace!("Auto-applying pending public login '{}' to new DM session {}", username, node_key);
                        session.login(username.clone(), 1).await?;
                        if let Ok(Some(user_before)) = self.storage.get_user(&username).await {
                            let prev_last = user_before.last_login;
                            let unread = self.storage.count_messages_since(prev_last).await.unwrap_or(0);
                            let _ = self.storage.record_user_login(&username).await; // update last_login
                            let summary = Self::format_unread_line(unread);
                            let _ = self.send_session_message(&node_key, &format!("Welcome, {} you are now logged in.\n{}", username, summary), true).await;
                        } else {
                            // New user case shouldn't normally happen here, fallback to zero unread
                            let summary = Self::format_unread_line(0);
                            let _ = self.send_session_message(&node_key, &format!("Welcome, {} you are now logged in.\n{}", username, summary), true).await;
                        }
                    }
                } else {
                    // For non-auth first messages, show banner immediately. If message is auth command it will be processed below and produce its own reply.
                    let first = ev.content.trim();
                    let upper_first = first.to_uppercase();
                    if !(upper_first.starts_with("LOGIN ") || upper_first.starts_with("REGISTER ")) {
                        let banner = self.prepare_login_banner(0);
                        let guidance = "Use REGISTER <name> <pass> to create an account or LOGIN <name> <pass>. Type HELP for basics.";
                        let _ = self.send_message(&node_key, &format!("{}{}\n", banner, guidance)).await;
                    }
                }
                self.sessions.insert(node_key.clone(), session);
            }
                // New consolidated DM command handling with max_users and idle pruning
                self.prune_idle_sessions().await; // always prune first
                let raw_content = ev.content.trim().to_string();
                let upper = raw_content.to_uppercase();
                // Count current logged in sessions (excluding the session for this node if it is not yet logged in)
                let logged_in_count = self.sessions.values().filter(|s| s.is_logged_in()).count();
                enum PostAction { None, Delete{area:String,id:String,actor:String}, Lock{area:String,actor:String}, Unlock{area:String,actor:String}, Broadcast{message:String,sender:String} }
                let mut post_action = PostAction::None;
                let mut deferred_reply: Option<String> = None;
                if let Some(session) = self.sessions.get_mut(&node_key) {
                    session.update_activity();
                    #[cfg(feature = "meshtastic-proto")]
                    if let (Some(dev), Ok(idnum)) = (&self.device, node_key.parse::<u32>()) {
                        let (short,long) = dev.format_node_combined(idnum);
                        session.update_labels(Some(short), Some(long));
                    }
                    if upper == "HELP+" || upper == "HELP V" || upper == "HELP  V" || upper == "HELP  +" { // tolerate minor spacing variants
                        let chunks = chunk_verbose_help();
                        let total = chunks.len();
                        for (i, chunk) in chunks.into_iter().enumerate() {
                            let last = i + 1 == total;
                            // For multi-part help, suppress prompt until final
                            self.send_session_message(&node_key, &chunk, last).await?;
                        }
                    } else if upper.starts_with("REGISTER ") {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 3 { deferred_reply = Some("Usage: REGISTER <username> <password>\n".into()); }
                        else {
                            let user = parts[1]; let pass = parts[2];
                            if pass.len() < 8 { deferred_reply = Some("Password too short (minimum 8 characters).\n".into()); }
                            else {
                                match self.storage.register_user(user, pass, Some(&node_key)).await {
                                    Ok(_) => { 
                                        session.login(user.to_string(), 1).await?; 
                                        let summary = Self::format_unread_line(0); 
                                        let welcome_msg = format!("\n🎉 Welcome to MeshBBS, {}! Here's a quick start guide:\n\n• Type 'HELP' to see all available commands\n• Type 'LIST' to browse message boards\n• Type 'POST <board_number> <subject>' to create a new post\n• Type 'READ <board_number>' to read messages\n• Type 'WHO' to see who else is online\n\nEnjoy exploring the mesh network BBS!\n", user);
                                        deferred_reply = Some(format!("Registered and logged in as {}.\nWelcome, {} you are now logged in.\n{}{}", user, user, summary, welcome_msg));
                                        // Mark welcome message as shown
                                        if let Err(e) = self.storage.mark_welcome_shown(user, true, false).await {
                                            eprintln!("Failed to mark welcome shown for {}: {}", user, e);
                                        }
                                    }
                                    Err(e) => { deferred_reply = Some(format!("Register failed: {}\n", e)); }
                                }
                            }
                        }
                    } else if upper.starts_with("LOGIN ") {
                        // Enforce max_users only if this session is not yet logged in
                        if !session.is_logged_in() && (logged_in_count as u32) >= self.config.bbs.max_users {
                            deferred_reply = Some("All available sessions are in use, please wait and try again later.\n".into());
                        } else if session.is_logged_in() {
                            deferred_reply = Some(format!("Already logged in as {}.\n", session.display_name()));
                        } else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: LOGIN <username> [password]\n".into()); }
                            else {
                                let user = parts[1];
                                let password_opt = if parts.len() >= 3 { Some(parts[2]) } else { None };
                                match self.storage.get_user(user).await? {
                                    None => deferred_reply = Some("No such user. Use REGISTER <u> <p>.\n".into()),
                                    Some(u) => {
                                        let has_password = u.password_hash.is_some();
                                        let node_bound = u.node_id.as_deref() == Some(&node_key);
                                        if !has_password {
                                            // User must set a password on first login attempt
                                            if let Some(pass) = password_opt {
                                                if pass.len() < 8 { deferred_reply = Some("Password too short (minimum 8 characters).\n".into()); }
                                                else {
                                                    let updated_user = self.storage.set_user_password(user, pass).await?;
                                                    let updated = if !node_bound { self.storage.bind_user_node(user, &node_key).await? } else { updated_user };
                                                    session.login(updated.username.clone(), updated.user_level).await?;
                                                    // First-time password set; unread messages prior to this first authenticated login are based on prior last_login value.
                                                    // set_user_password already bumped last_login, so computing unread would yield zero. This is acceptable; show none.
                                                    let _ = self.storage.record_user_login(user).await; // ensure fresh timestamp after full login
                                                    // No unread count expected here (legacy first login)
                                                    let summary = Self::format_unread_line(0); // first login after setting password shows no unread
                                                    
                                                    // Check if this is the first login after registration and show follow-up welcome
                                                    let mut login_msg = format!("Password set. Welcome, {} you are now logged in.\n{}", updated.username, summary);
                                                    if updated.welcome_shown_on_registration && !updated.welcome_shown_on_first_login {
                                                        login_msg.push_str("\n💡 Quick tip: Since this is your first time back, try these commands:\n• 'LIST' - Browse available message boards\n• 'WHO' - See who's currently online\n• 'RECENT' - Check the latest activity\n\nHappy posting!\n");
                                                        // Mark first login welcome as shown
                                                        if let Err(e) = self.storage.mark_welcome_shown(user, false, true).await {
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
                                            if password_opt.is_none() { deferred_reply = Some("Password required: LOGIN <user> <pass>\n".into()); }
                                            else {
                                                let pass = password_opt.unwrap();
                                                let (_maybe, ok) = self.storage.verify_user_password(user, pass).await?;
                                                if !ok { deferred_reply = Some("Invalid password.\n".into()); }
                                                else {
                                                    let updated = if !node_bound { self.storage.bind_user_node(user, &node_key).await? } else { u };
                                                    session.login(updated.username.clone(), updated.user_level).await?;
                                                    let prev_last = updated.last_login; // captured before we update last_login again
                                                    let unread = self.storage.count_messages_since(prev_last).await.unwrap_or(0);
                                                    let updated2 = self.storage.record_user_login(user).await.unwrap_or(updated);
                                                    let summary = Self::format_unread_line(unread);
                                                    
                                                    // Check if this is the first login after registration and show follow-up welcome
                                                    let mut login_msg = format!("Welcome, {} you are now logged in.\n{}", updated2.username, summary);
                                                    if updated2.welcome_shown_on_registration && !updated2.welcome_shown_on_first_login {
                                                        login_msg.push_str("\n💡 Quick tip: Since this is your first time back, try these commands:\n• 'LIST' - Browse available message boards\n• 'WHO' - See who's currently online\n• 'RECENT' - Check the latest activity\n\nHappy posting!\n");
                                                        // Mark first login welcome as shown
                                                        if let Err(e) = self.storage.mark_welcome_shown(user, false, true).await {
                                                            eprintln!("Failed to mark first login welcome shown for {}: {}", user, e);
                                                        }
                                                    }
                                                    deferred_reply = Some(login_msg);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("CHPASS ") {
                        if session.username.as_deref() == Some(&self.config.bbs.sysop) {
                            deferred_reply = Some("Sysop password managed externally. Use sysop-passwd CLI.\n".into());
                        } else if session.is_logged_in() {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 3 { deferred_reply = Some("Usage: CHPASS <old> <new>\n".into()); }
                            else {
                                let old = parts[1]; let newp = parts[2];
                                if newp.len() < 8 { deferred_reply = Some("New password too short (min 8).\n".into()); }
                                else if newp.len() > 128 { deferred_reply = Some("New password too long.\n".into()); }
                                else if let Some(user_name) = &session.username {
                                    match self.storage.get_user(user_name).await? {
                                        Some(u) => {
                                            if u.password_hash.is_none() { deferred_reply = Some("No existing password. Use SETPASS <new>.\n".into()); }
                                            else {
                                                let (_u2, ok) = self.storage.verify_user_password(user_name, old).await?;
                                                if !ok { deferred_reply = Some("Invalid password.\n".into()); }
                                                else if old == newp { deferred_reply = Some("New password must differ.\n".into()); }
                                                else { self.storage.update_user_password(user_name, newp).await?; deferred_reply = Some("Password changed.\n".into()); }
                                            }
                                        }
                                        None => deferred_reply = Some("Session user missing.\n".into())
                                    }
                                } else { deferred_reply = Some("Not logged in.\n".into()); }
                            }
                        } else { deferred_reply = Some("Not logged in.\n".into()); }
                    } else if upper.starts_with("SETPASS ") {
                        if session.username.as_deref() == Some(&self.config.bbs.sysop) {
                            deferred_reply = Some("Sysop password managed externally. Use sysop-passwd CLI.\n".into());
                        } else if session.is_logged_in() {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: SETPASS <new>\n".into()); }
                            else {
                                let newp = parts[1];
                                if newp.len() < 8 { deferred_reply = Some("New password too short (min 8).\n".into()); }
                                else if newp.len() > 128 { deferred_reply = Some("New password too long.\n".into()); }
                                else if let Some(user_name) = &session.username {
                                    match self.storage.get_user(user_name).await? {
                                        Some(u) => {
                                            if u.password_hash.is_some() { deferred_reply = Some("Password already set. Use CHPASS <old> <new>.\n".into()); }
                                            else { self.storage.update_user_password(user_name, newp).await?; deferred_reply = Some("Password set.\n".into()); }
                                        }
                                        None => deferred_reply = Some("Session user missing.\n".into())
                                    }
                                } else { deferred_reply = Some("Not logged in.\n".into()); }
                            }
                        } else { deferred_reply = Some("Not logged in.\n".into()); }
                    } else if upper.starts_with("PROMOTE ") {
                        if session.username.as_deref() != Some(&self.config.bbs.sysop) { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: PROMOTE <user>\n".into()); }
                            else {
                                let target = parts[1];
                                match self.storage.get_user(target).await? {
                                    None => deferred_reply = Some("User not found.\n".into()),
                                    Some(u) => {
                                        if u.username == self.config.bbs.sysop { deferred_reply = Some("Cannot modify sysop.\n".into()); }
                                        else if u.user_level >= LEVEL_MODERATOR { deferred_reply = Some("Already moderator or higher.\n".into()); }
                                        else { self.storage.update_user_level(&u.username, LEVEL_MODERATOR, session.username.as_deref().unwrap_or("unknown")).await?; deferred_reply = Some(format!("{} promoted to {}.\n", u.username, role_name(LEVEL_MODERATOR))); }
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("DEMOTE ") {
                        if session.username.as_deref() != Some(&self.config.bbs.sysop) { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: DEMOTE <user>\n".into()); }
                            else {
                                let target = parts[1];
                                match self.storage.get_user(target).await? {
                                    None => deferred_reply = Some("User not found.\n".into()),
                                    Some(u) => {
                                        if u.username == self.config.bbs.sysop { deferred_reply = Some("Cannot modify sysop.\n".into()); }
                                        else if u.user_level <= LEVEL_USER { deferred_reply = Some("Already at base level.\n".into()); }
                                        else { self.storage.update_user_level(&u.username, LEVEL_USER, session.username.as_deref().unwrap_or("unknown")).await?; deferred_reply = Some(format!("{} demoted to {}.\n", u.username, role_name(LEVEL_USER))); }
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("DELETE ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 3 { deferred_reply = Some("Usage: DELETE <area> <id>\n".into()); }
                            else {
                                let area = parts[1].to_lowercase();
                                let id = parts[2].to_string();
                                let actor = session.username.clone().unwrap_or("?".into());
                                post_action = PostAction::Delete{area,id,actor};
                            }
                        }
                    } else if upper.starts_with("LOCK ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: LOCK <area>\n".into()); }
                            else {
                                let area = parts[1].to_lowercase();
                                let actor = session.username.clone().unwrap_or("?".into());
                                post_action = PostAction::Lock{area:area.clone(), actor};
                                deferred_reply = Some(format!("Area {} locked.\n", area));
                            }
                        }
                    } else if upper.starts_with("UNLOCK ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: UNLOCK <area>\n".into()); }
                            else {
                                let area = parts[1].to_lowercase();
                                let actor = session.username.clone().unwrap_or("?".into());
                                post_action = PostAction::Unlock{area:area.clone(), actor};
                                deferred_reply = Some(format!("Area {} unlocked.\n", area));
                            }
                        }
                    } else if upper.starts_with("DELLOG") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            let page = if parts.len() >= 2 { parts[1].parse::<usize>().unwrap_or(1) } else { 1 };
                            match self.storage.get_deletion_audit_page(page, 10).await {
                                Ok(entries) => {
                                    if entries.is_empty() { deferred_reply = Some("No entries.\n".into()); }
                                    else { let mut out = String::from("Deletion Log:\n"); for e in entries { out.push_str(&format!("{} {} {} {}\n", e.timestamp, e.actor, e.area, e.id)); } deferred_reply = Some(out); }
                                }
                                Err(e) => deferred_reply = Some(format!("Failed: {}\n", e)),
                            }
                        }
                    } else if upper.starts_with("ADMINLOG") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            let page = if parts.len() >= 2 { parts[1].parse::<usize>().unwrap_or(1) } else { 1 };
                            match self.storage.get_admin_audit_page(page, 10).await {
                                Ok(entries) => {
                                    if entries.is_empty() { deferred_reply = Some("No admin audit entries.\n".into()); }
                                    else { 
                                        let mut out = String::from("Admin Audit Log:\n");
                                        for e in entries {
                                            let target_str = e.target.as_deref().unwrap_or("-");
                                            let details_str = e.details.as_deref().unwrap_or("");
                                            out.push_str(&format!("{} {} {} {} {}\n", 
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
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            let pattern = if parts.len() >= 2 { Some(parts[1].to_lowercase()) } else { None };
                            
                            match self.storage.list_all_users().await {
                                Ok(mut users) => {
                                    // Filter users by pattern if provided
                                    if let Some(ref p) = pattern {
                                        users.retain(|u| u.username.to_lowercase().contains(p));
                                    }
                                    
                                    let logged_in_usernames: std::collections::HashSet<&str> = self.get_logged_in_users()
                                        .iter()
                                        .filter_map(|s| s.username.as_deref())
                                        .collect();
                                    
                                    let mut response = if let Some(ref p) = pattern {
                                        format!("Users matching '{}' ({} found):\n", p, users.len())
                                    } else {
                                        format!("Registered Users ({}/{}):\n", users.len(), self.config.bbs.max_users)
                                    };
                                    
                                    for user in users {
                                        let status = if logged_in_usernames.contains(user.username.as_str()) { "Online" } else { "Offline" };
                                        let role = super::roles::role_name(user.user_level);
                                        response.push_str(&format!("  {} ({}, Level {}) - {}\n", user.username, role, user.user_level, status));
                                    }
                                    
                                    if pattern.is_none() {
                                        response.push_str(&format!("\nActive Sessions: {}\n", self.logged_in_session_count()));
                                    }
                                    deferred_reply = Some(response);
                                }
                                Err(e) => deferred_reply = Some(format!("Failed to list users: {}\n", e)),
                            }
                        }
                    } else if upper == "WHO" {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
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
                                        super::session::SessionState::MessageAreas => "Message Areas",
                                        super::session::SessionState::ReadingMessages => "Reading",
                                        super::session::SessionState::PostingMessage => "Posting",
                                        super::session::SessionState::UserMenu => "User Menu",
                                        _ => "Other",
                                    };
                                    response.push_str(&format!("  {} ({}) - {} - {}m - {}\n", username, role, session.node_id, duration, state));
                                }
                                deferred_reply = Some(response);
                            }
                        }
                    } else if upper.starts_with("USERINFO ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: USERINFO <user>\n".into()); }
                            else {
                                let target = parts[1];
                                match self.storage.get_user_details(target).await? {
                                    None => deferred_reply = Some("User not found.\n".into()),
                                    Some(user) => {
                                        let post_count = self.storage.count_user_posts(&user.username).await.unwrap_or(0);
                                        let is_online = self.get_logged_in_users().iter().any(|s| s.username.as_deref() == Some(&user.username));
                                        let role = super::roles::role_name(user.user_level);
                                        
                                        let mut response = format!("User Information for {}:\n", user.username);
                                        response.push_str(&format!("  Role: {} (Level {})\n", role, user.user_level));
                                        response.push_str(&format!("  Status: {}\n", if is_online { "Online" } else { "Offline" }));
                                        response.push_str(&format!("  First Login: {}\n", user.first_login.format("%Y-%m-%d %H:%M UTC")));
                                        response.push_str(&format!("  Last Login: {}\n", user.last_login.format("%Y-%m-%d %H:%M UTC")));
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
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
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
                                    super::session::SessionState::MessageAreas => "Message Areas",
                                    super::session::SessionState::ReadingMessages => "Reading",
                                    super::session::SessionState::PostingMessage => "Posting",
                                    super::session::SessionState::UserMenu => "User Menu",
                                    super::session::SessionState::Disconnected => "Disconnected",
                                };
                                response.push_str(&format!("  {} ({}) | {} | {}m | Login: {} | {}\n", 
                                    username, role, s.node_id, duration, logged_in, state));
                            }
                            deferred_reply = Some(response);
                        }
                    } else if upper.starts_with("KICK ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: KICK <user>\n".into()); }
                            else {
                                let target = parts[1];
                                let actor = session.username.as_deref().unwrap_or("unknown").to_string();
                                if target == actor {
                                    deferred_reply = Some("Cannot kick yourself.\n".into());
                                } else if target == self.config.bbs.sysop {
                                    deferred_reply = Some("Cannot kick sysop.\n".into());
                                } else {
                                    match self.force_logout_user(target).await? {
                                        true => {
                                            // Log the administrative action
                                            if let Err(e) = self.storage.log_admin_action("KICK", Some(target), &actor, None).await {
                                                warn!("Failed to log admin action: {}", e);
                                            }
                                            deferred_reply = Some(format!("User {} has been kicked.\n", target));
                                        },
                                        false => deferred_reply = Some("User not found or not logged in.\n".into()),
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("BROADCAST ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let message = raw_content.strip_prefix("BROADCAST ").unwrap_or("").trim();
                            if message.is_empty() { deferred_reply = Some("Usage: BROADCAST <message>\n".into()); }
                            else {
                                let sender = session.username.as_deref().unwrap_or("System").to_string();
                                let message = message.to_string();
                                post_action = PostAction::Broadcast{message, sender};
                            }
                        }
                    } else if upper == "ADMIN" || upper == "DASHBOARD" {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n".into()); }
                        else {
                            let stats = self.storage.get_statistics().await?;
                            let active_count = self.get_active_sessions().len();
                            let logged_in_count = self.logged_in_session_count();
                            
                            let mut response = String::from("=== ADMINISTRATIVE DASHBOARD ===\n");
                            response.push_str(&format!("System Status:\n"));
                            response.push_str(&format!("  Total Users: {}\n", stats.total_users));
                            response.push_str(&format!("  Total Messages: {}\n", stats.total_messages));
                            response.push_str(&format!("  Active Sessions: {}\n", active_count));
                            response.push_str(&format!("  Logged In Users: {}\n", logged_in_count));
                            response.push_str(&format!("  Max Users: {}\n", self.config.bbs.max_users));
                            response.push_str(&format!("  Session Timeout: {} min\n", self.config.bbs.session_timeout));
                            response.push_str(&format!("\nCommands: USERS, WHO, USERINFO <user>, SESSIONS, KICK <user>, BROADCAST <msg>\n"));
                            deferred_reply = Some(response);
                        }
                    } else if upper == "LOGOUT" {
                        if session.is_logged_in() { let name = session.display_name(); session.logout().await?; deferred_reply = Some(format!("User {} logged out.\n", name)); }
                        else { deferred_reply = Some("Not logged in.\n".into()); }
                    } else if upper == "HELP" || upper == "?" || upper == "H" {
                        // Use existing abbreviated help via command processor (ensures consistent text) and include shortcuts line first time
                        let mut help_text = session.process_command("HELP", &mut self.storage).await?;
                        if !session.help_seen {
                            session.help_seen = true;
                            help_text.push_str("Shortcuts: M=areas U=user Q=quit\n");
                        }
                        self.send_session_message(&node_key, &help_text, true).await?;
                    } else {
                        let redact = ["REGISTER ", "LOGIN ", "SETPASS ", "CHPASS "];
                        let log_snippet = if redact.iter().any(|p| upper.starts_with(p)) { "<redacted>" } else { raw_content.as_str() };
                        trace!("Session {} generic command '{}'", node_key, log_snippet);
                        let response = session.process_command(&raw_content, &mut self.storage).await?;
                        if !response.is_empty() { deferred_reply = Some(response); }
                    }
                }
                match post_action {
                    PostAction::None => {}
                    PostAction::Delete{area,id,actor} => {
                        match self.moderator_delete_message(&area, &id, &actor).await {
                            Ok(true) => { deferred_reply.get_or_insert(format!("Deleted message {} in {}.\n", id, area)); },
                            Ok(false) => { deferred_reply.get_or_insert("Not found.\n".into()); },
                            Err(e) => { deferred_reply.get_or_insert(format!("Delete failed: {}\n", e)); }
                        }
                    }
                    PostAction::Lock{area,actor} => {
                        if let Err(e) = self.moderator_lock_area(&area, &actor).await { deferred_reply.get_or_insert(format!("Lock failed: {}\n", e)); }
                    }
                    PostAction::Unlock{area,actor} => {
                        if let Err(e) = self.moderator_unlock_area(&area, &actor).await { deferred_reply.get_or_insert(format!("Unlock failed: {}\n", e)); }
                    }
                    PostAction::Broadcast{message,sender} => {
                        match self.broadcast_message(&message, &sender).await {
                            Ok(0) => { deferred_reply.get_or_insert("No users online to receive broadcast.\n".into()); },
                            Ok(count) => { 
                                // Log the administrative action
                                if let Err(e) = self.storage.log_admin_action("BROADCAST", None, &sender, Some(&message)).await {
                                    warn!("Failed to log admin action: {}", e);
                                }
                                deferred_reply.get_or_insert(format!("Broadcast sent to {} users.\n", count)); 
                            },
                            Err(e) => { deferred_reply.get_or_insert(format!("Broadcast failed: {}\n", e)); }
                        }
                    }
                }
                if let Some(msg) = deferred_reply { self.send_session_message(&node_key, &msg, true).await?; }
            // end direct path handling (removed extra closing brace)
        } else {
            // Public channel event: parse lightweight commands
            self.public_state.prune_expired();
            let cmd = self.public_parser.parse(&ev.content);
            trace!("Public command parse result for node {} => {:?}", node_key, cmd);
            match cmd {
                PublicCommand::Help => {
                    if self.public_state.should_reply(&node_key) {
                        // Compose public notice and detailed DM help
                        // Prefer a friendly node label (short label) if the protobuf node catalog knows it.
                        // Support node keys provided either as plain decimal or hex with 0x prefix.
                        #[cfg(feature = "meshtastic-proto")]
                        let friendly = if let Some(dev) = &self.device {
                            // Attempt to parse node id in decimal first, then hex (0x...)
                            let id_opt = if let Ok(id_dec) = node_key.parse::<u32>() {
                                Some(id_dec)
                            } else if let Some(hex) = node_key.strip_prefix("0x").or_else(|| node_key.strip_prefix("0X")) {
                                u32::from_str_radix(hex, 16).ok()
                            } else { None };
                            if let Some(id) = id_opt { dev.format_node_short_label(id) } else { node_key.clone() }
                        } else { node_key.clone() };
                        #[cfg(not(feature = "meshtastic-proto"))]
                        let friendly = node_key.clone();
                        let public_notice = format!("[{}] - please check your DM's for {} help", friendly, self.config.bbs.name);
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            if let Some(dev) = &mut self.device {
                                if let Err(e) = dev.send_text_packet(None, 0, &public_notice) { warn!("Public HELP broadcast failed: {e:?}"); }
                            }
                        }
                        // Always attempt to DM help (direct reply) so user gets instructions
                        let dm_help = "Help: LOGIN <name> to begin. After login via DM you can: LIST AREAS | READ <area> | POST <area>. Type HELP anytime.";
                        let mut dm_ok = false;
                        // Prefer direct protobuf send if device present
                        #[cfg(feature = "meshtastic-proto")]
                        if let Some(dev) = &mut self.device {
                            if dev.our_node_id().is_some() {
                                if let Err(e) = dev.send_text_packet(Some(ev.source), 0, dm_help) {
                                    warn!("Direct DM help send failed via protobuf path: {e:?}");
                                } else { dm_ok = true; }
                            } else {
                                // Queue until our node id known to ensure proper from field
                                self.pending_direct.push((ev.source, dm_help.to_string()));
                                debug!("Queued HELP DM for {} until our node id known", ev.source);
                                dm_ok = true; // treat as handled (queued)
                            }
                        }
                        if !dm_ok {
                            if let Err(e) = self.send_message(&node_key, dm_help).await {
                                warn!("Fallback DM help send failed: {e:?}");
                            } else {
                                debug!("Sent HELP DM to {} via fallback path", node_key);
                            }
                        } else {
                            debug!("Sent HELP DM to {} via protobuf path", node_key);
                        }
                    }
                }
                PublicCommand::Login(username) => {
                    if self.public_state.should_reply(&node_key) {
                        self.public_state.set_pending(&node_key, username.clone());
                        let reply = format!("Login pending for '{}'. Open a direct message to this node and say HI or LOGIN <name> again to complete.", username);
                        self.send_message(&node_key, &reply).await?;
                    }
                }
                PublicCommand::Weather => {
                    if self.public_state.should_reply(&node_key) {
                        let weather = self.fetch_weather().await.unwrap_or_else(|| "Weather unavailable".to_string());
                        let mut broadcasted = false;
                        #[cfg(feature = "meshtastic-proto")]
                        {
                            if let Some(dev) = &mut self.device {
                                match dev.send_text_packet(None, 0, &weather) {
                                    Ok(_) => {
                                        trace!("Broadcasted weather to public channel: '{}'", weather);
                                        broadcasted = true;
                                    }
                                    Err(e) => {
                                        warn!("Weather broadcast failed: {e:?} (will fallback DM)");
                                    }
                                }
                            } else {
                                debug!("No device connected; cannot broadcast weather, will fallback DM");
                            }
                        }
                        if !broadcasted {
                            // Fallback: send as direct message so user gets feedback instead of silence
                            let reply = format!("Weather: {}", weather);
                            if let Err(e) = self.send_message(&node_key, &reply).await { warn!("Weather DM fallback failed: {e:?}"); }
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
    async fn send_message(&mut self, to_node: &str, message: &str) -> Result<()> {
        if let Some(ref mut device) = self.device {
            device.send_message(to_node, message).await?;
            debug!("Sent message to {}: {}", to_node, message);
        } else {
            warn!("No device connected, cannot send message");
        }
        self.test_messages.push((to_node.to_string(), message.to_string()));
        Ok(())
    }

    /// Send a session-scoped reply, automatically appending a dynamic prompt unless suppressed.
    /// If chunked is true and not last_chunk, no prompt is appended (used for future multi-part HELP+).
    async fn send_session_message(&mut self, node_key: &str, body: &str, last_chunk: bool) -> Result<()> {
        // Retrieve session (if missing, just send body)
        let msg = if let Some(session) = self.sessions.get(node_key) {
            if last_chunk {
                let prompt = session.build_prompt();
                if body.ends_with('\n') { format!("{}{}", body, prompt) } else { format!("{}\n{}", body, prompt) }
            } else {
                body.to_string()
            }
        } else { body.to_string() };
        self.send_message(node_key, &msg).await
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
                let chunks = chunk_verbose_help();
                let total = chunks.len();
                for (i, chunk) in chunks.into_iter().enumerate() { let last = i + 1 == total; self.send_session_message(node_key, &chunk, last).await?; }
                return Ok(());
            } else if upper == "HELP" || upper == "?" || upper == "H" {
                let mut help_text = session.process_command("HELP", &mut self.storage).await?;
                if !session.help_seen { session.help_seen = true; help_text.push_str("Shortcuts: M=areas U=user Q=quit\n"); }
                self.send_session_message(node_key, &help_text, true).await?;
                return Ok(());
            } else if upper.starts_with("LOGIN ") {
                if !session.is_logged_in() && (logged_in_count as u32) >= self.config.bbs.max_users { deferred_reply = Some("All available sessions are in use, please wait and try again later.\n".into()); }
                else if session.is_logged_in() { deferred_reply = Some(format!("Already logged in as {}.\n", session.display_name())); }
                else {
                    let parts: Vec<&str> = raw_content.split_whitespace().collect();
                    if parts.len() >= 2 { let user = parts[1]; session.login(user.to_string(), 1).await?; deferred_reply = Some(format!("Welcome, {} you are now logged in.\n{}", user, Self::format_unread_line(0))); }
                }
            } else {
                let response = session.process_command(&raw_content, &mut self.storage).await?;
                if !response.is_empty() { deferred_reply = Some(response); }
            }
        }
        if let Some(msg) = deferred_reply { self.send_session_message(node_key, &msg, true).await?; }
        Ok(())
    }

    #[allow(unused)]
    #[cfg(feature = "weather")]
    async fn fetch_weather(&mut self) -> Option<String> {
        use tokio::time::timeout;
        use std::time::{SystemTime, UNIX_EPOCH};
        const TTL: Duration = Duration::from_secs(5 * 60); // 5 minutes
        const MAX_STALE_AGE: Duration = Duration::from_secs(60 * 60); // 1 hour max age for stale cache
        
        // If we have a fresh cached value, return it immediately
        if let Some((ts, val)) = &self.weather_cache {
            let age = ts.elapsed();
            debug!("Weather cache check: age={:.1}min, TTL={:.1}min, MAX_STALE={:.1}min", 
                   age.as_secs_f64() / 60.0, TTL.as_secs_f64() / 60.0, MAX_STALE_AGE.as_secs_f64() / 60.0);
            
            // Failsafe: if cache is suspiciously old (>2 hours), force clear it
            if age > Duration::from_secs(2 * 60 * 60) {
                warn!("Weather cache extremely stale ({:.1} hours), forcing clear", age.as_secs_f64() / 3600.0);
                self.weather_cache = None;
            } else if age < TTL { 
                debug!("Returning fresh cached weather (age: {:.1} min)", age.as_secs_f64() / 60.0);
                return Some(val.clone()); 
            }
        }
        
        // Attempt refresh
        let location = self.config.bbs.location.trim();
        let encoded_location = location.replace(" ", "%20");
        let url = format!("https://wttr.in/{}?format=%l:+%C+%t", encoded_location);
        trace!("Fetching weather from {} (refresh)", url);
        let fut = async {
            let client = reqwest::Client::new();
            match client.get(&url).send().await {
                Ok(resp) => {
                    if !resp.status().is_success() { return None; }
                    match resp.text().await { Ok(txt) => Some(Self::sanitize_weather(&txt)), Err(_) => None }
                },
                Err(e) => { debug!("weather fetch error: {e:?}"); None }
            }
        };
        let result = match timeout(Duration::from_secs(4), fut).await { Ok(v) => v, Err(_) => None };
        match result {
            Some(v) => { 
                info!("Weather fetched successfully: {}", v.chars().take(50).collect::<String>());
                self.weather_cache = Some((Instant::now(), v.clone())); 
                Some(v) 
            },
            None => {
                // If refresh failed but we have a stale cached value, check if it's not too old
                if let Some((ts, val)) = &self.weather_cache {
                    let age = ts.elapsed();
                    if age < MAX_STALE_AGE {
                        // Add staleness indicator for old cache
                        if age > TTL {
                            debug!("Returning stale weather cache (age: {:.1} min)", age.as_secs_f64() / 60.0);
                            return Some(format!("{} (cached)", val));
                        } else {
                            return Some(val.clone());
                        }
                    } else {
                        // Cache is too old, discard it
                        warn!("Weather cache too old ({:.1} minutes), discarding", age.as_secs_f64() / 60.0);
                        self.weather_cache = None;
                    }
                }
                None
            }
        }
    }

    #[allow(unused)]
    #[cfg(not(feature = "weather"))]
    async fn fetch_weather(&mut self) -> Option<String> { None }

    fn sanitize_weather(raw: &str) -> String {
        let mut out = String::new();
        for ch in raw.chars() {
            if ch == '\n' { break; }
            if ch.is_ascii() && !ch.is_control() { out.push(ch); }
        }
        let trimmed = out.trim();
        if trimmed.is_empty() { "Weather unavailable".to_string() } else { format!("Weather: {}", trimmed) }
    }

    /// Show BBS status and statistics
    pub async fn show_status(&self) -> Result<()> {
        println!("=== MeshBBS Status ===");
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
    async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down BBS server...");
        
        // Close all sessions
        for (session_id, session) in &mut self.sessions {
            info!("Closing session: {}", session_id);
            session.logout().await?;
        }
        self.sessions.clear();
        
        // Disconnect device
        if let Some(device) = &mut self.device {
            device.disconnect().await?;
        }
        
        info!("BBS server shutdown complete");
        Ok(())
    }
}
