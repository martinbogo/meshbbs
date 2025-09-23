use anyhow::Result;
use log::debug;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::storage::Storage;
use super::commands::CommandProcessor;

/// # User Session Management
///
/// Represents an active user session on the BBS system. Each session tracks
/// a user's connection state, authentication status, current location within
/// the BBS, and session-specific preferences.
///
/// ## Session Lifecycle
///
/// Sessions progress through several states:
/// 1. **Connected** - Initial connection established
/// 2. **LoggingIn** - User is authenticating
/// 3. **MainMenu** - Authenticated and at main menu
/// 4. **MessageAreas** - Browsing message areas
/// 5. **ReadingMessages** - Reading messages in an area
/// 6. **PostingMessage** - Composing a new message
/// 7. **UserMenu** - Managing user account settings
/// 8. **Disconnected** - Session ended
///
/// ## Usage
///
/// ```rust,no_run
/// use meshbbs::bbs::session::{Session, SessionState};
///
/// // Create new session for a connecting user
/// let session = Session::new("session_123".to_string(), "node_456".to_string());
/// 
/// // Sessions start in Connected state
/// assert!(matches!(session.state, SessionState::Connected));
/// ```
///
/// ## Authentication
///
/// Sessions track authentication state through the `username` and `user_level` fields:
/// - `username: None` - User not authenticated
/// - `username: Some(name)` - User authenticated as 'name'
/// - `user_level` - User's permission level (0=anonymous, 1=user, 5=moderator, 10=sysop)
///
/// ## Location Tracking
///
/// The session tracks the user's current location in the BBS:
/// - `current_area` - Current message area (if any)
/// - `state` - Current menu/interface state
///
/// ## Session Management
///
/// Sessions are managed by the BBS server and include automatic:
/// - **Timeout handling** - Sessions expire after configured inactivity
/// - **State persistence** - Session state survives across message exchanges
/// - **Activity tracking** - Last activity timestamp for timeout calculations
/// - **Label management** - Short and long display names for the session
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub node_id: String,
    pub short_label: Option<String>,
    pub long_label: Option<String>,
    pub username: Option<String>,
    pub user_level: u8,
    pub current_area: Option<String>,
    /// Whether the abbreviated HELP has already been shown this session (used to append shortcuts line once)
    pub help_seen: bool,
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
            short_label: None,
            long_label: None,
            username: None,
            user_level: 0,
            current_area: None,
            help_seen: false,
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
        // Logging handled in server (needs node database context)
        self.username = Some(username);
        self.user_level = user_level;
        self.state = SessionState::MainMenu;
        
        Ok(())
    }

    /// Log out the user
    pub async fn logout(&mut self) -> Result<()> {
        // Logging handled in server
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

    #[allow(dead_code)]
    pub fn display_node_short(&self) -> String {
        self.short_label.clone().unwrap_or_else(|| {
            if let Ok(n) = self.node_id.parse::<u32>() { format!("0x{:06X}", n & 0xFFFFFF) } else { self.node_id.clone() }
        })
    }

    #[allow(dead_code)]
    pub fn display_node_long(&self) -> String {
        self.long_label.clone().unwrap_or_else(|| self.display_node_short())
    }

    pub fn update_labels(&mut self, short: Option<String>, long: Option<String>) {
        if let Some(s) = short { if !s.is_empty() { self.short_label = Some(s); } }
        if let Some(l) = long { if !l.is_empty() { self.long_label = Some(l); } }
    }

    /// Check if the user has sufficient access level
    #[allow(dead_code)]
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

    /// Build a dynamic prompt string based on session state.
    /// 
    /// ## Prompt Formats
    /// 
    /// All prompts end with `>`:
    /// - Unauthenticated: `"unauth>"`
    /// - Main/menu (logged in): `"username (lvl1)>"`
    /// - Reading messages/in area: `"username@area>"` (area truncated to 20 chars)
    /// - Posting: `"post@area>"` (falls back to `"post>"` if no area)
    pub fn build_prompt(&self) -> String {
        // Unauthenticated
        if !self.is_logged_in() {
            return "unauth>".to_string();
        }

        let level = self.user_level;
        match self.state {
            SessionState::PostingMessage => {
                if let Some(area) = &self.current_area { format!("post@{}>", Self::truncate_area(area)) } else { "post>".into() }
            }
            SessionState::ReadingMessages | SessionState::MessageAreas => {
                if let Some(area) = &self.current_area { format!("{}@{}>", self.display_name(), Self::truncate_area(area)) } else { format!("{} (lvl{})>", self.display_name(), level) }
            }
            SessionState::MainMenu | SessionState::UserMenu | SessionState::LoggingIn | SessionState::Connected => {
                format!("{} (lvl{})>", self.display_name(), level)
            }
            SessionState::Disconnected => "".to_string(), // no prompt after disconnect
        }
    }

    fn truncate_area(area: &str) -> String {
        const MAX: usize = 20;
        if area.len() <= MAX { area.to_string() } else { format!("{}…", &area[..MAX-1]) }
    }
}