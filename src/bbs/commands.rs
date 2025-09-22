use anyhow::Result;
// use log::{debug}; // retained for future detailed command tracing

use crate::storage::Storage;

fn self_area_can_read(user_level: u8, area: &str, storage: &Storage) -> bool {
    if let Some((r,_)) = storage.get_area_levels(area) { user_level >= r } else { true }
}
fn self_area_can_post(user_level: u8, area: &str, storage: &Storage) -> bool {
    if let Some((_,p)) = storage.get_area_levels(area) { user_level >= p } else { true }
}
use super::session::{Session, SessionState};

/// Processes BBS commands from users
pub struct CommandProcessor;

impl CommandProcessor {
    pub fn new() -> Self { CommandProcessor }

    /// Process a command and return a response
    pub async fn process(&self, session: &mut Session, command: &str, storage: &mut Storage) -> Result<String> {
        let raw = command.trim();
        let cmd_upper = raw.to_uppercase();
        match session.state {
            SessionState::Connected => self.handle_initial_connection(session, &cmd_upper, storage).await,
            SessionState::LoggingIn => self.handle_login(session, &cmd_upper, storage).await,
            SessionState::MainMenu => {
                if let Some(resp) = self.try_inline_message_command(session, raw, &cmd_upper, storage).await? { return Ok(resp); }
                self.handle_main_menu(session, &cmd_upper, storage).await
            }
            SessionState::MessageAreas => {
                if let Some(resp) = self.try_inline_message_command(session, raw, &cmd_upper, storage).await? { return Ok(resp); }
                self.handle_message_areas(session, &cmd_upper, storage).await
            }
            SessionState::ReadingMessages => self.handle_reading_messages(session, &cmd_upper, storage).await,
            SessionState::PostingMessage => self.handle_posting_message(session, &cmd_upper, storage).await,
            SessionState::UserMenu => self.handle_user_menu(session, &cmd_upper, storage).await,
            SessionState::Disconnected => Ok("Session disconnected.".to_string()),
        }
    }

    async fn try_inline_message_command(&self, session: &mut Session, raw: &str, upper: &str, storage: &mut Storage) -> Result<Option<String>> {
        if upper.starts_with("READ") {
            let area = raw.split_whitespace().nth(1).unwrap_or("general").to_lowercase();
            // Permission check
            if !self_area_can_read(session.user_level, &area, storage) { return Ok(Some("Permission denied.\n>".into())); }
            session.current_area = Some(area.clone());
            let messages = storage.get_messages(&area, 10).await?;
            let mut response = format!("Messages in {}:\n", area);
            for msg in messages { response.push_str(&format!("{} | {}\n{}\n---\n", msg.author, msg.timestamp.format("%m/%d %H:%M"), msg.content)); }
            response.push_str(">\n");
            return Ok(Some(response));
        }
        if upper.starts_with("POST ") {
            let mut parts = raw.splitn(3, ' '); parts.next();
            let second = parts.next();
            let (area, text) = if let Some(s) = second { if let Some(rest) = parts.next() { (s.to_lowercase(), rest) } else { (session.current_area.clone().unwrap_or("general".into()), s) } } else { (session.current_area.clone().unwrap_or("general".into()), "") };
            if text.is_empty() { return Ok(Some("Usage: POST [area] <message>".into())); }
            if storage.is_area_locked(&area) { return Ok(Some("Area locked.\n>".into())); }
            if !self_area_can_post(session.user_level, &area, storage) { return Ok(Some("Permission denied.\n>".into())); }
            let author = session.display_name();
            storage.store_message(&area, &author, text).await?;
            return Ok(Some(format!("Posted to {}.\n>", area)));
        }
        if upper == "AREAS" || upper == "LIST" {
            let areas = storage.list_message_areas().await?;
            let mut response = "Areas:\n".to_string();
            for a in areas { if self_area_can_read(session.user_level, &a, storage) { response.push_str(&format!("- {}\n", a)); } }
            response.push_str(">\n");
            return Ok(Some(response));
        }
        Ok(None)
    }

    async fn handle_initial_connection(&self, session: &mut Session, _cmd: &str, _storage: &mut Storage) -> Result<String> {
        session.state = SessionState::MainMenu;
        Ok(format!(
            "Welcome to MeshBBS!\nNode: {}\nAuth: REGISTER <user> <pass> or LOGIN <user> [pass]\nType HELP for commands\nMain Menu:\n[M]essages [U]ser [Q]uit\n>",
            session.node_id
        ))
    }

    async fn handle_login(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        if cmd.starts_with("LOGIN ") {
            let username = cmd.strip_prefix("LOGIN ").unwrap_or("Guest").to_string();
            session.login(username.clone(), 1).await?;
            storage.create_or_update_user(&username, &session.node_id).await?;
            Ok(format!("Welcome {}!\nMain Menu:\n[M]essages [U]ser [Q]uit\n>", username))
        } else {
            Ok("Please enter: LOGIN <username>\n>".to_string())
        }
    }

    async fn handle_main_menu(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "M" | "MESSAGES" => {
                session.state = SessionState::MessageAreas;
                let areas = storage.list_message_areas().await?;
                let mut response = "Message Areas:\n".to_string();
                for (i, area) in areas.iter().enumerate() { response.push_str(&format!("{}. {}\n", i + 1, area)); }
                response.push_str("[R]ead [P]ost [L]ist [B]ack\n>");
                Ok(response)
            }
            "U" | "USER" => {
                session.state = SessionState::UserMenu;
                Ok(format!(
                    "User Menu:\nUsername: {}\nLevel: {}\nLogin time: {}\n[I]nfo [S]tats [B]ack\n>",
                    session.display_name(),
                    session.user_level,
                    session.login_time.format("%Y-%m-%d %H:%M UTC")
                ))
            }
            "Q" | "QUIT" | "GOODBYE" | "BYE" => { session.logout().await?; Ok("Goodbye! 73s".to_string()) }
            "HELP" | "?" => {
                let mut lines = Vec::new();
                if !session.is_logged_in() {
                    lines.push("REGISTER <u> <p> - Create account");
                    lines.push("LOGIN <u> [p] - Login");
                } else {
                    // Show password management depending on current account state
                    // We cannot easily know if password set without storage here, so give both hints minimalistically.
                    lines.push("SETPASS <new> - Set password (if none)");
                    lines.push("CHPASS <old> <new> - Change password");
                    if session.user_level >= 5 {
                        lines.push("DELETE <area> <id> - Delete a message");
                        lines.push("LOCK <area> / UNLOCK <area> - Control posting");
                        lines.push("DELLOG [page] - View deletion audit log (page size 10)");
                    }
                    if session.user_level >= 10 { lines.push("PROMOTE <user> | DEMOTE <user> - Manage roles"); }
                }
                lines.push("[M]essages - Read/post messages");
                lines.push("[U]ser - User settings");
                lines.push("[Q]uit - Logout");
                let mut out = String::from("Commands:\n");
                out.push_str(&lines.join("\n"));
                out.push_str("\n>");
                Ok(out)
            }
            _ => Ok("Unknown command. Type HELP for commands.\n>".to_string())
        }
    }

    async fn handle_message_areas(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "R" | "READ" => {
                session.state = SessionState::ReadingMessages;
                session.current_area = Some("general".to_string());
                let messages = storage.get_messages("general", 10).await?;
                let mut response = "Recent messages in General:\n".to_string();
                for msg in messages { response.push_str(&format!("From: {} | {}\n{}\n---\n", msg.author, msg.timestamp.format("%m/%d %H:%M"), msg.content)); }
                response.push_str("[N]ext [P]rev [R]eply [B]ack\n>");
                Ok(response)
            }
            "P" | "POST" => { session.state = SessionState::PostingMessage; Ok("Enter your message (end with . on a line):\n>".to_string()) }
            "L" | "LIST" => {
                let areas = storage.list_message_areas().await?;
                let mut response = "Available areas:\n".to_string();
                for area in areas { response.push_str(&format!("- {}\n", area)); }
                response.push_str("\n>");
                Ok(response)
            }
            "B" | "BACK" => { session.state = SessionState::MainMenu; Ok("Main Menu:\n[M]essages [U]ser [Q]uit\n>".to_string()) }
            _ => Ok("Commands: [R]ead [P]ost [L]ist [B]ack\n>".to_string())
        }
    }

    async fn handle_reading_messages(&self, session: &mut Session, cmd: &str, _storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "B" | "BACK" => { session.state = SessionState::MessageAreas; Ok("Message Areas:\n[R]ead [P]ost [L]ist [B]ack\n>".to_string()) }
            _ => Ok("Commands: [N]ext [P]rev [R]eply [B]ack\n>".to_string())
        }
    }

    async fn handle_posting_message(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        if cmd == "." {
            session.state = SessionState::MessageAreas;
            Ok("Message posted!\nMessage Areas:\n[R]ead [P]ost [L]ist [B]ack\n>".to_string())
        } else {
            let area = session.current_area.as_ref().unwrap_or(&"general".to_string()).clone();
            let author = session.display_name();
            storage.store_message(&area, &author, cmd).await?;
            session.state = SessionState::MessageAreas;
            Ok("Message posted!\nMessage Areas:\n[R]ead [P]ost [L]ist [B]ack\n>".to_string())
        }
    }

    async fn handle_user_menu(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "I" | "INFO" => Ok(format!(
                "User Information:\nUsername: {}\nNode ID: {}\nAccess Level: {}\nSession Duration: {} minutes\n>",
                session.display_name(), session.node_id, session.user_level, session.session_duration().num_minutes()
            )),
            "S" | "STATS" => {
                let stats = storage.get_statistics().await?;
                Ok(format!(
                    "BBS Statistics:\nTotal Messages: {}\nTotal Users: {}\nUptime: Connected\n>",
                    stats.total_messages, stats.total_users
                ))
            }
            "B" | "BACK" => { session.state = SessionState::MainMenu; Ok("Main Menu:\n[M]essages [U]ser [Q]uit\n>".to_string()) }
            _ => Ok("Commands: [I]nfo [S]tats [B]ack\n>".to_string())
        }
    }
}