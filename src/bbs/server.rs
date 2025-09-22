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
use super::session::Session;
use super::public::{PublicState, PublicCommandParser, PublicCommand};

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
    #[cfg(feature = "meshtastic-proto")]
    pending_direct: Vec<(u32, String)>, // queue of (dest_node_id, message) awaiting our node id
}

impl BbsServer {
    /// Create a new BBS server instance
    pub async fn new(config: Config) -> Result<Self> {
        let storage = Storage::new(&config.storage.data_dir).await?;
        
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
            #[cfg(feature = "meshtastic-proto")]
            pending_direct: Vec::new(),
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
                // If there was a pending login, apply username now
                if let Some(username) = self.public_state.take_pending(&node_key) {
                    trace!("Applying pending public login '{}' to new DM session {}", username, node_key);
                    session.login(username, 1).await?;
                } else {
                    // Optionally send a small greeting expecting LOGIN if not logged in
                    let _ = self.send_message(&node_key, "Welcome to MeshBBS (private). Use REGISTER <name> <pass> to create an account or LOGIN <name> [pass]. Type HELP for basics.").await;
                }
                self.sessions.insert(node_key.clone(), session);
            }
            if let Some(session) = self.sessions.get_mut(&node_key) {
                #[cfg(feature = "meshtastic-proto")]
                if let (Some(dev), Ok(idnum)) = (&self.device, node_key.parse::<u32>()) {
                    let (short, long) = dev.format_node_combined(idnum);
                    session.update_labels(Some(short), Some(long));
                }
                let content = ev.content.trim();
                let upper = content.to_uppercase();
                // We'll accumulate an optional immediate reply to send after we release the session borrow
                let mut deferred_reply: Option<String> = None;
                if upper.starts_with("REGISTER ") {
                    // REGISTER <username> <password>
                    let parts: Vec<&str> = content.split_whitespace().collect();
                    if parts.len() < 3 { deferred_reply = Some("Usage: REGISTER <username> <password>\n>".to_string()); }
                    else {
                        let user = parts[1];
                        let pass = parts[2];
                        match self.storage.register_user(user, pass, Some(&node_key)).await {
                            Ok(_) => {
                                session.login(user.to_string(), 1).await?;
                                deferred_reply = Some(format!("Registered and logged in as {}.\n>", user));
                            }
                            Err(e) => { deferred_reply = Some(format!("Register failed: {}\n>", e)); }
                        }
                    }
                } else if upper.starts_with("LOGIN ") {
                    // LOGIN <username> [password]
                    trace!("Direct LOGIN attempt from node {} raw='{}'", node_key, content);
                    if session.is_logged_in() { deferred_reply = Some(format!("Already logged in as {}.\n>", session.display_name())); }
                    else {
                        let parts: Vec<&str> = content.split_whitespace().collect();
                        if parts.len() < 2 { deferred_reply = Some("Usage: LOGIN <username> [password]\n>".to_string()); }
                        else {
                            let user = parts[1];
                            let password_opt = if parts.len() >= 3 { Some(parts[2]) } else { None };
                            // Fetch user
                            match self.storage.get_user(user).await? {
                                None => { deferred_reply = Some("No such user. Use REGISTER <u> <p>.\n>".to_string()); }
                                Some(u) => {
                                    // Determine if password required
                                    let needs_password = u.password_hash.is_some();
                                    let node_bound = u.node_id.as_deref() == Some(&node_key);
                                    if needs_password && !node_bound {
                                        if password_opt.is_none() { deferred_reply = Some("Password required: LOGIN <user> <pass>\n>".to_string()); }
                                        else {
                                            let pass = password_opt.unwrap();
                                            let (_maybe, ok) = self.storage.verify_user_password(user, pass).await?;
                                            if !ok { deferred_reply = Some("Invalid password.\n>".to_string()); }
                                            else {
                                                let updated = if !node_bound { self.storage.bind_user_node(user, &node_key).await? } else { u };
                                                session.login(updated.username.clone(), updated.user_level).await?;
                                                deferred_reply = Some(format!("Welcome {}!\n>", updated.username));
                                            }
                                        }
                                    } else {
                                        // Either no password set or already bound to this node; allow login without password
                                        let updated = if !node_bound { self.storage.bind_user_node(user, &node_key).await? } else { u };
                                        session.login(updated.username.clone(), updated.user_level).await?;
                                        deferred_reply = Some(format!("Welcome {}!\n>", updated.username));
                                    }
                                }
                            }
                        }
                    }
                } else if upper.starts_with("CHPASS ") {
                    // Change existing password: CHPASS <old> <new>
                    // Do not trace raw command to avoid leaking secrets
                    if session.is_logged_in() {
                        let parts: Vec<&str> = content.split_whitespace().collect();
                        if parts.len() < 3 { deferred_reply = Some("Usage: CHPASS <old> <new>\n>".to_string()); } else {
                            let old = parts[1];
                            let newp = parts[2];
                            if newp.len() < 8 { deferred_reply = Some("New password too short (min 8).\n>".to_string()); }
                            else if newp.len() > 128 { deferred_reply = Some("New password too long.\n>".to_string()); }
                            else {
                                // Verify old password (if user has one); if no password set, instruct SETPASS instead
                                if let Some(user_name) = &session.username {
                                    match self.storage.get_user(user_name).await? {
                                        Some(u) => {
                                            if u.password_hash.is_none() { deferred_reply = Some("No existing password. Use SETPASS <new>.\n>".to_string()); }
                                            else {
                                                let (_u2, ok) = self.storage.verify_user_password(user_name, old).await?;
                                                if !ok { deferred_reply = Some("Invalid password.\n>".to_string()); }
                                                else if old == newp { deferred_reply = Some("New password must differ.\n>".to_string()); }
                                                else {
                                                    self.storage.update_user_password(user_name, newp).await?;
                                                    deferred_reply = Some("Password changed.\n>".to_string());
                                                }
                                            }
                                        }
                                        None => { deferred_reply = Some("Session user missing.\n>".to_string()); }
                                    }
                                } else { deferred_reply = Some("Not logged in.\n>".to_string()); }
                            }
                        }
                    } else { deferred_reply = Some("Not logged in.\n>".to_string()); }
                } else if upper.starts_with("SETPASS ") {
                    // Set initial password when none exists: SETPASS <new>
                    if session.is_logged_in() {
                        let parts: Vec<&str> = content.split_whitespace().collect();
                        if parts.len() < 2 { deferred_reply = Some("Usage: SETPASS <new>\n>".to_string()); }
                        else {
                            let newp = parts[1];
                            if newp.len() < 8 { deferred_reply = Some("New password too short (min 8).\n>".to_string()); }
                            else if newp.len() > 128 { deferred_reply = Some("New password too long.\n>".to_string()); }
                            else if let Some(user_name) = &session.username {
                                match self.storage.get_user(user_name).await? {
                                    Some(u) => {
                                        if u.password_hash.is_some() { deferred_reply = Some("Password already set. Use CHPASS <old> <new>.\n>".to_string()); }
                                        else {
                                            self.storage.update_user_password(user_name, newp).await?;
                                            deferred_reply = Some("Password set.\n>".to_string());
                                        }
                                    }
                                    None => deferred_reply = Some("Session user missing.\n>".to_string())
                                }
                            } else { deferred_reply = Some("Not logged in.\n>".to_string()); }
                        }
                    } else { deferred_reply = Some("Not logged in.\n>".to_string()); }
                } else if upper == "LOGOUT" {
                    if session.is_logged_in() {
                        let name = session.display_name();
                        session.logout().await?;
                        deferred_reply = Some(format!("User {} logged out.\n>", name));
                    } else { deferred_reply = Some("Not logged in.\n>".to_string()); }
                } else {
                    // Process all content (including potential inline commands) through command processor
                    trace!("Processing direct command/session input from {} => '{}'", node_key, content);
                    let response = session.process_command(content, &mut self.storage).await?;
                    if !response.is_empty() { deferred_reply = Some(response); }
                }
                // End of session mutable borrow scope; now send any deferred reply
                drop(session);
                if let Some(msg) = deferred_reply { self.send_message(&node_key, &msg).await?; }
            }
        } else {
            // Public channel event: parse lightweight commands
            self.public_state.prune_expired();
            let cmd = self.public_parser.parse(&ev.content);
            trace!("Public command parse result for node {} => {:?}", node_key, cmd);
            match cmd {
                PublicCommand::Help => {
                    if self.public_state.should_reply(&node_key) {
                        // Compose public notice and detailed DM help
                        // Prefer a friendly node label (long name) if protobuf node catalog knows it
                        #[cfg(feature = "meshtastic-proto")]
                        let friendly = if let Some(dev) = &self.device { if let Ok(id) = node_key.parse::<u32>() { dev.format_node_short_label(id) } else { node_key.clone() } } else { node_key.clone() };
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
        Ok(())
    }

    #[allow(unused)]
    #[cfg(feature = "weather")]
    async fn fetch_weather(&mut self) -> Option<String> {
        use tokio::time::timeout;
        const TTL: Duration = Duration::from_secs(15 * 60); // 15 minutes
        // If we have a fresh cached value, return it immediately
        if let Some((ts, val)) = &self.weather_cache {
            if ts.elapsed() < TTL { return Some(val.clone()); }
        }
        // Attempt refresh
        let zipcode = self.config.bbs.zipcode.trim();
        let url = format!("https://wttr.in/{}?format=%l:+%C+%t", zipcode);
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
            Some(v) => { self.weather_cache = Some((Instant::now(), v.clone())); Some(v) },
            None => {
                // If refresh failed but we have a stale cached value, reuse it
                if let Some((_, val)) = &self.weather_cache { return Some(val.clone()); }
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