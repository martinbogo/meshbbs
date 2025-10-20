//! Command processing and compact UI flows for the MeshBBS interactive experience.
//!
//! This module contains the state machine that drives navigation and actions in the
//! BBS user interface. It is optimized for very small message sizes (≈230 bytes) and
//! low‑bandwidth links, so all prompts and help are intentionally terse.
//!
//! Key ideas:
//! - Stateless text commands that map to a [Session] state transition
//! - Paged listings (max 5 items per page) to keep replies short
//! - UTF‑8 safe truncation helpers to avoid splitting multi‑byte characters
//!
//! The primary entrypoint is [CommandProcessor::process], which routes commands to the
//! appropriate handler based on [SessionState]. Handlers return fully rendered strings
//! to be sent to the user via the server.
//!
//! Permissions and topic visibility are derived from runtime topic configuration in
//! [crate::storage], falling back to legacy topic levels when needed for compatibility.
//!
//! Note: All outputs aim to remain within a single frame on constrained links. If you
//! extend help text or add new commands, prefer concise phrasing and consider the
//! 230‑byte budget per response.
use anyhow::Result;
// use log::{debug}; // retained for future detailed command tracing
use crate::logutil::escape_log;
use crate::metrics;
use log::{error, info, warn};

use super::games::{self, GameDoorKind};
use super::roles::LEVEL_MODERATOR;
use super::session::{Session, SessionState};
use crate::config::Config;
use crate::storage::{ReplyEntry, Storage};
use crate::tmush::commands::TinyMushProcessor;
use crate::validation::{sanitize_message_content, validate_user_name};

/// UI rendering helpers for compact, 230-byte-safe outputs
mod ui {
    /// Truncate a &str to at most max_bytes bytes, not splitting UTF-8; append '…' if truncated
    pub fn utf8_truncate(s: &str, max_bytes: usize) -> String {
        if s.len() <= max_bytes {
            return s.to_string();
        }
        let mut out = s.as_bytes()[..max_bytes.min(s.len())].to_vec();
        while !out.is_empty()
            && (out
                .last()
                .map(|b| (*b & 0b1100_0000) == 0b1000_0000)
                .unwrap_or(false))
        {
            out.pop();
        }
        let mut s = String::from_utf8_lossy(&out).into_owned();
        if !s.is_empty() {
            s.push('…');
        }
        s
    }

    /// Join items into a short row, capping at 5 entries per page
    pub fn list_1_to_5(items: &[String]) -> String {
        let capped = items.iter().take(5).cloned().collect::<Vec<_>>();
        let mut line = String::new();
        for (i, it) in capped.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(it);
        }
        line
    }

    /// Build a compact topics header + list + reply line
    pub fn topics_page(bbs_name: &str, items: &[String], footer: &str) -> String {
        let header = format!("[{}] Topics\n", bbs_name);
        let list = format!("{}\n", list_1_to_5(items));
        format!("{}{}{}\n", header, list, footer)
    }
}

fn self_topic_can_read(user_level: u8, topic: &str, storage: &Storage) -> bool {
    // Use runtime topic configuration for permission checks
    if let Some(topic_config) = storage.get_topic_config(topic) {
        user_level >= topic_config.read_level
    } else {
        // Fallback to old topic_levels system for backwards compatibility
        if let Some((r, _)) = storage.get_topic_levels(topic) {
            user_level >= r
        } else {
            true
        }
    }
}

fn self_topic_can_post(user_level: u8, topic: &str, storage: &Storage) -> bool {
    // Use runtime topic configuration for permission checks
    if let Some(topic_config) = storage.get_topic_config(topic) {
        user_level >= topic_config.post_level
    } else {
        // Fallback to old topic_levels system for backwards compatibility
        if let Some((_, p)) = storage.get_topic_levels(topic) {
            user_level >= p
        } else {
            true
        }
    }
}

/// Processes BBS commands from users
pub struct CommandProcessor;

impl CommandProcessor {
    pub fn new() -> Self {
        CommandProcessor
    }

    /// Render the top-level main menu based on enabled modules
    fn render_main_menu(&self, _session: &Session, config: &Config) -> String {
        let mut line = String::from("Main Menu:\n[M]essages ");
        if games::has_enabled_doors(&config.games) {
            line.push_str("[G]ames ");
        }
        line.push_str("[P]references [Q]uit\n");
        line
    }

    async fn user_has_password(&self, session: &Session, storage: &mut Storage) -> Result<bool> {
        if let Some(user) = session.username.as_deref() {
            if let Some(details) = storage.get_user_details(user).await? {
                return Ok(details.password_hash.is_some());
            }
        }
        Ok(false)
    }

    async fn render_preferences_menu(
        &self,
        session: &Session,
        storage: &mut Storage,
    ) -> Result<String> {
        let username = session.display_name();
        let has_password = self.user_has_password(session, storage).await?;
        let mut out = format!(
            "Preferences:\nUsername: {}\nLevel: {}\nLogin time: {}\n",
            username,
            session.user_level,
            session.login_time.format("%Y-%m-%d %H:%M UTC")
        );
        out.push_str("Options: [I]nfo [S]tats ");
        if has_password {
            out.push_str("[C]hange pass ");
        } else {
            out.push_str("[N]ew pass ");
        }
        out.push_str("[L]ogout [B]ack\n");
        Ok(out)
    }

    fn where_am_i(&self, session: &Session, config: &Config) -> String {
        // Build a compact breadcrumb like: BBS > Topics > hello > Threads > Read
        let mut parts: Vec<String> = vec![config.bbs.name.clone()];
        match session.state {
            SessionState::Connected | SessionState::LoggingIn => parts.push("Login".into()),
            SessionState::MainMenu => parts.push("Main".into()),
            SessionState::MessageTopics | SessionState::Topics => {
                parts.push("Topics".into());
            }
            SessionState::Subtopics => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Subtopics".into());
            }
            SessionState::Threads => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Threads".into());
            }
            SessionState::ThreadRead => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Read".into());
            }
            SessionState::ComposeNewTitle | SessionState::ComposeNewBody => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Compose".into());
            }
            SessionState::ComposeReply => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Reply".into());
            }
            SessionState::ConfirmDelete => {
                parts.push("Confirm".into());
            }
            SessionState::UserMenu
            | SessionState::UserChangePassCurrent
            | SessionState::UserChangePassNew
            | SessionState::UserSetPassNew => {
                parts.push("Preferences".into());
            }
            SessionState::TinyHack => {
                parts.push("TinyHack".into());
            }
            SessionState::TinyMush => {
                parts.push("TinyMUSH".into());
            }
            SessionState::ReadingMessages => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Reading".into());
            }
            SessionState::PostingMessage => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic {
                    parts.push(t.clone());
                }
                parts.push("Posting".into());
            }
            SessionState::Disconnected => parts.push("Disconnected".into()),
        }
        parts.join(" > ")
    }

    /// Process a command and return a response
    pub async fn process(
        &self,
        session: &mut Session,
        command: &str,
        storage: &mut Storage,
        config: &Config,
        game_registry: &crate::bbs::GameRegistry,
    ) -> Result<String> {
        let raw = command.trim();
        let cmd_upper = raw.to_uppercase();

        match session.state {
            // Game states - let door games handle their own commands including WHERE
            SessionState::TinyHack | SessionState::TinyMush => {
                // Commands handled in game-specific branches below
            }
            // BBS states - handle WHERE command for navigation breadcrumbs
            _ => {
                if cmd_upper == "WHERE" || cmd_upper == "W" {
                    let here = self.where_am_i(session, config);
                    return Ok(format!("[BBS] You are at: {}\n", here));
                }
            }
        }

        match session.state {
            SessionState::Connected => {
                self.handle_initial_connection(session, &cmd_upper, storage, config)
                    .await
            }
            SessionState::LoggingIn => {
                self.handle_login(session, &cmd_upper, storage, config)
                    .await
            }
            SessionState::MainMenu => {
                self.handle_main_menu(session, &cmd_upper, storage, config, game_registry)
                    .await
            }
            SessionState::TinyHack => {
                // Special game loop: 'B' or 'Q' to return to main menu; otherwise forward to game engine
                if cmd_upper == "B"
                    || cmd_upper == "BACK"
                    || cmd_upper == "MENU"
                    || cmd_upper == "Q"
                    || cmd_upper == "QUIT"
                {
                    let slug = session
                        .current_game_slug
                        .clone()
                        .unwrap_or_else(|| "tinyhack".to_string());
                    let exit_counts = metrics::record_game_exit(&slug);
                    let user = session.display_name();
                    info!(
                        target: "meshbbs::games",
                        "game.exit slug={} session={} user={} node={} reason=command command={} active={} exits={} entries={} peak={}",
                        slug,
                        escape_log(&session.id),
                        escape_log(&user),
                        escape_log(&session.node_id),
                        escape_log(cmd_upper.as_str()),
                        exit_counts.currently_active,
                        exit_counts.exits,
                        exit_counts.entries,
                        exit_counts.concurrent_peak
                    );
                    session.current_game_slug = None;
                    session.state = SessionState::MainMenu;
                    return Ok(self.render_main_menu(session, config));
                }
                let username = session.display_name();
                // Load or use prior state from disk; forgiving of missing/malformed
                let (gs0, _) =
                    crate::bbs::tinyhack::load_or_new_and_render(storage.base_dir(), &username);
                let screen =
                    crate::bbs::tinyhack::apply_and_save(storage.base_dir(), &username, gs0, raw);
                Ok(screen)
            }
            SessionState::TinyMush => {
                // TinyMUSH game loop: 'B', 'BACK', 'MENU', 'Q', or 'QUIT' to return to main menu
                if cmd_upper == "B"
                    || cmd_upper == "BACK"
                    || cmd_upper == "MENU"
                    || cmd_upper == "Q"
                    || cmd_upper == "QUIT"
                {
                    let slug = session
                        .current_game_slug
                        .clone()
                        .unwrap_or_else(|| "tinymush".to_string());
                    let exit_counts = metrics::record_game_exit(&slug);
                    let user = session.display_name();
                    info!(
                        target: "meshbbs::games",
                        "game.exit slug={} session={} user={} node={} reason=command command={} active={} exits={} entries={} peak={}",
                        slug,
                        escape_log(&session.id),
                        escape_log(&user),
                        escape_log(&session.node_id),
                        escape_log(cmd_upper.as_str()),
                        exit_counts.currently_active,
                        exit_counts.exits,
                        exit_counts.entries,
                        exit_counts.concurrent_peak
                    );
                    session.current_game_slug = None;
                    session.state = SessionState::MainMenu;
                    return Ok(self.render_main_menu(session, config));
                }

                // Forward all other commands to TinyMUSH command processor
                // Use the shared store to avoid multi-handle caching issues
                if let Some(store) = game_registry.get_tinymush_store() {
                    let mut processor = TinyMushProcessor::new(store.clone());
                    processor
                        .process_command(session, raw, storage, config)
                        .await
                } else {
                    Ok("TinyMUSH is not available".to_string())
                }
            }
            SessionState::Topics => {
                self.handle_topics(session, raw, &cmd_upper, storage, config)
                    .await
            }
            SessionState::Subtopics => {
                self.handle_subtopics(session, raw, &cmd_upper, storage, config)
                    .await
            }
            SessionState::Threads => {
                self.handle_threads(session, raw, &cmd_upper, storage, config)
                    .await
            }
            SessionState::ThreadRead => {
                self.handle_thread_read(session, raw, &cmd_upper, storage, config)
                    .await
            }
            SessionState::ComposeNewTitle => {
                self.handle_compose_new_title(session, raw, storage, config)
                    .await
            }
            SessionState::ComposeNewBody => {
                self.handle_compose_new_body(session, raw, storage, config)
                    .await
            }
            SessionState::ComposeReply => {
                self.handle_compose_reply(session, raw, storage, config)
                    .await
            }
            SessionState::ConfirmDelete => {
                self.handle_confirm_delete(session, raw, storage, config)
                    .await
            }
            SessionState::MessageTopics => {
                self.handle_message_topics(session, &cmd_upper, storage, config)
                    .await
            }
            SessionState::ReadingMessages => {
                self.handle_reading_messages(session, &cmd_upper, storage, config)
                    .await
            }
            SessionState::PostingMessage => {
                self.handle_posting_message(session, &cmd_upper, storage, config)
                    .await
            }
            SessionState::UserMenu => {
                self.handle_user_menu(session, &cmd_upper, storage, config)
                    .await
            }
            SessionState::UserChangePassCurrent => {
                self.handle_user_change_pass_current(session, raw, storage, config)
                    .await
            }
            SessionState::UserChangePassNew => {
                self.handle_user_change_pass_new(session, raw, storage, config)
                    .await
            }
            SessionState::UserSetPassNew => {
                self.handle_user_set_pass_new(session, raw, storage, config)
                    .await
            }
            SessionState::Disconnected => Ok("Session disconnected.".to_string()),
        }
    }

    async fn handle_initial_connection(
        &self,
        session: &mut Session,
        _cmd: &str,
        _storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        session.state = SessionState::MainMenu;
        // Security: Minimal information disclosure for unauthenticated users
        Ok(format!(
            "[{}]\nAuthentication required.\nPlease REGISTER <username> <password> or LOGIN <username> [password]\nType H for help.\n",
            config.bbs.name
        ))
    }

    async fn handle_login(
        &self,
        session: &mut Session,
        cmd: &str,
        storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        if cmd.starts_with("LOGIN ") {
            let raw_username = cmd.strip_prefix("LOGIN ").unwrap_or("").trim();

            // Validate username before proceeding
            let username = match validate_user_name(raw_username) {
                Ok(name) => name,
                Err(e) => {
                    return Ok(format!(
                        "Invalid username: {}\n\n\
                        Valid usernames must:\n\
                        • Be 2-30 characters long\n\
                        • Not start or end with spaces\n\
                        • Not contain path separators (/, \\)\n\
                        • Not be reserved system names\n\
                        • Not contain control characters\n\n\
                        Please try: LOGIN <valid_username>\n",
                        e
                    ));
                }
            };

            session.login(username.clone(), 1).await?;
            storage
                .create_or_update_user(&username, &session.node_id)
                .await?;
            {
                Ok(format!(
                    "Welcome {}!\n{}",
                    username,
                    self.render_main_menu(session, _config)
                ))
            }
        } else {
            Ok("Please enter: LOGIN <username>\n".to_string())
        }
    }

    async fn handle_main_menu(
        &self,
        session: &mut Session,
        cmd: &str,
        storage: &mut Storage,
        config: &Config,
        game_registry: &crate::bbs::GameRegistry,
    ) -> Result<String> {
        // Security: Check authentication before allowing access to BBS functions
        // Only HELP and QUIT commands are allowed for unauthenticated users
        if !session.is_logged_in() && cmd != "H" && cmd != "?" && cmd != "Q" {
            return Ok("Authentication required.\n\
                Please REGISTER <username> <password> or LOGIN <username> [password]\n\
                Type H for help.\n"
                .to_string());
        }

        let game_doors = games::enabled_doors(&config.games);
        if cmd == "G" || cmd == "GAMES" {
            if game_doors.is_empty() {
                return Ok("No games are currently enabled.\n".to_string());
            }
            return Ok(games::format_games_menu(&game_doors));
        }

        if let Some(door) = games::resolve_games_command(cmd, &game_doors) {
            match door.kind {
                GameDoorKind::TinyHack => {
                    let slug = door.slug;
                    session.state = SessionState::TinyHack;
                    session.current_game_slug = Some(slug.to_string());
                    let username = session.display_name();
                    let entry_counts = metrics::record_game_entry(slug);
                    info!(
                        target: "meshbbs::games",
                        "game.entry slug={} session={} user={} node={} command={} active={} peak={} entries={}",
                        slug,
                        escape_log(&session.id),
                        escape_log(&username),
                        escape_log(&session.node_id),
                        escape_log(cmd),
                        entry_counts.currently_active,
                        entry_counts.concurrent_peak,
                        entry_counts.entries
                    );
                    let (gs, screen, _is_new) =
                        crate::bbs::tinyhack::load_or_new_with_flag(storage.base_dir(), &username);
                    session.filter_text = Some(serde_json::to_string(&gs).unwrap_or_default());
                    return Ok(screen);
                }
                GameDoorKind::TinyMush => {
                    let slug = door.slug;
                    session.state = SessionState::TinyMush;
                    session.current_game_slug = Some(slug.to_string());
                    let username = session.display_name();
                    let entry_counts = metrics::record_game_entry(slug);
                    info!(
                        target: "meshbbs::games",
                        "game.entry slug={} session={} user={} node={} command={} active={} peak={} entries={}",
                        slug,
                        escape_log(&session.id),
                        escape_log(&username),
                        escape_log(&session.node_id),
                        escape_log(cmd),
                        entry_counts.currently_active,
                        entry_counts.concurrent_peak,
                        entry_counts.entries
                    );

                    // Initialize TinyMUSH for the user and return welcome screen
                    // Use the shared store from game registry
                    if let Some(store) = game_registry.get_tinymush_store() {
                        let mut processor = TinyMushProcessor::new(store.clone());
                        return processor.initialize_player(session, storage, config).await;
                    } else {
                        return Ok("TinyMUSH is not available".to_string());
                    }
                }
            }
        }

        match cmd {
            "M" => {
                // New compact Topics UI (paged, ≤5 items)
                session.state = SessionState::Topics;
                session.list_page = 1;
                self.render_topics_page(session, storage, config).await
            }
            "P" => {
                session.state = SessionState::UserMenu;
                self.render_preferences_menu(session, storage).await
            }
            "Q" => {
                session.logout().await?;
                Ok("Goodbye!".to_string())
            }
            "H" | "?" => {
                // Build compact contextual help to fit within 230 bytes
                let mut out = String::new();
                if !session.is_logged_in() {
                    // Security: Minimal information for unauthenticated users
                    out.push_str("Authentication required.\n");
                    out.push_str(
                        "Please REGISTER <username> <password> or LOGIN <username> [password]\n",
                    );
                    out.push_str("Type H for help.\n");
                    return Ok(out);
                }
                out.push_str("ACCT: P then [C]hange/[N]ew pass | [L]ogout\n");
                out.push_str("MSG: M topics; digits pick; +/- next; F <txt> filter\n");
                if !game_doors.is_empty() {
                    out.push_str("GAME: G list; G# launch\n");
                }
                if session.user_level >= 5 {
                    out.push_str(
                        "MOD: D<n> delete | P<n> pin | R<n> rename | K lock | DL [p] log\n",
                    );
                }
                if session.user_level >= 10 {
                    out.push_str("ADM: G @user=LEVEL | SYSLOG LEVEL msg | ADMIN\n");
                }
                out.push_str("OTHER: WHERE | U | Q\n");
                // Ensure length <=230 (should already be compact; final guard)
                const MAX: usize = 230;
                if out.len() > MAX {
                    out.truncate(MAX);
                }
                Ok(out)
            }
            // Admin commands for moderators and sysops
            cmd if cmd.starts_with("SYSLOG") => {
                // Syntax: SYSLOG <LEVEL> <message>
                if session.user_level < 10 {
                    return Ok("Permission denied.\n".to_string());
                }
                let rest = cmd.strip_prefix("SYSLOG").unwrap_or("").trim();
                if rest.is_empty() {
                    return Ok("Usage: SYSLOG <INFO|WARN|ERROR> <message>\n".to_string());
                }
                let mut parts = rest.splitn(2, ' ');
                let level = parts.next().unwrap_or("").to_uppercase();
                let message = parts.next().unwrap_or("").trim();
                if message.is_empty() {
                    return Ok("Usage: SYSLOG <INFO|WARN|ERROR> <message>\n".to_string());
                }
                // Sanitize message for logging (avoid multi-line injection)
                let safe = escape_log(message);
                match level.as_str() {
                    "INFO" => {
                        info!("SYSLOG (sysop {}): {}", session.display_name(), safe);
                        Ok("Logged INFO.\n".to_string())
                    }
                    "WARN" => {
                        warn!("SYSLOG (sysop {}): {}", session.display_name(), safe);
                        Ok("Logged WARN.\n".to_string())
                    }
                    "ERROR" => {
                        error!("SYSLOG (sysop {}): {}", session.display_name(), safe);
                        Ok("Logged ERROR.\n".to_string())
                    }
                    _ => Ok("Usage: SYSLOG <INFO|WARN|ERROR> <message>\n".to_string()),
                }
            }
            cmd if cmd.starts_with("USERS") => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                let pattern = if parts.len() >= 2 {
                    Some(parts[1].to_lowercase())
                } else {
                    None
                };

                match storage.list_all_users().await {
                    Ok(mut users) => {
                        // Filter users by pattern if provided
                        if let Some(ref p) = pattern {
                            users.retain(|u| u.username.to_lowercase().contains(p));
                        }

                        let mut response = if let Some(ref p) = pattern {
                            format!("Users matching '{}' ({} found):\n", p, users.len())
                        } else {
                            format!("Registered Users ({}):\n", users.len())
                        };

                        for user in users {
                            let role = super::roles::role_name(user.user_level);
                            response.push_str(&format!(
                                "  {} ({}, Level {})\n",
                                user.username, role, user.user_level
                            ));
                        }

                        Ok(response)
                    }
                    Err(e) => Ok(format!("Error listing users: {}\n", e)),
                }
            }
            cmd if cmd.starts_with("G ") || cmd.starts_with("GRANT ") => {
                // Syntax: G @user=level | G @user=Role
                if session.user_level < 10 {
                    return Ok("Permission denied.\n".to_string());
                }
                let rest = cmd
                    .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                    .trim();
                if rest.is_empty() {
                    return Ok("Usage: G @user=LEVEL|ROLE\n".into());
                }
                // Expect @username=VALUE
                let mut parts = rest.splitn(2, '=');
                let lhs = parts.next().unwrap_or("").trim();
                let rhs = parts.next().unwrap_or("").trim();
                if !lhs.starts_with('@') || rhs.is_empty() {
                    return Ok("Usage: G @user=LEVEL|ROLE\n".into());
                }
                let raw_user = lhs.trim_start_matches('@');
                // Validate username
                let username = match validate_user_name(raw_user) {
                    Ok(u) => u,
                    Err(_) => return Ok("Invalid username.\n".into()),
                };
                // Parse level/role
                let level: u8 =
                    match rhs.parse::<u8>() {
                        Ok(n) => n,
                        Err(_) => {
                            let up = rhs.to_uppercase();
                            match up.as_str() {
                                "USER" => 1,
                                "MOD" | "MODERATOR" => 5,
                                "SYSOP" | "ADMIN" => 10,
                                _ => return Ok(
                                    "Unknown role. Use USER/MODERATOR/SYSOP or numeric 1/5/10.\n"
                                        .into(),
                                ),
                            }
                        }
                    };
                match storage
                    .update_user_level(
                        &username,
                        level,
                        session.username.as_deref().unwrap_or("sysop"),
                    )
                    .await
                {
                    Ok(user) => Ok(format!(
                        "{} set to {} (level {}).\n",
                        user.username,
                        super::roles::role_name(user.user_level),
                        user.user_level
                    )),
                    Err(e) => Ok(format!("Grant failed: {}\n", e)),
                }
            }
            "WHO" => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                Ok(
                    "Logged In Users:\nNone (session info not available in this context)\n"
                        .to_string(),
                )
            }
            cmd if cmd.starts_with("USERINFO") => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() < 2 {
                    return Ok("Usage: USERINFO <username>\n".to_string());
                }

                let raw_username = parts[1];

                // Validate the username to look up
                let username = match validate_user_name(raw_username) {
                    Ok(name) => name,
                    Err(_) => {
                        return Ok("Invalid username specified.\n".to_string());
                    }
                };

                match storage.get_user_details(&username).await {
                    Ok(Some(user)) => {
                        let post_count = storage.count_user_posts(&username).await.unwrap_or(0);
                        let role = super::roles::role_name(user.user_level);
                        Ok(format!(
                            "User Information for {}:\n  Level: {} ({})\n  Posts: {}\n  Registered: {}\n",
                            user.username,
                            user.user_level,
                            role,
                            post_count,
                            user.first_login.format("%Y-%m-%d")
                        ))
                    }
                    Ok(None) => Ok(format!("User '{}' not found.\n", username)),
                    Err(e) => Ok(format!("Error getting user info: {}\n", e)),
                }
            }
            "SESSIONS" => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                Ok(
                    "Active Sessions:\nNone (session info not available in this context)\n"
                        .to_string(),
                )
            }
            cmd if cmd.starts_with("KICK") => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() < 2 {
                    return Ok("Usage: KICK <username>\n".to_string());
                }

                // Validate the username to kick
                let target_username = parts[1];
                match validate_user_name(target_username) {
                    Ok(_) => Ok(format!(
                        "{} has been kicked (action deferred)\n",
                        target_username
                    )),
                    Err(_) => Ok("Invalid username specified.\n".to_string()),
                }
            }
            cmd if cmd.starts_with("BROADCAST") => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                let message = cmd
                    .strip_prefix("BROADCAST")
                    .map(|s| s.trim())
                    .unwrap_or("");
                if message.is_empty() {
                    return Ok("Usage: BROADCAST <message>\n".to_string());
                }

                // Sanitize broadcast message content
                let sanitized_message = match sanitize_message_content(message, 5000) {
                    // 5KB limit for broadcasts
                    Ok(content) => content,
                    Err(_) => return Ok(
                        "Broadcast message contains invalid characters or exceeds size limit.\n"
                            .to_string(),
                    ),
                };

                if sanitized_message.trim().is_empty() {
                    return Ok(
                        "Broadcast message cannot be empty after sanitization.\n".to_string()
                    );
                }

                Ok(format!("Broadcast sent: {}\n", sanitized_message))
            }
            "ADMIN" | "DASHBOARD" => {
                if session.user_level < 5 {
                    return Ok("Permission denied.\n".to_string());
                }
                // Get statistics
                match storage.get_statistics().await {
                    Ok(stats) => {
                        Ok(format!(
                            "BBS Administration Dashboard:\n  Total Users: {}\n  Total Messages: {}\n  Moderators: {}\n  Recent Registrations: {}\n",
                            stats.total_users,
                            stats.total_messages,
                            stats.moderator_count,
                            stats.recent_registrations
                        ))
                    }
                    Err(e) => Ok(format!("Error getting statistics: {}\n", e)),
                }
            }
            _ => {
                // Quote back the invalid command; enforce overall length <= 230 bytes.
                // New terse form: 'Invalid command "<snippet>"\n'
                const PREFIX: &str = "Invalid command \""; // 17 bytes
                const SUFFIX: &str = "\"\n"; // 2 bytes + newline (3 total)
                const MAX_TOTAL: usize = 230;
                let budget = MAX_TOTAL.saturating_sub(PREFIX.len() + SUFFIX.len());
                let mut snippet = cmd.to_string();
                if snippet.len() > budget {
                    snippet.truncate(budget.saturating_sub(1));
                    while !snippet.is_char_boundary(snippet.len()) {
                        snippet.pop();
                    }
                    snippet.push('…');
                }
                Ok(format!("{}{}{}", PREFIX, snippet, SUFFIX))
            }
        }
    }

    async fn render_topics_page(
        &self,
        session: &Session,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        // Gather readable root topics (no parent)
        let all = storage.list_message_topics().await?;
        let mut readable: Vec<(String, String)> = Vec::new(); // (id, display)
        for t in all {
            if self_topic_can_read(session.user_level, &t, storage) {
                // Filter to only root topics (no parent)
                let is_root = storage
                    .get_topic_config(&t)
                    .map(|cfg| cfg.parent.is_none())
                    .unwrap_or(true);
                if !is_root {
                    continue;
                }
                let name = config
                    .message_topics
                    .get(&t)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| t.clone());
                readable.push((t, name));
            }
        }
        let start = (session.list_page.saturating_sub(1)) * 5;
        let page = &readable
            .get(start..(start + 5).min(readable.len()))
            .unwrap_or(&[]);
        let mut items: Vec<String> = Vec::new();
        for (i, (id, _name)) in page.iter().enumerate() {
            // Use topic id for display to satisfy tests expecting '1. general'
            if let Some(since) = session.unread_since {
                if let Ok(n) = storage.count_messages_since_in_topic(id, since).await {
                    if n > 0 {
                        items.push(format!("{}. {} ({})", i + 1, id, n));
                        continue;
                    }
                }
            }
            // If this topic has subtopics, add a marker
            let sub_count = storage.list_subtopics(id).len();
            if sub_count > 0 {
                items.push(format!("{}. {} ›", i + 1, id));
            } else {
                items.push(format!("{}. {}", i + 1, id));
            }
        }
        let footer = "Type number to select topic. L more. H help. B back. Q quit";
        let body = ui::topics_page(&config.bbs.name, &items, footer);
        Ok(body)
    }

    async fn handle_topics(
        &self,
        session: &mut Session,
        raw: &str,
        upper: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        // Global controls
        match upper {
            "H" | "?" => return Ok("Topics: 1-9 pick, L more, B back, M menu, Q quit\n".into()),
            "M" => {
                session.list_page = 1;
                return self.render_topics_page(session, storage, config).await;
            }
            "B" => {
                session.state = SessionState::MainMenu;
                return Ok(self.render_main_menu(session, config));
            }
            "Q" | "X" => {
                session.state = SessionState::Disconnected;
                return Ok("Goodbye! 73s".into());
            }
            "L" => {
                session.list_page += 1;
                return self.render_topics_page(session, storage, config).await;
            }
            _ => {}
        }
        // Digit selection 1-9
        if let Some(ch) = raw.chars().next() {
            if ch.is_ascii_digit() && ch != '0' {
                let n = ch.to_digit(10).unwrap() as usize; // 1..9
                let all = storage.list_message_topics().await?;
                let mut readable: Vec<String> = Vec::new();
                for t in all {
                    if self_topic_can_read(session.user_level, &t, storage) {
                        readable.push(t);
                    }
                }
                let idx = (session.list_page.saturating_sub(1)) * 5 + (n - 1);
                if idx < readable.len() {
                    let picked = readable[idx].clone();
                    session.current_topic = Some(picked.clone());
                    session.list_page = 1;
                    // If the picked topic has subtopics, go to Subtopics view; otherwise into Threads
                    if storage.list_subtopics(&picked).is_empty() {
                        session.state = SessionState::Threads;
                        return self.render_threads_list(session, storage, config).await;
                    } else {
                        session.state = SessionState::Subtopics;
                        return self.render_subtopics_page(session, storage, config).await;
                    }
                } else {
                    return Ok("No more items. L shows more, B back\n".into());
                }
            }
        }
        self.render_topics_page(session, storage, config).await
    }

    async fn render_subtopics_page(
        &self,
        session: &Session,
        storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        let parent = session
            .current_topic
            .clone()
            .unwrap_or_else(|| "general".into());
        let mut subs: Vec<String> = storage.list_subtopics(&parent);
        // Filter by read permission
        subs.retain(|t| self_topic_can_read(session.user_level, t, storage));
        let start = (session.list_page.saturating_sub(1)) * 5;
        let page = &subs.get(start..(start + 5).min(subs.len())).unwrap_or(&[]);
        let mut items: Vec<String> = Vec::new();
        for (i, id) in page.iter().enumerate() {
            // Unread count marker for subtopic
            if let Some(since) = session.unread_since {
                if let Ok(n) = storage.count_messages_since_in_topic(id, since).await {
                    if n > 0 {
                        items.push(format!("{}. {} ({})", i + 1, id, n));
                        continue;
                    }
                }
            }
            // Further nesting indicator if grandchild subtopics exist
            let sub_count = storage.list_subtopics(id).len();
            if sub_count > 0 {
                items.push(format!("{}. {} ›", i + 1, id));
            } else {
                items.push(format!("{}. {}", i + 1, id));
            }
        }
        let header = format!("[BBS][{}] Subtopics\n", parent);
        let list = format!("{}\n", ui::list_1_to_5(&items));
        let footer = "Pick: 1-9. U up. L more. M topics. B back. Q quit";
        Ok(format!("{}{}{}\n", header, list, footer))
    }

    async fn handle_subtopics(
        &self,
        session: &mut Session,
        raw: &str,
        upper: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        match upper {
            "H" | "?" => {
                return Ok("Subtopics: 1-9 pick, U up, L more, M topics, B back, Q quit\n".into())
            }
            "M" => {
                session.state = SessionState::Topics;
                session.list_page = 1;
                return self.render_topics_page(session, storage, config).await;
            }
            "U" | "B" => {
                session.state = SessionState::Topics;
                session.list_page = 1;
                return self.render_topics_page(session, storage, config).await;
            }
            "Q" | "X" => {
                session.state = SessionState::Disconnected;
                return Ok("Goodbye! 73s".into());
            }
            "L" => {
                session.list_page += 1;
                return self.render_subtopics_page(session, storage, config).await;
            }
            _ => {}
        }
        if let Some(ch) = raw.chars().next() {
            if ch.is_ascii_digit() && ch != '0' {
                let n = ch.to_digit(10).unwrap() as usize; // 1..9
                let parent = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                let mut subs: Vec<String> = storage.list_subtopics(&parent);
                subs.retain(|t| self_topic_can_read(session.user_level, t, storage));
                let idx = (session.list_page.saturating_sub(1)) * 5 + (n - 1);
                if idx < subs.len() {
                    let picked = subs[idx].clone();
                    session.current_topic = Some(picked.clone());
                    session.list_page = 1;
                    // If further nesting, stay in Subtopics; else proceed to Threads
                    if storage.list_subtopics(&picked).is_empty() {
                        session.state = SessionState::Threads;
                        return self.render_threads_list(session, storage, config).await;
                    } else {
                        session.state = SessionState::Subtopics;
                        return self.render_subtopics_page(session, storage, config).await;
                    }
                } else {
                    return Ok("No more items. L shows more, U up\n".into());
                }
            }
        }
        self.render_subtopics_page(session, storage, config).await
    }

    async fn render_threads_list(
        &self,
        session: &Session,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        let topic = session
            .current_topic
            .clone()
            .unwrap_or_else(|| "general".into());
        let msgs = storage.get_messages(&topic, 200).await?;
        // Order pinned first, then by timestamp desc (storage already sorts by timestamp desc)
        let (mut pinned, mut unpinned): (Vec<_>, Vec<_>) = msgs.into_iter().partition(|m| m.pinned);
        pinned.append(&mut unpinned);
        let msgs = pinned; // now ordered
                           // Paginate 5 per page
        let start = (session.list_page.saturating_sub(1)) * 5;
        // Apply optional title filter
        let filtered: Vec<_> = if let Some(f) = &session.filter_text {
            let q = f.to_lowercase();
            msgs.into_iter()
                .filter(|m| {
                    let title_src = m
                        .title
                        .as_deref()
                        .unwrap_or_else(|| m.content.lines().next().unwrap_or(""));
                    title_src.to_lowercase().contains(&q)
                })
                .collect()
        } else {
            msgs
        };
        let page = &filtered
            .get(start..(start + 5).min(filtered.len()))
            .unwrap_or(&[]);
        let mut items: Vec<String> = Vec::new();
        for (i, m) in page.iter().enumerate() {
            let title_src = m
                .title
                .as_deref()
                .unwrap_or_else(|| m.content.lines().next().unwrap_or(""));
            let title = ui::utf8_truncate(title_src, 32);
            let mut marker = "";
            if let Some(since) = session.unread_since {
                if m.timestamp > since
                    || m.replies
                        .iter()
                        .any(|r| matches!(r, ReplyEntry::Reply(rr) if rr.timestamp > since))
                {
                    marker = "*";
                }
            }
            let pin = if m.pinned { " \u{1F4CC}" } else { "" }; // 📌
            items.push(format!("{}{} {}{}", i + 1, pin, title, marker));
        }
        let topic_disp = config
            .message_topics
            .get(&topic)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| topic.clone());
        let locked_note = if storage.is_topic_locked(&topic) {
            " [locked]"
        } else {
            ""
        };
        let header = format!(
            "Messages in {}:\n[BBS][{}] Threads{}\n",
            topic, topic_disp, locked_note
        );
        let list = format!("{}\n", ui::list_1_to_5(&items));
        let mut footer = if session.filter_text.is_some() {
            "Reply: 1-9 read, N new, L more, B back, F clear".to_string()
        } else {
            "Reply: 1-9 read, N new, L more, B back, F <text> filter".to_string()
        };
        if session.user_level >= LEVEL_MODERATOR {
            footer.push_str(" | mod: D<n> del, P<n> pin, R<n> name, K lock");
        }
        Ok(format!("{}{}{}\n", header, list, footer))
    }

    async fn handle_threads(
        &self,
        session: &mut Session,
        raw: &str,
        upper: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        match upper {
            "H" | "?" => {
                let mut s = "Threads: 1-9 read, N new, L more, B back, F filter, M topics, Q quit"
                    .to_string();
                if session.user_level >= LEVEL_MODERATOR {
                    s.push_str(" | mod: D<n>, P<n>, R<n>, K");
                }
                s.push('\n');
                return Ok(s);
            }
            "M" => {
                session.state = SessionState::Topics;
                session.list_page = 1;
                return self.render_topics_page(session, storage, config).await;
            }
            "Q" | "X" => {
                session.state = SessionState::Disconnected;
                return Ok("Goodbye! 73s".into());
            }
            "B" => {
                let _ = session.filter_text.take();
                // If current topic has a parent, go up to Subtopics of parent; else to Topics
                let up_state = if let Some(t) = &session.current_topic {
                    if let Some(cfg) = storage.get_topic_config(t) {
                        if cfg.parent.is_some() {
                            Some(cfg.parent.clone().unwrap())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(parent) = up_state {
                    session.current_topic = Some(parent);
                    session.state = SessionState::Subtopics;
                    session.list_page = 1;
                    return self.render_subtopics_page(session, storage, config).await;
                } else {
                    session.state = SessionState::Topics;
                    session.list_page = 1;
                    return self.render_topics_page(session, storage, config).await;
                }
            }
            "L" => {
                session.list_page += 1;
                return self.render_threads_list(session, storage, config).await;
            }
            "N" => {
                session.state = SessionState::ComposeNewTitle;
                return Ok("[BBS] New thread title (≤32):\n".into());
            }
            _ => {}
        }
        // Moderator actions in Threads list
        if session.user_level >= LEVEL_MODERATOR {
            // K: toggle topic lock
            if upper == "K" {
                let topic = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                if storage.is_topic_locked(&topic) {
                    let _ = storage.unlock_topic_persist(&topic).await;
                } else {
                    let _ = storage.lock_topic_persist(&topic).await;
                }
                return self.render_threads_list(session, storage, config).await;
            }
            // P<n>: toggle pin on nth thread in current page (or with explicit index across pages)
            if upper.starts_with("P") {
                // Extract number (supports "P5" or "P 5")
                let idx_str = raw
                    .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                    .trim();
                if let Some(ch) = idx_str.chars().find(|c| c.is_ascii_digit()) {
                    let n = ch.to_digit(10).unwrap() as usize;
                    let topic = session
                        .current_topic
                        .clone()
                        .unwrap_or_else(|| "general".into());
                    let msgs = storage.get_messages(&topic, 200).await?;
                    let (mut pinned, mut unpinned): (Vec<_>, Vec<_>) =
                        msgs.into_iter().partition(|m| m.pinned);
                    pinned.append(&mut unpinned);
                    let start = (session.list_page.saturating_sub(1)) * 5;
                    let idx = start + (n.saturating_sub(1));
                    if idx < pinned.len() {
                        let target = &pinned[idx];
                        let new_val = !target.pinned;
                        let _ = storage
                            .set_message_pinned(&topic, &target.id, new_val)
                            .await;
                        return self.render_threads_list(session, storage, config).await;
                    } else {
                        return Ok("No such item on this page.\n".into());
                    }
                } else {
                    return Ok("Usage: P<n> (e.g., P1)\n".into());
                }
            }
            // D<n>: delete with confirm
            if upper.starts_with("D") {
                let idx_str = raw
                    .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                    .trim();
                if let Some(ch) = idx_str.chars().find(|c| c.is_ascii_digit()) {
                    let n = ch.to_digit(10).unwrap() as usize;
                    let topic = session
                        .current_topic
                        .clone()
                        .unwrap_or_else(|| "general".into());
                    let msgs = storage.get_messages(&topic, 200).await?;
                    // Same ordering
                    let (mut pinned, mut unpinned): (Vec<_>, Vec<_>) =
                        msgs.into_iter().partition(|m| m.pinned);
                    pinned.append(&mut unpinned);
                    let start = (session.list_page.saturating_sub(1)) * 5;
                    let idx = start + (n.saturating_sub(1));
                    if idx < pinned.len() {
                        let target = &pinned[idx];
                        session.current_thread_id = Some(target.id.clone());
                        session.state = SessionState::ConfirmDelete;
                        return Ok(format!("Confirm delete {}? (Y/N)\n", target.id));
                    } else {
                        return Ok("No such item on this page.\n".into());
                    }
                } else {
                    return Ok("Usage: D<n> (e.g., D1)\n".into());
                }
            }
            // R<n> <new title>: rename thread title (moderator+)
            if upper.starts_with("R") {
                let parts: Vec<&str> = raw.split_whitespace().collect();
                if parts.len() >= 2 {
                    let idx_token = parts[0];
                    let idx_str = idx_token.trim_start_matches(|c: char| c.is_ascii_alphabetic());
                    if let Ok(n) = idx_str.parse::<usize>() {
                        let new_title = raw[idx_token.len()..].trim();
                        if new_title.is_empty() {
                            return Ok("Usage: R<n> <new title>\n".into());
                        }
                        let topic = session
                            .current_topic
                            .clone()
                            .unwrap_or_else(|| "general".into());
                        let msgs = storage.get_messages(&topic, 200).await?;
                        let (mut pinned, mut unpinned): (Vec<_>, Vec<_>) =
                            msgs.into_iter().partition(|m| m.pinned);
                        pinned.append(&mut unpinned);
                        let start = (session.list_page.saturating_sub(1)) * 5;
                        let idx = start + (n.saturating_sub(1));
                        if idx < pinned.len() {
                            let target = &pinned[idx];
                            // 32-char cap consistent with compose title
                            let title_cap = if new_title.len() > 32 {
                                ui::utf8_truncate(new_title, 32)
                            } else {
                                new_title.to_string()
                            };
                            let _ = storage
                                .set_message_title(&topic, &target.id, Some(&title_cap))
                                .await;
                            return self.render_threads_list(session, storage, config).await;
                        } else {
                            return Ok("No such item on this page.\n".into());
                        }
                    }
                }
                return Ok("Usage: R<n> <new title>\n".into());
            }
        }
        // Filter: F <text> or just F to clear
        if upper.starts_with("F") {
            let text = raw
                .strip_prefix('F')
                .or_else(|| raw.strip_prefix('f'))
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                session.filter_text = None;
            } else {
                session.filter_text = Some(text.to_string());
                session.list_page = 1;
            }
            return self.render_threads_list(session, storage, config).await;
        }
        if let Some(ch) = raw.chars().next() {
            if ch.is_ascii_digit() && ch != '0' {
                // Navigate to full read view for the selected message (no body truncation)
                let n = ch.to_digit(10).unwrap() as usize; // 1..9
                let topic = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                let msgs = storage.get_messages(&topic, 50).await?;
                let idx = (session.list_page.saturating_sub(1)) * 5 + (n - 1);
                if idx < msgs.len() {
                    let m = &msgs[idx];
                    session.state = SessionState::ThreadRead;
                    session.current_thread_id = Some(m.id.clone());
                    session.post_index = 1;
                    session.slice_index = 1;
                    return self.render_thread_read(session, storage, config).await;
                } else {
                    return Ok("No more items. L shows more, B back\n".into());
                }
            }
        }
        self.render_threads_list(session, storage, config).await
    }

    async fn render_thread_read(
        &self,
        session: &Session,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        let topic = session
            .current_topic
            .clone()
            .unwrap_or_else(|| "general".into());
        let id = if let Some(id) = &session.current_thread_id {
            id.clone()
        } else {
            return self.render_threads_list(session, storage, config).await;
        };
        let msgs = storage.get_messages(&topic, 200).await?;
        if let Some(m) = msgs.into_iter().find(|mm| mm.id == id) {
            let topic_disp = config
                .message_topics
                .get(&topic)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| topic.clone());
            let title = ui::utf8_truncate(m.content.lines().next().unwrap_or(""), 24);
            let locked_note = if storage.is_topic_locked(&topic) {
                " [locked]"
            } else {
                ""
            };
            let pin_note = if m.pinned { " \u{1F4CC}" } else { "" }; // 📌
            let head = format!(
                "[BBS][{} > {}]{}{} p1/1\n",
                topic_disp, title, pin_note, locked_note
            );
            // Show full body; rely on sender auto-chunking for large content
            let mut body = m.content.clone();
            if let Some(last) = m.replies.last() {
                let rp = match last {
                    ReplyEntry::Legacy(s) => s.clone(),
                    ReplyEntry::Reply(r) => {
                        let stamp = r.timestamp.format("%m/%d %H:%M");
                        format!("{} | {}: {}", stamp, r.author, r.content)
                    }
                };
                body.push_str("\n- ");
                body.push_str(&rp);
            }
            let footer = "Reply: + next, - prev, Y reply, B back, H help";
            Ok(format!("{}{}\n{}\n", head, body, footer))
        } else {
            Ok("Thread missing. B back.\n".into())
        }
    }

    async fn handle_thread_read(
        &self,
        session: &mut Session,
        raw: &str,
        upper: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        match upper {
            "B" => {
                // From read, go back to threads; threads handler will handle further 'B'
                session.state = SessionState::Threads;
                return self.render_threads_list(session, storage, config).await;
            }
            "Q" => {
                session.state = SessionState::MainMenu;
                return Ok(self.render_main_menu(session, config));
            }
            "H" | "?" => {
                let mut s = "Read: + next, - prev, Y reply, B back, M topics".to_string();
                if session.user_level >= LEVEL_MODERATOR {
                    s.push_str(" | mod: D delete, P pin, R rename, K lock");
                }
                s.push('\n');
                return Ok(s);
            }
            "+" | "-" => {
                let topic = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                if let Some(curr) = &session.current_thread_id {
                    let msgs = storage.get_messages(&topic, 200).await?;
                    if let Some(pos) = msgs.iter().position(|m| &m.id == curr) {
                        let new_pos = if upper == "+" {
                            pos + 1
                        } else {
                            pos.saturating_sub(1)
                        };
                        if new_pos < msgs.len() {
                            session.current_thread_id = Some(msgs[new_pos].id.clone());
                        }
                    }
                }
                return self.render_thread_read(session, storage, config).await;
            }
            "Y" => {
                session.state = SessionState::ComposeReply;
                return Ok("[BBS] Reply text (single message):\n".into());
            }
            _ => {}
        }
        // Moderator actions while reading a thread
        if session.user_level >= LEVEL_MODERATOR {
            // Delete current
            if upper == "D" && session.current_thread_id.is_some() {
                session.state = SessionState::ConfirmDelete;
                let id = session.current_thread_id.clone().unwrap_or_default();
                return Ok(format!("Confirm delete {}? (Y/N)\n", id));
            }
            // Pin toggle current
            if upper == "P" {
                let topic = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                if let Some(id) = &session.current_thread_id {
                    // Determine current pin state
                    let msgs = storage.get_messages(&topic, 200).await?;
                    if let Some(m) = msgs.into_iter().find(|mm| &mm.id == id) {
                        let _ = storage.set_message_pinned(&topic, id, !m.pinned).await;
                        return self.render_thread_read(session, storage, config).await;
                    }
                }
            }
            // Rename current title
            if upper == "R" || upper.starts_with("R ") {
                let topic = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                if let Some(id) = &session.current_thread_id {
                    let new_title = raw.trim_start_matches(['R', 'r']).trim();
                    if new_title.is_empty() {
                        return Ok("Usage: R <new title>\n".into());
                    }
                    let title_cap = if new_title.len() > 32 {
                        ui::utf8_truncate(new_title, 32)
                    } else {
                        new_title.to_string()
                    };
                    let _ = storage
                        .set_message_title(&topic, id, Some(&title_cap))
                        .await;
                    return self.render_thread_read(session, storage, config).await;
                }
            }
            // K: toggle topic lock
            if upper == "K" {
                let topic = session
                    .current_topic
                    .clone()
                    .unwrap_or_else(|| "general".into());
                if storage.is_topic_locked(&topic) {
                    let _ = storage.unlock_topic_persist(&topic).await;
                } else {
                    let _ = storage.lock_topic_persist(&topic).await;
                }
                return self.render_thread_read(session, storage, config).await;
            }
        }
        self.render_thread_read(session, storage, config).await
    }

    async fn handle_compose_new_title(
        &self,
        session: &mut Session,
        raw: &str,
        _storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        let title = raw.trim();
        if title.is_empty() {
            return Ok("Title required (≤32).\n".into());
        }
        let title = if title.len() > 32 {
            ui::utf8_truncate(title, 32)
        } else {
            title.to_string()
        };
        session.filter_text = Some(title);
        session.state = SessionState::ComposeNewBody;
        Ok("Body: (single message)\n".into())
    }

    async fn handle_compose_new_body(
        &self,
        session: &mut Session,
        raw: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        let topic = session
            .current_topic
            .clone()
            .unwrap_or_else(|| "general".into());
        if storage.is_topic_locked(&topic) {
            session.state = SessionState::Threads;
            return Ok("Topic locked.\n".into());
        }
        if !self_topic_can_post(session.user_level, &topic, storage) {
            session.state = SessionState::Threads;
            return Ok("Permission denied.\n".into());
        }
        let title = session
            .filter_text
            .clone()
            .unwrap_or_else(|| "New thread".into());
        let body = raw.trim();
        if body.is_empty() {
            return Ok("Body required.\n".into());
        }
        let content = format!("{}\n\n{}", title, body);
        let author = session.display_name();
        let _ = storage.store_message(&topic, &author, &content).await?;
        session.state = SessionState::Threads;
        session.filter_text = None;
        self.render_threads_list(session, storage, config).await
    }

    async fn handle_compose_reply(
        &self,
        session: &mut Session,
        raw: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        let topic = session
            .current_topic
            .clone()
            .unwrap_or_else(|| "general".into());
        let id = if let Some(id) = &session.current_thread_id {
            id.clone()
        } else {
            session.state = SessionState::Threads;
            return self.render_threads_list(session, storage, config).await;
        };
        if storage.is_topic_locked(&topic) {
            session.state = SessionState::ThreadRead;
            return Ok("Topic locked.\n".into());
        }
        if !self_topic_can_post(session.user_level, &topic, storage) {
            session.state = SessionState::ThreadRead;
            return Ok("Permission denied.\n".into());
        }
        let author = session.display_name();
        storage
            .append_reply(&topic, &id, &author, raw.trim())
            .await?;
        session.state = SessionState::ThreadRead;
        self.render_thread_read(session, storage, config).await
    }

    async fn handle_confirm_delete(
        &self,
        session: &mut Session,
        raw: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        // Only moderators can delete
        if session.user_level < LEVEL_MODERATOR {
            session.state = SessionState::Threads;
            return self.render_threads_list(session, storage, config).await;
        }
        let answer = raw.trim().to_uppercase();
        let topic = session
            .current_topic
            .clone()
            .unwrap_or_else(|| "general".into());
        match answer.as_str() {
            "Y" => {
                if let Some(id) = session.current_thread_id.clone() {
                    let ok = storage.delete_message(&topic, &id).await.unwrap_or(false);
                    if ok {
                        let actor = session
                            .username
                            .as_deref()
                            .unwrap_or(&session.display_name())
                            .to_string();
                        let _ = storage.append_deletion_audit(&topic, &id, &actor).await;
                    }
                }
                session.state = SessionState::Threads;
                session.current_thread_id = None;
                let mut body = String::from("Deleted.\n");
                body.push_str(&self.render_threads_list(session, storage, config).await?);
                Ok(body)
            }
            "N" => {
                session.state = SessionState::Threads;
                let mut body = String::from("Canceled.\n");
                body.push_str(&self.render_threads_list(session, storage, config).await?);
                Ok(body)
            }
            _ => Ok("Confirm delete? (Y/N)\n".into()),
        }
    }

    async fn handle_message_topics(
        &self,
        session: &mut Session,
        cmd: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        // Check if command is a number for topic selection
        if let Ok(num) = cmd.parse::<usize>() {
            if num >= 1 {
                let topics = storage.list_message_topics().await?;
                if num <= topics.len() {
                    let selected_topic = &topics[num - 1];
                    session.state = SessionState::ReadingMessages;
                    session.current_topic = Some(selected_topic.clone());
                    let messages = storage.get_messages(selected_topic, 10).await?;
                    let mut response = format!("Recent messages in {}:\n", selected_topic);
                    for msg in messages {
                        response.push_str(&format!(
                            "From: {} | {}\n{}\n---\n",
                            msg.author,
                            msg.timestamp.format("%m/%d %H:%M"),
                            msg.content
                        ));
                    }
                    response.push_str("[N]ext [P]rev [R]eply [B]ack\n");
                    return Ok(response);
                } else {
                    return Ok(format!("Invalid topic number. Choose 1-{}\n", topics.len()));
                }
            }
        }

        match cmd {
            "R" => {
                session.state = SessionState::ReadingMessages;
                // Default to first available topic instead of hardcoded "general"
                let topics = storage.list_message_topics().await?;
                let default_topic = topics.first().unwrap_or(&"general".to_string()).clone();
                session.current_topic = Some(default_topic.clone());
                let messages = storage.get_messages(&default_topic, 10).await?;
                let mut response = format!("Recent messages in {}:\n", default_topic);
                for msg in messages {
                    response.push_str(&format!(
                        "From: {} | {}\n{}\n---\n",
                        msg.author,
                        msg.timestamp.format("%m/%d %H:%M"),
                        msg.content
                    ));
                }
                response.push_str("[N]ext [P]rev [R]eply [B]ack\n");
                Ok(response)
            }
            "P" => {
                session.state = SessionState::PostingMessage;
                Ok("Enter your message (end with . on a line):\n".to_string())
            }
            "L" => {
                let topics = storage.list_message_topics().await?;
                let mut response = "Available topics:\n".to_string();
                for topic in topics {
                    if let Some(topic_config) = config.message_topics.get(&topic) {
                        response.push_str(&format!("- {} - {}\n", topic, topic_config.description));
                    } else {
                        response.push_str(&format!("- {}\n", topic));
                    }
                }
                response.push('\n');
                Ok(response)
            }
            "B" | "Q" => {
                session.state = SessionState::MainMenu;
                Ok(self.render_main_menu(session, config))
            }
            _ => Ok("Commands: [R]ead [P]ost [L]ist [B]ack or type topic number\n".to_string()),
        }
    }

    async fn handle_reading_messages(
        &self,
        session: &mut Session,
        cmd: &str,
        _storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        match cmd {
            "B" => {
                session.state = SessionState::MessageTopics;
                Ok("Message Topics:\n[R]ead [P]ost [L]ist [B]ack\n".to_string())
            }
            "Q" => {
                session.state = SessionState::MainMenu;
                Ok(self.render_main_menu(session, _config))
            }
            _ => Ok("Commands: [N]ext [P]rev [R]eply [B]ack\n".to_string()),
        }
    }

    async fn handle_posting_message(
        &self,
        session: &mut Session,
        cmd: &str,
        storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        if cmd == "." {
            session.state = SessionState::MessageTopics;
            Ok("Message posted!\nMessage Topics:\n[R]ead [P]ost [L]ist [B]ack\n".to_string())
        } else {
            let topic = session
                .current_topic
                .as_ref()
                .unwrap_or(&"general".to_string())
                .clone();

            // Sanitize message content before storing
            let sanitized_content = match sanitize_message_content(cmd, 10000) { // 10KB limit
                Ok(content) => content,
                Err(_) => return Ok("Message content contains invalid characters or exceeds size limit. Try again or type '.' to cancel:\n".to_string())
            };

            if sanitized_content.trim().is_empty() {
                return Ok("Message content cannot be empty after sanitization. Try again or type '.' to cancel:\n".to_string());
            }

            let author = session.display_name();
            storage
                .store_message(&topic, &author, &sanitized_content)
                .await?;
            session.state = SessionState::MessageTopics;
            Ok("Message posted!\nMessage Topics:\n[R]ead [P]ost [L]ist [B]ack\n".to_string())
        }
    }

    async fn handle_user_menu(
        &self,
        session: &mut Session,
        cmd: &str,
        storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        match cmd {
            "I" => {
                let mut out = format!(
                    "User Information:\nUsername: {}\nNode ID: {}\nAccess Level: {}\nSession Duration: {} minutes\n\n",
                    session.display_name(),
                    session.node_id,
                    session.user_level,
                    session.session_duration().num_minutes()
                );
                out.push_str(&self.render_preferences_menu(session, storage).await?);
                Ok(out)
            }
            "S" => {
                let stats = storage.get_statistics().await?;
                let mut out = format!(
                    "BBS Statistics:\nTotal Messages: {}\nTotal Users: {}\nModerators: {}\nRecent Registrations (7d): {}\nUptime: Connected\n\n",
                    stats.total_messages, stats.total_users, stats.moderator_count, stats.recent_registrations
                );
                out.push_str(&self.render_preferences_menu(session, storage).await?);
                Ok(out)
            }
            "C" => {
                if !self.user_has_password(session, storage).await? {
                    let mut out =
                        String::from("Password not set. Use [N] to create one first.\n\n");
                    out.push_str(&self.render_preferences_menu(session, storage).await?);
                    return Ok(out);
                }
                session.state = SessionState::UserChangePassCurrent;
                session.pending_input = None;
                Ok("Enter current password (or '.' to cancel):\n".to_string())
            }
            "N" => {
                if self.user_has_password(session, storage).await? {
                    let mut out = String::from("Password already set. Use [C] to change it.\n\n");
                    out.push_str(&self.render_preferences_menu(session, storage).await?);
                    return Ok(out);
                }
                session.state = SessionState::UserSetPassNew;
                Ok("Enter new password (min 8 chars, or '.' to cancel):\n".to_string())
            }
            "L" => {
                session.logout().await?;
                Ok("Goodbye! 73s".to_string())
            }
            "B" => {
                session.state = SessionState::MainMenu;
                Ok(self.render_main_menu(session, config))
            }
            _ => {
                let mut out = String::from("Unrecognized option.\n\n");
                out.push_str(&self.render_preferences_menu(session, storage).await?);
                Ok(out)
            }
        }
    }

    async fn handle_user_change_pass_current(
        &self,
        session: &mut Session,
        raw: &str,
        storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        let input = raw.trim();
        if input == "." {
            session.state = SessionState::UserMenu;
            session.pending_input = None;
            let mut out = String::from("Password change cancelled.\n\n");
            out.push_str(&self.render_preferences_menu(session, storage).await?);
            return Ok(out);
        }
        session.pending_input = Some(input.to_string());
        session.state = SessionState::UserChangePassNew;
        Ok("Enter new password (min 8 chars, or '.' to cancel):\n".to_string())
    }

    async fn handle_user_change_pass_new(
        &self,
        session: &mut Session,
        raw: &str,
        storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        let input = raw.trim();
        if input == "." {
            session.state = SessionState::UserMenu;
            session.pending_input = None;
            let mut out = String::from("Password change cancelled.\n\n");
            out.push_str(&self.render_preferences_menu(session, storage).await?);
            return Ok(out);
        }
        if input.len() < 8 {
            return Ok("Password too short (min 8). Try again or '.' to cancel:\n".to_string());
        }
        if input.len() > 128 {
            return Ok("Password too long (max 128). Try again or '.' to cancel:\n".to_string());
        }
        let Some(old_password) = session.pending_input.clone() else {
            session.state = SessionState::UserMenu;
            let mut out = String::from("Password change state lost. Start again from [P].\n\n");
            out.push_str(&self.render_preferences_menu(session, storage).await?);
            return Ok(out);
        };
        let Some(user) = session.username.clone() else {
            session.state = SessionState::MainMenu;
            return Ok("Session no longer authenticated. Returning to main menu.\n".to_string());
        };
        let (_, ok) = storage.verify_user_password(&user, &old_password).await?;
        if !ok {
            session.state = SessionState::UserMenu;
            session.pending_input = None;
            let mut out = String::from("Invalid current password.\n\n");
            out.push_str(&self.render_preferences_menu(session, storage).await?);
            return Ok(out);
        }
        if old_password == input {
            return Ok(
                "New password must differ from current. Try again or '.' to cancel:\n".to_string(),
            );
        }
        storage.update_user_password(&user, input).await?;
        session.state = SessionState::UserMenu;
        session.pending_input = None;
        let mut out = String::from("Password changed.\n\n");
        out.push_str(&self.render_preferences_menu(session, storage).await?);
        Ok(out)
    }

    async fn handle_user_set_pass_new(
        &self,
        session: &mut Session,
        raw: &str,
        storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        let input = raw.trim();
        if input == "." {
            session.state = SessionState::UserMenu;
            let mut out = String::from("Password setup cancelled.\n\n");
            out.push_str(&self.render_preferences_menu(session, storage).await?);
            return Ok(out);
        }
        if input.len() < 8 {
            return Ok("Password too short (min 8). Try again or '.' to cancel:\n".to_string());
        }
        if input.len() > 128 {
            return Ok("Password too long (max 128). Try again or '.' to cancel:\n".to_string());
        }
        let Some(user) = session.username.clone() else {
            session.state = SessionState::MainMenu;
            return Ok("Session no longer authenticated. Returning to main menu.\n".to_string());
        };
        storage.update_user_password(&user, input).await?;
        session.state = SessionState::UserMenu;
        let mut out = String::from("Password set.\n\n");
        out.push_str(&self.render_preferences_menu(session, storage).await?);
        Ok(out)
    }
}

impl Default for CommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}
