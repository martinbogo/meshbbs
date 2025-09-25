use anyhow::Result;
// use log::{debug}; // retained for future detailed command tracing
use log::{info, warn, error};
use crate::logutil::escape_log;

use crate::config::Config;
use crate::storage::{Storage, ReplyEntry};
use crate::validation::{validate_user_name, validate_topic_name, sanitize_message_content};
use super::session::{Session, SessionState};

/// UI rendering helpers for compact, 230-byte-safe outputs
mod ui {
    /// Truncate a &str to at most max_bytes bytes, not splitting UTF-8; append '…' if truncated
    pub fn utf8_truncate(s: &str, max_bytes: usize) -> String {
        if s.as_bytes().len() <= max_bytes { return s.to_string(); }
        let mut out = s.as_bytes()[..max_bytes.min(s.len())].to_vec();
        while !out.is_empty() && (out.last().map(|b| (*b & 0b1100_0000) == 0b1000_0000).unwrap_or(false)) { out.pop(); }
        let mut s = String::from_utf8_lossy(&out).into_owned();
        if !s.is_empty() { s.push('…'); }
        s
    }

    /// Join items into a short row, capping at 5 entries per page
    pub fn list_1_to_5(items: &[String]) -> String {
        let capped = items.iter().take(5).cloned().collect::<Vec<_>>();
        let mut line = String::new();
        for (i, it) in capped.iter().enumerate() { if i > 0 { line.push_str("  "); } line.push_str(it); }
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
        if let Some((r,_)) = storage.get_topic_levels(topic) { user_level >= r } else { true }
    }
}

fn self_topic_can_post(user_level: u8, topic: &str, storage: &Storage) -> bool {
    // Use runtime topic configuration for permission checks
    if let Some(topic_config) = storage.get_topic_config(topic) {
        user_level >= topic_config.post_level
    } else {
        // Fallback to old topic_levels system for backwards compatibility
        if let Some((_,p)) = storage.get_topic_levels(topic) { user_level >= p } else { true }
    }
}

/// Processes BBS commands from users
pub struct CommandProcessor;

impl CommandProcessor {
    pub fn new() -> Self { CommandProcessor }

    fn where_am_i(&self, session: &Session, config: &Config) -> String {
        // Build a compact breadcrumb like: BBS > Topics > hello > Threads > Read
        let mut parts: Vec<String> = vec![config.bbs.name.clone()];
        match session.state {
            SessionState::Connected | SessionState::LoggingIn => parts.push("Login".into()),
            SessionState::MainMenu => parts.push("Main".into()),
            SessionState::MessageTopics | SessionState::Topics => {
                parts.push("Topics".into());
            }
            SessionState::Threads => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic { parts.push(t.clone()); }
                parts.push("Threads".into());
            }
            SessionState::ThreadRead => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic { parts.push(t.clone()); }
                parts.push("Read".into());
            }
            SessionState::ComposeNewTitle | SessionState::ComposeNewBody => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic { parts.push(t.clone()); }
                parts.push("Compose".into());
            }
            SessionState::ComposeReply => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic { parts.push(t.clone()); }
                parts.push("Reply".into());
            }
            SessionState::ConfirmDelete => { parts.push("Confirm".into()); }
            SessionState::UserMenu => parts.push("User".into()),
            SessionState::ReadingMessages => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic { parts.push(t.clone()); }
                parts.push("Reading".into());
            }
            SessionState::PostingMessage => {
                parts.push("Topics".into());
                if let Some(t) = &session.current_topic { parts.push(t.clone()); }
                parts.push("Posting".into());
            }
            SessionState::Disconnected => parts.push("Disconnected".into()),
        }
        parts.join(" > ")
    }

    /// Process a command and return a response
    pub async fn process(&self, session: &mut Session, command: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        let raw = command.trim();
        let cmd_upper = raw.to_uppercase();
        // Allow certain inline commands in any state for backward compatibility
        if let Some(resp) = self.try_inline_message_command(session, raw, &cmd_upper, storage, config).await? {
            return Ok(resp);
        }
        match session.state {
            SessionState::Connected => self.handle_initial_connection(session, &cmd_upper, storage, config).await,
            SessionState::LoggingIn => self.handle_login(session, &cmd_upper, storage, config).await,
            SessionState::MainMenu => {
                if let Some(resp) = self.try_inline_message_command(session, raw, &cmd_upper, storage, config).await? { return Ok(resp); }
                self.handle_main_menu(session, &cmd_upper, storage, config).await
            }
            SessionState::Topics => self.handle_topics(session, raw, &cmd_upper, storage, config).await,
            SessionState::Threads => self.handle_threads(session, raw, &cmd_upper, storage, config).await,
            SessionState::ThreadRead => self.handle_thread_read(session, raw, &cmd_upper, storage, config).await,
            SessionState::ComposeNewTitle => self.handle_compose_new_title(session, raw, storage, config).await,
            SessionState::ComposeNewBody => self.handle_compose_new_body(session, raw, storage, config).await,
            SessionState::ComposeReply => self.handle_compose_reply(session, raw, storage, config).await,
            SessionState::ConfirmDelete => self.handle_confirm_delete(session, raw, storage, config).await,
            SessionState::MessageTopics => {
                if let Some(resp) = self.try_inline_message_command(session, raw, &cmd_upper, storage, config).await? { return Ok(resp); }
                self.handle_message_topics(session, &cmd_upper, storage, config).await
            }
            SessionState::ReadingMessages => self.handle_reading_messages(session, &cmd_upper, storage, config).await,
            SessionState::PostingMessage => self.handle_posting_message(session, &cmd_upper, storage, config).await,
            SessionState::UserMenu => self.handle_user_menu(session, &cmd_upper, storage, config).await,
            SessionState::Disconnected => Ok("Session disconnected.".to_string()),
        }
    }

    async fn try_inline_message_command(&self, session: &mut Session, raw: &str, upper: &str, storage: &mut Storage, config: &Config) -> Result<Option<String>> {
        // WHERE-AM-I breadcrumb (global)
        if upper == "WHERE" || upper == "W" {
            let here = self.where_am_i(session, config);
            return Ok(Some(format!("[BBS] You are at: {}\n", here)));
        }
        if upper.starts_with("READ") {
            let raw_topic = raw.split_whitespace().nth(1).unwrap_or("general");
            
            // Validate topic name before using it
            let topic = match validate_topic_name(raw_topic) {
                Ok(_) => raw_topic.to_lowercase(),
                Err(_) => {
                    return Ok(Some("Invalid topic name. Topic names must contain only letters, numbers, and underscores.\n".to_string()));
                }
            };
            
            // Permission check
            if !self_topic_can_read(session.user_level, &topic, storage) { return Ok(Some("Permission denied.\n".into())); }
            session.current_topic = Some(topic.clone());
            let messages = storage.get_messages(&topic, 10).await?;
            let mut response = format!("Messages in {}:\n", topic);
            for msg in messages { response.push_str(&format!("{} | {}\n{}\n---\n", msg.author, msg.timestamp.format("%m/%d %H:%M"), msg.content)); }
            response.push_str(">\n");
            return Ok(Some(response));
        }
        if upper.starts_with("POST ") {
            let mut parts = raw.splitn(3, ' '); 
            parts.next(); // skip "POST"
            let second = parts.next();
            
            // Parse topic and message content
            let (raw_topic, text) = if let Some(s) = second { 
                if let Some(rest) = parts.next() { 
                    (s, rest) 
                } else { 
                    (session.current_topic.as_ref().map(|s| s.as_str()).unwrap_or("general"), s) 
                } 
            } else { 
                (session.current_topic.as_ref().map(|s| s.as_str()).unwrap_or("general"), "") 
            };
            
            if text.is_empty() { 
                return Ok(Some("Usage: POST [topic] <message>".into())); 
            }
            
            // Validate topic name
            let topic = match validate_topic_name(raw_topic) {
                Ok(_) => raw_topic.to_lowercase(),
                Err(_) => {
                    return Ok(Some("Invalid topic name. Topic names must contain only letters, numbers, and underscores.\n".to_string()));
                }
            };
            
            // Sanitize message content
            let sanitized_content = match sanitize_message_content(text, 10000) { // 10KB limit
                Ok(content) => content,
                Err(_) => return Ok(Some("Message content contains invalid characters or exceeds size limit.\n".to_string()))
            };
            
            if sanitized_content.trim().is_empty() {
                return Ok(Some("Message content cannot be empty after sanitization.\n".to_string()));
            }
            
            if storage.is_topic_locked(&topic) { 
                return Ok(Some("Topic locked.\n".into())); 
            }
            
            if !self_topic_can_post(session.user_level, &topic, storage) { 
                return Ok(Some("Permission denied.\n".into())); 
            }
            
            let author = session.display_name();
            storage.store_message(&topic, &author, &sanitized_content).await?;
            return Ok(Some(format!("Posted to {}.\n", topic)));
        }
        if upper == "TOPICS" || upper == "LIST" {
            let topics = storage.list_message_topics().await?;
            let mut response = "Topics:\n".to_string();
            for t in topics { 
                if self_topic_can_read(session.user_level, &t, storage) { 
                    if let Some(topic_config) = config.message_topics.get(&t) {
                        response.push_str(&format!("- {} - {}\n", t, topic_config.description));
                    } else {
                        response.push_str(&format!("- {}\n", t));
                    }
                }
            }
            response.push_str(">\n");
            return Ok(Some(response));
        }
        Ok(None)
    }

    async fn handle_initial_connection(&self, session: &mut Session, _cmd: &str, _storage: &mut Storage, config: &Config) -> Result<String> {
        session.state = SessionState::MainMenu;
        Ok(format!(
            "[{}]\nNode: {}\nAuth: REGISTER <user> <pass> or LOGIN <user> [pass]\nType HELP for commands\nMain Menu:\n[M]essages [U]ser [Q]uit\n",
            config.bbs.name,
            session.node_id
        ))
    }

    async fn handle_login(&self, session: &mut Session, cmd: &str, storage: &mut Storage, _config: &Config) -> Result<String> {
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
            storage.create_or_update_user(&username, &session.node_id).await?;
            Ok(format!("Welcome {}!\nMain Menu:\n[M]essages [U]ser [Q]uit\n", username))
        } else {
            Ok("Please enter: LOGIN <username>\n".to_string())
        }
    }

    async fn handle_main_menu(&self, session: &mut Session, cmd: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        match &cmd[..] {
            "M" | "MESSAGES" => {
                // New compact Topics UI (paged, ≤5 items)
                session.state = SessionState::Topics;
                session.list_page = 1;
                Ok(self.render_topics_page(session, storage, config).await?)
            }
            "U" | "USER" => {
                session.state = SessionState::UserMenu;
                Ok(format!(
                    "User Menu:\nUsername: {}\nLevel: {}\nLogin time: {}\n[I]nfo [S]tats [B]ack\n",
                    session.display_name(),
                    session.user_level,
                    session.login_time.format("%Y-%m-%d %H:%M UTC")
                ))
            }
            "Q" | "QUIT" | "GOODBYE" | "BYE" => { session.logout().await?; Ok("Goodbye! 73s".to_string()) }
            "H" | "HELP" | "?" => {
                // Build compact contextual help to fit within 230 bytes
                let mut out = String::new();
                if !session.is_logged_in() {
                    out.push_str("AUTH: REGISTER <u> <p> | LOGIN <u> <p>\n");
                    return Ok(out);
                }
                out.push_str("ACCT: SETPASS <new> | CHPASS <old> <new> | LOGOUT\n");
                out.push_str("MSG: M=menu; digits pick; +/- nav; F filter; READ <t>; POST <t> <txt>; TOPICS\n");
                if session.user_level >= 5 { out.push_str("MOD: DELETE <a> <id> LOCK/UNLOCK <a> DELLOG [p]\n"); }
                if session.user_level >= 10 { out.push_str("ADM: PROMOTE <u> DEMOTE <u> SYSLOG <lvl> <msg>\n"); }
                out.push_str("OTHER: WHERE/W breadcrumb | U=User | Q=Quit\n");
                // Ensure length <=230 (should already be compact; final guard)
                const MAX: usize = 230;
                if out.as_bytes().len() > MAX { out.truncate(MAX); }
                Ok(out)
            }
            // Admin commands for moderators and sysops
            cmd if cmd.starts_with("SYSLOG") => {
                // Syntax: SYSLOG <LEVEL> <message>
                if session.user_level < 10 { return Ok("Permission denied.\n".to_string()); }
                let rest = cmd.strip_prefix("SYSLOG").unwrap_or("").trim();
                if rest.is_empty() { return Ok("Usage: SYSLOG <INFO|WARN|ERROR> <message>\n".to_string()); }
                let mut parts = rest.splitn(2, ' ');
                let level = parts.next().unwrap_or("").to_uppercase();
                let message = parts.next().unwrap_or("").trim();
                if message.is_empty() { return Ok("Usage: SYSLOG <INFO|WARN|ERROR> <message>\n".to_string()); }
                // Sanitize message for logging (avoid multi-line injection)
                let safe = escape_log(message);
                match level.as_str() {
                    "INFO" => { info!("SYSLOG (sysop {}): {}", session.display_name(), safe); Ok("Logged INFO.\n".to_string()) },
                    "WARN" => { warn!("SYSLOG (sysop {}): {}", session.display_name(), safe); Ok("Logged WARN.\n".to_string()) },
                    "ERROR" => { error!("SYSLOG (sysop {}): {}", session.display_name(), safe); Ok("Logged ERROR.\n".to_string()) },
                    _ => Ok("Usage: SYSLOG <INFO|WARN|ERROR> <message>\n".to_string())
                }
            }
            cmd if cmd.starts_with("USERS") => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                let pattern = if parts.len() >= 2 { Some(parts[1].to_lowercase()) } else { None };
                
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
                            response.push_str(&format!("  {} ({}, Level {})\n", user.username, role, user.user_level));
                        }
                        
                        Ok(response)
                    }
                    Err(e) => Ok(format!("Error listing users: {}\n", e)),
                }
            }
            "WHO" => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
                Ok("Logged In Users:\nNone (session info not available in this context)\n".to_string())
            }
            cmd if cmd.starts_with("USERINFO") => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
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
                            user.username, user.user_level, role, post_count, 
                            user.first_login.format("%Y-%m-%d").to_string()
                        ))
                    }
                    Ok(None) => Ok(format!("User '{}' not found.\n", username)),
                    Err(e) => Ok(format!("Error getting user info: {}\n", e)),
                }
            }
            "SESSIONS" => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
                Ok("Active Sessions:\nNone (session info not available in this context)\n".to_string())
            }
            cmd if cmd.starts_with("KICK") => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if parts.len() < 2 {
                    return Ok("Usage: KICK <username>\n".to_string());
                }
                
                // Validate the username to kick
                let target_username = parts[1];
                match validate_user_name(target_username) {
                    Ok(_) => Ok(format!("{} has been kicked (action deferred)\n", target_username)),
                    Err(_) => Ok("Invalid username specified.\n".to_string())
                }
            }
            cmd if cmd.starts_with("BROADCAST") => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
                let message = cmd.strip_prefix("BROADCAST").map(|s| s.trim()).unwrap_or("");
                if message.is_empty() {
                    return Ok("Usage: BROADCAST <message>\n".to_string());
                }
                
                // Sanitize broadcast message content
                let sanitized_message = match sanitize_message_content(message, 5000) { // 5KB limit for broadcasts
                    Ok(content) => content,
                    Err(_) => return Ok("Broadcast message contains invalid characters or exceeds size limit.\n".to_string())
                };
                
                if sanitized_message.trim().is_empty() {
                    return Ok("Broadcast message cannot be empty after sanitization.\n".to_string());
                }
                
                Ok(format!("Broadcast sent: {}\n", sanitized_message))
            }
            "ADMIN" | "DASHBOARD" => {
                if session.user_level < 5 { return Ok("Permission denied.\n".to_string()); }
                // Get statistics
                match storage.get_statistics().await {
                    Ok(stats) => {
                        Ok(format!(
                            "BBS Administration Dashboard:\n  Total Users: {}\n  Total Messages: {}\n  Moderators: {}\n  Recent Registrations: {}\n",
                            stats.total_users, stats.total_messages, 
                            stats.moderator_count, stats.recent_registrations
                        ))
                    }
                    Err(e) => Ok(format!("Error getting statistics: {}\n", e)),
                }
            }
            _ => {
                // Quote back the invalid command; enforce overall length <= 230 bytes.
                // New terse form: 'Invalid command "<snippet>"\n'
                const PREFIX: &str = "Invalid command \""; // 17 bytes
                const SUFFIX: &str = "\"\n";               // 2 bytes + newline (3 total)
                const MAX_TOTAL: usize = 230;
                let budget = MAX_TOTAL.saturating_sub(PREFIX.len() + SUFFIX.len());
                let mut snippet = cmd.to_string();
                if snippet.len() > budget {
                    snippet.truncate(budget.saturating_sub(1));
                    while !snippet.is_char_boundary(snippet.len()) { snippet.pop(); }
                    snippet.push('…');
                }
                Ok(format!("{}{}{}", PREFIX, snippet, SUFFIX))
            }
        }
    }

    async fn render_topics_page(&self, session: &Session, storage: &mut Storage, config: &Config) -> Result<String> {
        // Gather readable topics
        let all = storage.list_message_topics().await?;
        let mut readable: Vec<(String, String)> = Vec::new(); // (id, display)
        for t in all {
            if self_topic_can_read(session.user_level, &t, storage) {
                let name = config.message_topics.get(&t).map(|c| c.name.clone()).unwrap_or_else(|| t.clone());
                readable.push((t, name));
            }
        }
        let start = (session.list_page.saturating_sub(1)) * 5;
        let page = &readable.get(start..(start+5).min(readable.len())).unwrap_or(&[]);
        let mut items: Vec<String> = Vec::new();
        for (i, (id, _name)) in page.iter().enumerate() {
            // Use topic id for display to satisfy tests expecting '1. general'
            if let Some(since) = session.unread_since {
                if let Ok(n) = storage.count_messages_since_in_topic(id, since).await {
                    if n > 0 { items.push(format!("{}. {} ({})", i+1, id, n)); continue; }
                }
            }
            items.push(format!("{}. {}", i+1, id));
        }
        let footer = "Type number to select topic. L more. H help. X exit";
        let body = ui::topics_page(&config.bbs.name, &items, footer);
        Ok(body)
    }

    async fn handle_topics(&self, session: &mut Session, raw: &str, upper: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        // Global controls
        match upper {
            "H" | "HELP" | "?" => return Ok("Topics: 1-9 pick, L more, B back, M menu, X exit\n".into()),
            "M" => { session.list_page = 1; return Ok(self.render_topics_page(session, storage, config).await?); }
            "B" => { session.state = SessionState::MainMenu; return Ok("Main Menu:\n[M]essages [U]ser [Q]uit\n".into()); }
            "X" => { session.state = SessionState::Disconnected; return Ok("Goodbye! 73s".into()); }
            "L" => { session.list_page += 1; return Ok(self.render_topics_page(session, storage, config).await?); }
            _ => {}
        }
        // Digit selection 1-9
        if let Some(ch) = raw.chars().next() { if ch.is_ascii_digit() && ch != '0' {
            let n = ch.to_digit(10).unwrap() as usize; // 1..9
            let all = storage.list_message_topics().await?;
            let mut readable: Vec<String> = Vec::new();
            for t in all { if self_topic_can_read(session.user_level, &t, storage) { readable.push(t); } }
            let idx = (session.list_page.saturating_sub(1)) * 5 + (n-1);
            if idx < readable.len() {
                session.current_topic = Some(readable[idx].clone());
                session.state = SessionState::Threads;
                session.list_page = 1;
                return Ok(self.render_threads_list(session, storage, config).await?);
            } else {
                return Ok("No more items. L shows more, B back\n".into());
            }
        }}
        Ok(self.render_topics_page(session, storage, config).await?)
    }

    async fn render_threads_list(&self, session: &Session, storage: &mut Storage, config: &Config) -> Result<String> {
        let topic = session.current_topic.clone().unwrap_or_else(|| "general".into());
        let msgs = storage.get_messages(&topic, 50).await?;
        // Newest first (already sorted in storage); paginate 5 per page
        let start = (session.list_page.saturating_sub(1)) * 5;
        // Apply optional title filter
        let filtered: Vec<_> = if let Some(f) = &session.filter_text {
            let q = f.to_lowercase();
            msgs.into_iter().filter(|m| {
                let title_src = m.title.as_deref().unwrap_or_else(|| m.content.lines().next().unwrap_or(""));
                title_src.to_lowercase().contains(&q)
            }).collect()
        } else { msgs };
        let page = &filtered.get(start..(start+5).min(filtered.len())).unwrap_or(&[]);
        let mut items: Vec<String> = Vec::new();
        for (i, m) in page.iter().enumerate() {
            let title_src = m.title.as_deref().unwrap_or_else(|| m.content.lines().next().unwrap_or(""));
            let title = ui::utf8_truncate(title_src, 32);
            let mut marker = "";
            if let Some(since) = session.unread_since {
                if m.timestamp > since { marker = "*"; }
                else if m.replies.iter().any(|r| match r { ReplyEntry::Reply(rr) => rr.timestamp > since, ReplyEntry::Legacy(_) => false }) { marker = "*"; }
            }
            items.push(format!("{} {}{}", i+1, title, marker));
        }
        let topic_disp = config.message_topics.get(&topic).map(|c| c.name.clone()).unwrap_or_else(|| topic.clone());
        let header = format!("Messages in {}:\n[BBS][{}] Threads\n", topic, topic_disp);
        let list = format!("{}\n", ui::list_1_to_5(&items));
        let footer = if session.filter_text.is_some() { "Reply: 1-9 read, N new, L more, B back, F clear" } else { "Reply: 1-9 read, N new, L more, B back, F <text> filter" };
        Ok(format!("{}{}{}\n", header, list, footer))
    }

    async fn handle_threads(&self, session: &mut Session, raw: &str, upper: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        match upper {
            "H" | "HELP" | "?" => return Ok("Threads: 1-9 read, N new, L more, B back, F filter, M topics, X exit\n".into()),
            "M" => { session.state = SessionState::Topics; session.list_page = 1; return Ok(self.render_topics_page(session, storage, config).await?); }
            "B" => { session.state = SessionState::Topics; let _ = session.filter_text.take(); return Ok(self.render_topics_page(session, storage, config).await?); }
            "X" => { session.state = SessionState::Disconnected; return Ok("Goodbye! 73s".into()); }
            "L" => { session.list_page += 1; return Ok(self.render_threads_list(session, storage, config).await?); }
            "N" => { session.state = SessionState::ComposeNewTitle; return Ok("[BBS] New thread title (≤32):\n".into()); }
            _ => {}
        }
        // Filter: F <text> or just F to clear
        if upper.starts_with("F") {
            let text = raw.strip_prefix('F').or_else(|| raw.strip_prefix('f')).unwrap_or("").trim();
            if text.is_empty() { session.filter_text = None; }
            else { session.filter_text = Some(text.to_string()); session.list_page = 1; }
            return Ok(self.render_threads_list(session, storage, config).await?);
        }
        if let Some(ch) = raw.chars().next() { if ch.is_ascii_digit() && ch != '0' {
            // Show a minimal read view of the selected message (single slice)
            let n = ch.to_digit(10).unwrap() as usize; // 1..9
            let topic = session.current_topic.clone().unwrap_or_else(|| "general".into());
            let msgs = storage.get_messages(&topic, 50).await?;
            let idx = (session.list_page.saturating_sub(1)) * 5 + (n-1);
            if idx < msgs.len() {
                let m = &msgs[idx];
                session.state = SessionState::ThreadRead;
                session.current_thread_id = Some(m.id.clone());
                session.post_index = 1;
                session.slice_index = 1;
                let topic_disp = config.message_topics.get(&topic).map(|c| c.name.clone()).unwrap_or_else(|| topic.clone());
                let title = ui::utf8_truncate(m.content.lines().next().unwrap_or(""), 24);
                let head = format!("[BBS][{} > {}] p1/1\n", topic_disp, title);
                // Leave ~80 bytes for header+footer+prompt; clamp body around 140 bytes
                let body = ui::utf8_truncate(&m.content, 140);
                let footer = "Reply: + next, Y reply, B back, H help";
                return Ok(format!("{}{}\n{}\n", head, body, footer));
            } else {
                return Ok("No more items. L shows more, B back\n".into());
            }
        }}
        Ok(self.render_threads_list(session, storage, config).await?)
    }

    async fn render_thread_read(&self, session: &Session, storage: &mut Storage, config: &Config) -> Result<String> {
        let topic = session.current_topic.clone().unwrap_or_else(|| "general".into());
        let id = if let Some(id) = &session.current_thread_id { id.clone() } else { return Ok(self.render_threads_list(session, storage, config).await?) };
        let msgs = storage.get_messages(&topic, 200).await?;
        if let Some(m) = msgs.into_iter().find(|mm| mm.id == id) {
            let topic_disp = config.message_topics.get(&topic).map(|c| c.name.clone()).unwrap_or_else(|| topic.clone());
            let title = ui::utf8_truncate(m.content.lines().next().unwrap_or(""), 24);
            let head = format!("[BBS][{} > {}] p1/1\n", topic_disp, title);
            // Budget for replies preview: include last 1 reply if present
            let mut body = ui::utf8_truncate(&m.content, 120);
            if let Some(last) = m.replies.last() {
                let rp = match last {
                    ReplyEntry::Legacy(s) => ui::utf8_truncate(s, 80),
                    ReplyEntry::Reply(r) => {
                        let stamp = r.timestamp.format("%m/%d %H:%M");
                        let line = format!("{} | {}: {}", stamp, r.author, r.content);
                        ui::utf8_truncate(&line, 80)
                    }
                };
                body.push_str("\n— ");
                body.push_str(&rp);
            }
            let footer = "Reply: + next, - prev, Y reply, B back, H help";
            Ok(format!("{}{}\n{}\n", head, body, footer))
        } else {
            Ok("Thread missing. B back.\n".into())
        }
    }

    async fn handle_thread_read(&self, session: &mut Session, _raw: &str, upper: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        match upper {
            "B" => { session.state = SessionState::Threads; return Ok(self.render_threads_list(session, storage, config).await?); }
            "H" | "HELP" | "?" => return Ok("Read: + next, - prev, Y reply, B back\n".into()),
            "+" | "-" => {
                let topic = session.current_topic.clone().unwrap_or_else(|| "general".into());
                if let Some(curr) = &session.current_thread_id {
                    let msgs = storage.get_messages(&topic, 200).await?;
                    if let Some(pos) = msgs.iter().position(|m| &m.id == curr) {
                        let new_pos = if upper == "+" { pos + 1 } else { pos.saturating_sub(1) };
                        if new_pos < msgs.len() {
                            session.current_thread_id = Some(msgs[new_pos].id.clone());
                        }
                    }
                }
                return Ok(self.render_thread_read(session, storage, config).await?);
            }
            "Y" => { session.state = SessionState::ComposeReply; return Ok("[BBS] Reply text (single message):\n".into()); }
            _ => {}
        }
        self.render_thread_read(session, storage, config).await
    }

    async fn handle_compose_new_title(&self, session: &mut Session, raw: &str, _storage: &mut Storage, _config: &Config) -> Result<String> {
        let title = raw.trim();
        if title.is_empty() { return Ok("Title required (≤32).\n".into()); }
        let title = if title.len() > 32 { ui::utf8_truncate(title, 32) } else { title.to_string() };
        session.filter_text = Some(title);
        session.state = SessionState::ComposeNewBody;
        Ok("Body: (single message)\n".into())
    }

    async fn handle_compose_new_body(&self, session: &mut Session, raw: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        let topic = session.current_topic.clone().unwrap_or_else(|| "general".into());
        if storage.is_topic_locked(&topic) { session.state = SessionState::Threads; return Ok("Topic locked.\n".into()); }
        if !self_topic_can_post(session.user_level, &topic, storage) { session.state = SessionState::Threads; return Ok("Permission denied.\n".into()); }
        let title = session.filter_text.clone().unwrap_or_else(|| "New thread".into());
        let body = raw.trim();
        if body.is_empty() { return Ok("Body required.\n".into()); }
        let content = format!("{}\n\n{}", title, body);
        let author = session.display_name();
        let _ = storage.store_message(&topic, &author, &content).await?;
        session.state = SessionState::Threads;
        session.filter_text = None;
        Ok(self.render_threads_list(session, storage, config).await?)
    }

    async fn handle_compose_reply(&self, session: &mut Session, raw: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        let topic = session.current_topic.clone().unwrap_or_else(|| "general".into());
        let id = if let Some(id) = &session.current_thread_id { id.clone() } else { session.state = SessionState::Threads; return Ok(self.render_threads_list(session, storage, config).await?) };
        if storage.is_topic_locked(&topic) { session.state = SessionState::ThreadRead; return Ok("Topic locked.\n".into()); }
        if !self_topic_can_post(session.user_level, &topic, storage) { session.state = SessionState::ThreadRead; return Ok("Permission denied.\n".into()); }
        let author = session.display_name();
        storage.append_reply(&topic, &id, &author, raw.trim()).await?;
        session.state = SessionState::ThreadRead;
        self.render_thread_read(session, storage, config).await
    }

    async fn handle_confirm_delete(&self, session: &mut Session, _raw: &str, _storage: &mut Storage, _config: &Config) -> Result<String> {
        session.state = SessionState::Threads;
        Ok("Delete flow not implemented.\n".into())
    }

    async fn handle_message_topics(&self, session: &mut Session, cmd: &str, storage: &mut Storage, config: &Config) -> Result<String> {
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
                    for msg in messages { response.push_str(&format!("From: {} | {}\n{}\n---\n", msg.author, msg.timestamp.format("%m/%d %H:%M"), msg.content)); }
                    response.push_str("[N]ext [P]rev [R]eply [B]ack\n");
                    return Ok(response);
                } else {
                    return Ok(format!("Invalid topic number. Choose 1-{}\n", topics.len()));
                }
            }
        }

        match &cmd[..] {
            "R" | "READ" => {
                session.state = SessionState::ReadingMessages;
                // Default to first available topic instead of hardcoded "general"
                let topics = storage.list_message_topics().await?;
                let default_topic = topics.first().unwrap_or(&"general".to_string()).clone();
                session.current_topic = Some(default_topic.clone());
                let messages = storage.get_messages(&default_topic, 10).await?;
                let mut response = format!("Recent messages in {}:\n", default_topic);
                for msg in messages { response.push_str(&format!("From: {} | {}\n{}\n---\n", msg.author, msg.timestamp.format("%m/%d %H:%M"), msg.content)); }
                response.push_str("[N]ext [P]rev [R]eply [B]ack\n");
                Ok(response)
            }
            "P" | "POST" => { session.state = SessionState::PostingMessage; Ok("Enter your message (end with . on a line):\n".to_string()) }
            "L" | "LIST" => {
                let topics = storage.list_message_topics().await?;
                let mut response = "Available topics:\n".to_string();
                for topic in topics { 
                    if let Some(topic_config) = config.message_topics.get(&topic) {
                        response.push_str(&format!("- {} - {}\n", topic, topic_config.description));
                    } else {
                        response.push_str(&format!("- {}\n", topic));
                    }
                }
                response.push_str("\n");
                Ok(response)
            }
            "B" | "BACK" => { session.state = SessionState::MainMenu; Ok("Main Menu:\n[M]essages [U]ser [Q]uit\n".to_string()) }
            _ => Ok("Commands: [R]ead [P]ost [L]ist [B]ack or type topic number\n".to_string())
        }
    }

    async fn handle_reading_messages(&self, session: &mut Session, cmd: &str, _storage: &mut Storage, _config: &Config) -> Result<String> {
        match &cmd[..] {
            "B" | "BACK" => { session.state = SessionState::MessageTopics; Ok("Message Topics:\n[R]ead [P]ost [L]ist [B]ack\n".to_string()) }
            _ => Ok("Commands: [N]ext [P]rev [R]eply [B]ack\n".to_string())
        }
    }

    async fn handle_posting_message(&self, session: &mut Session, cmd: &str, storage: &mut Storage, _config: &Config) -> Result<String> {
        if cmd == "." {
            session.state = SessionState::MessageTopics;
            Ok("Message posted!\nMessage Topics:\n[R]ead [P]ost [L]ist [B]ack\n".to_string())
        } else {
            let topic = session.current_topic.as_ref().unwrap_or(&"general".to_string()).clone();
            
            // Sanitize message content before storing
            let sanitized_content = match sanitize_message_content(cmd, 10000) { // 10KB limit
                Ok(content) => content,
                Err(_) => return Ok("Message content contains invalid characters or exceeds size limit. Try again or type '.' to cancel:\n".to_string())
            };
            
            if sanitized_content.trim().is_empty() {
                return Ok("Message content cannot be empty after sanitization. Try again or type '.' to cancel:\n".to_string());
            }
            
            let author = session.display_name();
            storage.store_message(&topic, &author, &sanitized_content).await?;
            session.state = SessionState::MessageTopics;
            Ok("Message posted!\nMessage Topics:\n[R]ead [P]ost [L]ist [B]ack\n".to_string())
        }
    }

    async fn handle_user_menu(&self, session: &mut Session, cmd: &str, storage: &mut Storage, _config: &Config) -> Result<String> {
        match &cmd[..] {
            "I" | "INFO" => Ok(format!(
                "User Information:\nUsername: {}\nNode ID: {}\nAccess Level: {}\nSession Duration: {} minutes\n",
                session.display_name(), session.node_id, session.user_level, session.session_duration().num_minutes()
            )),
            "S" | "STATS" => {
                let stats = storage.get_statistics().await?;
                Ok(format!(
                    "BBS Statistics:\nTotal Messages: {}\nTotal Users: {}\nModerators: {}\nRecent Registrations (7d): {}\nUptime: Connected\n",
                    stats.total_messages, stats.total_users, stats.moderator_count, stats.recent_registrations
                ))
            }
            "B" | "BACK" => { session.state = SessionState::MainMenu; Ok("Main Menu:\n[M]essages [U]ser [Q]uit\n".to_string()) }
            _ => Ok("Commands: [I]nfo [S]tats [B]ack\n".to_string())
        }
    }
}