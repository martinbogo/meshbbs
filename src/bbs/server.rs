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
    #[cfg(feature = "meshtastic-proto")]
    pending_direct: Vec<(u32, String)>, // queue of (dest_node_id, message) awaiting our node id
    #[cfg(test)]
    pub(crate) test_messages: Vec<(String,String)>, // (to, message)
    // Track the last login banner sent so integration tests (without cfg(test)) can assert content.
    last_banner: Option<String>,
}

impl BbsServer {
    /// Create a new BBS server instance
    pub async fn new(config: Config) -> Result<Self> {
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
            #[cfg(feature = "meshtastic-proto")]
            pending_direct: Vec::new(),
            #[cfg(test)]
            test_messages: Vec::new(),
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
    pub fn last_banner(&self) -> Option<&String> { self.last_banner.as_ref() }

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
            self.last_banner = Some(banner.clone());
            if unread > 0 { banner.push_str(&format!("{} new messages since your last login.\n", unread)); }
            banner
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
    pub async fn test_update_level(&mut self, username: &str, lvl: u8) -> Result<()> { if username == self.config.bbs.sysop { return Err(anyhow::anyhow!("Cannot modify sysop level")); } self.storage.update_user_level(username, lvl).await.map(|_| ()) }
    #[allow(dead_code)]
    pub async fn test_store_message(&mut self, area: &str, author: &str, content: &str) -> Result<String> { self.storage.store_message(area, author, content).await }
    #[allow(dead_code)]
    pub async fn test_get_messages(&self, area: &str, limit: usize) -> Result<Vec<crate::storage::Message>> { self.storage.get_messages(area, limit).await }
    #[allow(dead_code)]
    pub fn test_is_locked(&self, area: &str) -> bool { self.storage.is_area_locked(area) }
    #[allow(dead_code)]
    pub async fn test_deletion_page(&self, page: usize, size: usize) -> Result<Vec<crate::storage::DeletionAuditEntry>> { self.storage.get_deletion_audit_page(page, size).await }

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
                // If there was a pending login, apply username now
                if let Some(username) = self.public_state.take_pending(&node_key) {
                    // Enforce capacity before auto-login from public channel
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
                            let banner = self.prepare_login_banner(unread);
                            let _ = self.send_message(&node_key, &format!("{}>", banner)).await;
                        } else {
                            let banner = self.prepare_login_banner(0);
                            let _ = self.send_message(&node_key, &format!("{}>", banner)).await;
                        }
                    }
                } else {
                    let _ = self.send_message(&node_key, "Welcome to MeshBBS. Use REGISTER <name> <pass> to create an account or LOGIN <name> <pass>. Type HELP for basics.").await;
                }
                self.sessions.insert(node_key.clone(), session);
            }
                // New consolidated DM command handling with max_users and idle pruning
                self.prune_idle_sessions().await; // always prune first
                let raw_content = ev.content.trim().to_string();
                let upper = raw_content.to_uppercase();
                // Count current logged in sessions (excluding the session for this node if it is not yet logged in)
                let logged_in_count = self.sessions.values().filter(|s| s.is_logged_in()).count();
                enum PostAction { None, Delete{area:String,id:String,actor:String}, Lock{area:String,actor:String}, Unlock{area:String,actor:String} }
                let mut post_action = PostAction::None;
                let mut deferred_reply: Option<String> = None;
                if let Some(session) = self.sessions.get_mut(&node_key) {
                    session.update_activity();
                    #[cfg(feature = "meshtastic-proto")]
                    if let (Some(dev), Ok(idnum)) = (&self.device, node_key.parse::<u32>()) {
                        let (short,long) = dev.format_node_combined(idnum);
                        session.update_labels(Some(short), Some(long));
                    }
                    if upper.starts_with("REGISTER ") {
                        let parts: Vec<&str> = raw_content.split_whitespace().collect();
                        if parts.len() < 3 { deferred_reply = Some("Usage: REGISTER <username> <password>\n>".into()); }
                        else {
                            let user = parts[1]; let pass = parts[2];
                            if pass.len() < 4 { deferred_reply = Some("Password too short (min 4).\n>".into()); }
                            else {
                                match self.storage.register_user(user, pass, Some(&node_key)).await {
                                    Ok(_) => { session.login(user.to_string(), 1).await?; deferred_reply = Some(format!("Registered and logged in as {}.\nWelcome, {} you are now logged in.\n>", user, user)); }
                                    Err(e) => { deferred_reply = Some(format!("Register failed: {}\n>", e)); }
                                }
                            }
                        }
                    } else if upper.starts_with("LOGIN ") {
                        // Enforce max_users only if this session is not yet logged in
                        if !session.is_logged_in() && (logged_in_count as u32) >= self.config.bbs.max_users {
                            deferred_reply = Some("All available sessions are in use, please wait and try again later.\n>".into());
                        } else if session.is_logged_in() {
                            deferred_reply = Some(format!("Already logged in as {}.\n>", session.display_name()));
                        } else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: LOGIN <username> [password]\n>".into()); }
                            else {
                                let user = parts[1];
                                let password_opt = if parts.len() >= 3 { Some(parts[2]) } else { None };
                                match self.storage.get_user(user).await? {
                                    None => deferred_reply = Some("No such user. Use REGISTER <u> <p>.\n>".into()),
                                    Some(u) => {
                                        let has_password = u.password_hash.is_some();
                                        let node_bound = u.node_id.as_deref() == Some(&node_key);
                                        if !has_password {
                                            // User must set a password on first login attempt
                                            if let Some(pass) = password_opt {
                                                if pass.len() < 4 { deferred_reply = Some("Password too short (min 4).\n>".into()); }
                                                else {
                                                    let updated_user = self.storage.set_user_password(user, pass).await?;
                                                    let updated = if !node_bound { self.storage.bind_user_node(user, &node_key).await? } else { updated_user };
                                                    session.login(updated.username.clone(), updated.user_level).await?;
                                                    // First-time password set; unread messages prior to this first authenticated login are based on prior last_login value.
                                                    // set_user_password already bumped last_login, so computing unread would yield zero. This is acceptable; show none.
                                                    let _ = self.storage.record_user_login(user).await; // ensure fresh timestamp after full login
                                                    // No unread count expected here (legacy first login)
                                                    let banner = self.prepare_login_banner(0);
                                                    deferred_reply = Some(format!("Password set. {}Welcome, {} you are now logged in.\n>", banner, updated.username));
                                                }
                                            } else {
                                                deferred_reply = Some("Password not set. LOGIN <user> <newpass> to set your password.\n>".into());
                                            }
                                        } else {
                                            // Has password: require it if not bound or if password provided
                                            if password_opt.is_none() { deferred_reply = Some("Password required: LOGIN <user> <pass>\n>".into()); }
                                            else {
                                                let pass = password_opt.unwrap();
                                                let (_maybe, ok) = self.storage.verify_user_password(user, pass).await?;
                                                if !ok { deferred_reply = Some("Invalid password.\n>".into()); }
                                                else {
                                                    let updated = if !node_bound { self.storage.bind_user_node(user, &node_key).await? } else { u };
                                                    session.login(updated.username.clone(), updated.user_level).await?;
                                                    let prev_last = updated.last_login; // captured before we update last_login again
                                                    let unread = self.storage.count_messages_since(prev_last).await.unwrap_or(0);
                                                    let updated2 = self.storage.record_user_login(user).await.unwrap_or(updated);
                                                    let banner = self.prepare_login_banner(unread);
                                                    deferred_reply = Some(format!("{}Welcome, {} you are now logged in.\n>", banner, updated2.username));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("CHPASS ") {
                        if session.username.as_deref() == Some(&self.config.bbs.sysop) {
                            deferred_reply = Some("Sysop password managed externally. Use sysop-passwd CLI.\n>".into());
                        } else if session.is_logged_in() {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 3 { deferred_reply = Some("Usage: CHPASS <old> <new>\n>".into()); }
                            else {
                                let old = parts[1]; let newp = parts[2];
                                if newp.len() < 8 { deferred_reply = Some("New password too short (min 8).\n>".into()); }
                                else if newp.len() > 128 { deferred_reply = Some("New password too long.\n>".into()); }
                                else if let Some(user_name) = &session.username {
                                    match self.storage.get_user(user_name).await? {
                                        Some(u) => {
                                            if u.password_hash.is_none() { deferred_reply = Some("No existing password. Use SETPASS <new>.\n>".into()); }
                                            else {
                                                let (_u2, ok) = self.storage.verify_user_password(user_name, old).await?;
                                                if !ok { deferred_reply = Some("Invalid password.\n>".into()); }
                                                else if old == newp { deferred_reply = Some("New password must differ.\n>".into()); }
                                                else { self.storage.update_user_password(user_name, newp).await?; deferred_reply = Some("Password changed.\n>".into()); }
                                            }
                                        }
                                        None => deferred_reply = Some("Session user missing.\n>".into())
                                    }
                                } else { deferred_reply = Some("Not logged in.\n>".into()); }
                            }
                        } else { deferred_reply = Some("Not logged in.\n>".into()); }
                    } else if upper.starts_with("SETPASS ") {
                        if session.username.as_deref() == Some(&self.config.bbs.sysop) {
                            deferred_reply = Some("Sysop password managed externally. Use sysop-passwd CLI.\n>".into());
                        } else if session.is_logged_in() {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: SETPASS <new>\n>".into()); }
                            else {
                                let newp = parts[1];
                                if newp.len() < 8 { deferred_reply = Some("New password too short (min 8).\n>".into()); }
                                else if newp.len() > 128 { deferred_reply = Some("New password too long.\n>".into()); }
                                else if let Some(user_name) = &session.username {
                                    match self.storage.get_user(user_name).await? {
                                        Some(u) => {
                                            if u.password_hash.is_some() { deferred_reply = Some("Password already set. Use CHPASS <old> <new>.\n>".into()); }
                                            else { self.storage.update_user_password(user_name, newp).await?; deferred_reply = Some("Password set.\n>".into()); }
                                        }
                                        None => deferred_reply = Some("Session user missing.\n>".into())
                                    }
                                } else { deferred_reply = Some("Not logged in.\n>".into()); }
                            }
                        } else { deferred_reply = Some("Not logged in.\n>".into()); }
                    } else if upper.starts_with("PROMOTE ") {
                        if session.username.as_deref() != Some(&self.config.bbs.sysop) { deferred_reply = Some("Permission denied.\n>".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: PROMOTE <user>\n>".into()); }
                            else {
                                let target = parts[1];
                                match self.storage.get_user(target).await? {
                                    None => deferred_reply = Some("User not found.\n>".into()),
                                    Some(u) => {
                                        if u.username == self.config.bbs.sysop { deferred_reply = Some("Cannot modify sysop.\n>".into()); }
                                        else if u.user_level >= LEVEL_MODERATOR { deferred_reply = Some("Already moderator or higher.\n>".into()); }
                                        else { self.storage.update_user_level(&u.username, LEVEL_MODERATOR).await?; deferred_reply = Some(format!("{} promoted to {}.\n>", u.username, role_name(LEVEL_MODERATOR))); }
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("DEMOTE ") {
                        if session.username.as_deref() != Some(&self.config.bbs.sysop) { deferred_reply = Some("Permission denied.\n>".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: DEMOTE <user>\n>".into()); }
                            else {
                                let target = parts[1];
                                match self.storage.get_user(target).await? {
                                    None => deferred_reply = Some("User not found.\n>".into()),
                                    Some(u) => {
                                        if u.username == self.config.bbs.sysop { deferred_reply = Some("Cannot modify sysop.\n>".into()); }
                                        else if u.user_level <= LEVEL_USER { deferred_reply = Some("Already at base level.\n>".into()); }
                                        else { self.storage.update_user_level(&u.username, LEVEL_USER).await?; deferred_reply = Some(format!("{} demoted to {}.\n>", u.username, role_name(LEVEL_USER))); }
                                    }
                                }
                            }
                        }
                    } else if upper.starts_with("DELETE ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n>".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 3 { deferred_reply = Some("Usage: DELETE <area> <id>\n>".into()); }
                            else {
                                let area = parts[1].to_lowercase();
                                let id = parts[2].to_string();
                                let actor = session.username.clone().unwrap_or("?".into());
                                post_action = PostAction::Delete{area,id,actor};
                            }
                        }
                    } else if upper.starts_with("LOCK ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n>".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: LOCK <area>\n>".into()); }
                            else {
                                let area = parts[1].to_lowercase();
                                let actor = session.username.clone().unwrap_or("?".into());
                                post_action = PostAction::Lock{area:area.clone(), actor};
                                deferred_reply = Some(format!("Area {} locked.\n>", area));
                            }
                        }
                    } else if upper.starts_with("UNLOCK ") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n>".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            if parts.len() < 2 { deferred_reply = Some("Usage: UNLOCK <area>\n>".into()); }
                            else {
                                let area = parts[1].to_lowercase();
                                let actor = session.username.clone().unwrap_or("?".into());
                                post_action = PostAction::Unlock{area:area.clone(), actor};
                                deferred_reply = Some(format!("Area {} unlocked.\n>", area));
                            }
                        }
                    } else if upper.starts_with("DELLOG") {
                        if session.user_level < LEVEL_MODERATOR { deferred_reply = Some("Permission denied.\n>".into()); }
                        else {
                            let parts: Vec<&str> = raw_content.split_whitespace().collect();
                            let page = if parts.len() >= 2 { parts[1].parse::<usize>().unwrap_or(1) } else { 1 };
                            match self.storage.get_deletion_audit_page(page, 10).await {
                                Ok(entries) => {
                                    if entries.is_empty() { deferred_reply = Some("No entries.\n>".into()); }
                                    else { let mut out = String::from("Deletion Log:\n"); for e in entries { out.push_str(&format!("{} {} {} {}\n", e.timestamp, e.actor, e.area, e.id)); } out.push('>'); deferred_reply = Some(out); }
                                }
                                Err(e) => deferred_reply = Some(format!("Failed: {}\n>", e)),
                            }
                        }
                    } else if upper == "LOGOUT" {
                        if session.is_logged_in() { let name = session.display_name(); session.logout().await?; deferred_reply = Some(format!("User {} logged out.\n>", name)); }
                        else { deferred_reply = Some("Not logged in.\n>".into()); }
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
                            Ok(true) => { deferred_reply.get_or_insert(format!("Deleted message {} in {}.\n>", id, area)); },
                            Ok(false) => { deferred_reply.get_or_insert("Not found.\n>".into()); },
                            Err(e) => { deferred_reply.get_or_insert(format!("Delete failed: {}\n>", e)); }
                        }
                    }
                    PostAction::Lock{area,actor} => {
                        if let Err(e) = self.moderator_lock_area(&area, &actor).await { deferred_reply.get_or_insert(format!("Lock failed: {}\n>", e)); }
                    }
                    PostAction::Unlock{area,actor} => {
                        if let Err(e) = self.moderator_unlock_area(&area, &actor).await { deferred_reply.get_or_insert(format!("Unlock failed: {}\n>", e)); }
                    }
                }
                if let Some(msg) = deferred_reply { self.send_message(&node_key, &msg).await?; }
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
        #[cfg(test)]
        self.test_messages.push((to_node.to_string(), message.to_string()));
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
