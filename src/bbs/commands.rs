use anyhow::Result;
use log::{debug, warn};

use crate::storage::Storage;
use super::session::{Session, SessionState};

/// Processes BBS commands from users
pub struct CommandProcessor;

impl CommandProcessor {
    pub fn new() -> Self {
        CommandProcessor
    }

    /// Process a command and return a response
    pub async fn process(&self, session: &mut Session, command: &str, storage: &mut Storage) -> Result<String> {
        let cmd = command.trim().to_uppercase();
        
        match session.state {
            SessionState::Connected => self.handle_initial_connection(session, &cmd, storage).await,
            SessionState::LoggingIn => self.handle_login(session, &cmd, storage).await,
            SessionState::MainMenu => self.handle_main_menu(session, &cmd, storage).await,
            SessionState::MessageAreas => self.handle_message_areas(session, &cmd, storage).await,
            SessionState::ReadingMessages => self.handle_reading_messages(session, &cmd, storage).await,
            SessionState::PostingMessage => self.handle_posting_message(session, &cmd, storage).await,
            SessionState::FileAreas => self.handle_file_areas(session, &cmd, storage).await,
            SessionState::UserMenu => self.handle_user_menu(session, &cmd, storage).await,
            SessionState::Disconnected => Ok("Session disconnected.".to_string()),
        }
    }

    async fn handle_initial_connection(&self, session: &mut Session, cmd: &str, _storage: &mut Storage) -> Result<String> {
        session.state = SessionState::MainMenu;
        
        Ok(format!(
            "Welcome to MeshBBS!\nNode: {}\nType HELP for commands\nMain Menu:\n[M]essages [F]iles [U]ser [Q]uit\n>",
            session.node_id
        ))
    }

    async fn handle_login(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        // Simple login - in a real system, you'd validate credentials
        if cmd.starts_with("LOGIN ") {
            let username = cmd.strip_prefix("LOGIN ").unwrap_or("Guest").to_string();
            session.login(username.clone(), 1).await?;
            
            // Store/update user in storage
            storage.create_or_update_user(&username, &session.node_id).await?;
            
            Ok(format!("Welcome {}!\nMain Menu:\n[M]essages [F]iles [U]ser [Q]uit\n>", username))
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
                for (i, area) in areas.iter().enumerate() {
                    response.push_str(&format!("{}. {}\n", i + 1, area));
                }
                response.push_str("[R]ead [P]ost [L]ist [B]ack\n>");
                Ok(response)
            }
            "F" | "FILES" => {
                session.state = SessionState::FileAreas;
                Ok("File Areas:\n[L]ist [U]pload [D]ownload [B]ack\n>".to_string())
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
            "Q" | "QUIT" | "GOODBYE" | "BYE" => {
                session.logout().await?;
                Ok("Goodbye! 73s".to_string())
            }
            "HELP" | "?" => {
                Ok("Commands:\n[M]essages - Read/post messages\n[F]iles - File transfer\n[U]ser - User settings\n[Q]uit - Logout\n>".to_string())
            }
            _ => {
                Ok("Unknown command. Type HELP for commands.\n>".to_string())
            }
        }
    }

    async fn handle_message_areas(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "R" | "READ" => {
                session.state = SessionState::ReadingMessages;
                session.current_area = Some("general".to_string());
                
                let messages = storage.get_messages("general", 10).await?;
                let mut response = "Recent messages in General:\n".to_string();
                
                for msg in messages {
                    response.push_str(&format!(
                        "From: {} | {}\n{}\n---\n",
                        msg.author,
                        msg.timestamp.format("%m/%d %H:%M"),
                        msg.content
                    ));
                }
                
                response.push_str("[N]ext [P]rev [R]eply [B]ack\n>");
                Ok(response)
            }
            "P" | "POST" => {
                session.state = SessionState::PostingMessage;
                Ok("Enter your message (end with . on a line):\n>".to_string())
            }
            "L" | "LIST" => {
                let areas = storage.list_message_areas().await?;
                let mut response = "Available areas:\n".to_string();
                for area in areas {
                    response.push_str(&format!("- {}\n", area));
                }
                response.push_str("\n>");
                Ok(response)
            }
            "B" | "BACK" => {
                session.state = SessionState::MainMenu;
                Ok("Main Menu:\n[M]essages [F]iles [U]ser [Q]uit\n>".to_string())
            }
            _ => {
                Ok("Commands: [R]ead [P]ost [L]ist [B]ack\n>".to_string())
            }
        }
    }

    async fn handle_reading_messages(&self, session: &mut Session, cmd: &str, _storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "B" | "BACK" => {
                session.state = SessionState::MessageAreas;
                Ok("Message Areas:\n[R]ead [P]ost [L]ist [B]ack\n>".to_string())
            }
            _ => {
                Ok("Commands: [N]ext [P]rev [R]eply [B]ack\n>".to_string())
            }
        }
    }

    async fn handle_posting_message(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        if cmd == "." {
            session.state = SessionState::MessageAreas;
            Ok("Message posted!\nMessage Areas:\n[R]ead [P]ost [L]ist [B]ack\n>".to_string())
        } else {
            // In a real implementation, you'd collect the message content
            // and store it when the user enters "."
            let area = session.current_area.as_ref().unwrap_or(&"general".to_string()).clone();
            let author = session.display_name();
            
            storage.store_message(&area, &author, cmd).await?;
            
            session.state = SessionState::MessageAreas;
            Ok("Message posted!\nMessage Areas:\n[R]ead [P]ost [L]ist [B]ack\n>".to_string())
        }
    }

    async fn handle_file_areas(&self, session: &mut Session, cmd: &str, _storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "L" | "LIST" => {
                Ok("Available files:\n(File listing not implemented yet)\n[L]ist [U]pload [D]ownload [B]ack\n>".to_string())
            }
            "B" | "BACK" => {
                session.state = SessionState::MainMenu;
                Ok("Main Menu:\n[M]essages [F]iles [U]ser [Q]uit\n>".to_string())
            }
            _ => {
                Ok("Commands: [L]ist [U]pload [D]ownload [B]ack\n>".to_string())
            }
        }
    }

    async fn handle_user_menu(&self, session: &mut Session, cmd: &str, storage: &mut Storage) -> Result<String> {
        match &cmd[..] {
            "I" | "INFO" => {
                Ok(format!(
                    "User Information:\nUsername: {}\nNode ID: {}\nAccess Level: {}\nSession Duration: {} minutes\n>",
                    session.display_name(),
                    session.node_id,
                    session.user_level,
                    session.session_duration().num_minutes()
                ))
            }
            "S" | "STATS" => {
                let stats = storage.get_statistics().await?;
                Ok(format!(
                    "BBS Statistics:\nTotal Messages: {}\nTotal Users: {}\nUptime: Connected\n>",
                    stats.total_messages,
                    stats.total_users
                ))
            }
            "B" | "BACK" => {
                session.state = SessionState::MainMenu;
                Ok("Main Menu:\n[M]essages [F]iles [U]ser [Q]uit\n>".to_string())
            }
            _ => {
                Ok("Commands: [I]nfo [S]tats [B]ack\n>".to_string())
            }
        }
    }
}