use anyhow::Result;
use log::{info, debug};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::storage::Storage;
use super::commands::CommandProcessor;

/// Represents a user session on the BBS
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub node_id: String,
    pub username: Option<String>,
    pub user_level: u8,
    pub current_area: Option<String>,
    pub login_time: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub state: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionState {
    Connected,
    LoggingIn,
    MainMenu,
    MessageAreas,
    ReadingMessages,
    PostingMessage,
    FileAreas,
    UserMenu,
    Disconnected,
}

impl Session {
    /// Create a new session
    pub fn new(id: String, node_id: String) -> Self {
        let now = Utc::now();
        
        Session {
            id,
            node_id,
            username: None,
            user_level: 0,
            current_area: None,
            login_time: now,
            last_activity: now,
            state: SessionState::Connected,
        }
    }

    /// Process a command from the user
    pub async fn process_command(&mut self, command: &str, storage: &mut Storage) -> Result<String> {
        self.update_activity();
        
        debug!("Session {}: Processing command: {}", self.id, command);
        
        let processor = CommandProcessor::new();
        let response = processor.process(self, command, storage).await?;
        
        Ok(response)
    }

    /// Update the last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = Utc::now();
    }

    /// Log in a user
    pub async fn login(&mut self, username: String, user_level: u8) -> Result<()> {
        info!("User {} logged in from node {}", username, self.node_id);
        
        self.username = Some(username);
        self.user_level = user_level;
        self.state = SessionState::MainMenu;
        
        Ok(())
    }

    /// Log out the user
    pub async fn logout(&mut self) -> Result<()> {
        if let Some(ref username) = self.username {
            info!("User {} logged out from node {}", username, self.node_id);
        }
        
        self.username = None;
        self.user_level = 0;
        self.current_area = None;
        self.state = SessionState::Disconnected;
        
        Ok(())
    }

    /// Check if the user is logged in
    pub fn is_logged_in(&self) -> bool {
        self.username.is_some()
    }

    /// Get the username, or "Guest" if not logged in
    pub fn display_name(&self) -> String {
        self.username.clone().unwrap_or_else(|| "Guest".to_string())
    }

    /// Check if the user has sufficient access level
    pub fn has_access(&self, required_level: u8) -> bool {
        self.user_level >= required_level
    }

    /// Get session duration
    pub fn session_duration(&self) -> chrono::Duration {
        self.last_activity - self.login_time
    }

    /// Check if session is inactive (for cleanup)
    pub fn is_inactive(&self, timeout_minutes: i64) -> bool {
        let now = Utc::now();
        let timeout = chrono::Duration::minutes(timeout_minutes);
        now - self.last_activity > timeout
    }
}