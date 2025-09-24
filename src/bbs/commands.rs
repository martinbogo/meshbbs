use anyhow::Result;
// use log::{debug}; // retained for future detailed command tracing

use crate::config::Config;
use crate::storage::Storage;
use crate::validation::{validate_user_name, validate_topic_name, sanitize_message_content};
use super::session::{Session, SessionState};

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

    /// Process a command and return a response
    pub async fn process(&self, session: &mut Session, command: &str, storage: &mut Storage, config: &Config) -> Result<String> {
        let raw = command.trim();
        let cmd_upper = raw.to_uppercase();
        match session.state {
            SessionState::Connected => self.handle_initial_connection(session, &cmd_upper, storage, config).await,
            SessionState::LoggingIn => self.handle_login(session, &cmd_upper, storage, config).await,
            SessionState::MainMenu => {
                if let Some(resp) = self.try_inline_message_command(session, raw, &cmd_upper, storage, config).await? { return Ok(resp); }
                self.handle_main_menu(session, &cmd_upper, storage, config).await
            }
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

    async fn handle_initial_connection(&self, session: &mut Session, _cmd: &str, _storage: &mut Storage, _config: &Config) -> Result<String> {
        session.state = SessionState::MainMenu;
        Ok(format!(
            "Welcome to MeshBBS!\nNode: {}\nAuth: REGISTER <user> <pass> or LOGIN <user> [pass]\nType HELP for commands\nMain Menu:\n[M]essages [U]ser [Q]uit\n",
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
                session.state = SessionState::MessageTopics;
                let topics = storage.list_message_topics().await?;
                let mut response = "Message Topics:\n".to_string();
                for (i, topic) in topics.iter().enumerate() { 
                    if let Some(topic_config) = config.message_topics.get(topic) {
                        response.push_str(&format!("{}. {} - {}\n", i + 1, topic, topic_config.description));
                    } else {
                        response.push_str(&format!("{}. {}\n", i + 1, topic));
                    }
                }
                response.push_str("Type number to select topic, or [R]ead [P]ost [L]ist [B]ack\n");
                Ok(response)
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
                out.push_str("MSG: M=menu READ <t> POST <t> <txt> TOPICS/LIST\n");
                if session.user_level >= 5 { out.push_str("MOD: DELETE <a> <id> LOCK/UNLOCK <a> DELLOG [p]\n"); }
                if session.user_level >= 10 { out.push_str("ADM: PROMOTE <u> DEMOTE <u>\n"); }
                out.push_str("OTHER: U=User Q=Quit\n");
                // Ensure length <=230 (should already be compact; final guard)
                const MAX: usize = 230;
                if out.as_bytes().len() > MAX { out.truncate(MAX); }
                Ok(out)
            }
            // Admin commands for moderators and sysops
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
            _ => Ok("Unknown command. Type HELP\n".to_string())
        }
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