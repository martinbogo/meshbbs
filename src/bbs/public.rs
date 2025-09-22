use std::collections::HashMap;
use std::time::{Instant, Duration};

#[derive(Debug, Clone)]
pub struct PendingLogin {
    pub requested_username: String,
    pub created_at: Instant,
}

#[derive(Debug, Default)]
pub struct PublicState {
    pub pending: HashMap<String, PendingLogin>, // node_id -> pending login
    pub last_public_reply: HashMap<String, Instant>, // rate limit map
    pub reply_cooldown: Duration,
    pub pending_timeout: Duration,
}

impl PublicState {
    pub fn new(reply_cooldown: Duration, pending_timeout: Duration) -> Self {
        Self { pending: HashMap::new(), last_public_reply: HashMap::new(), reply_cooldown, pending_timeout }
    }

    pub fn prune_expired(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, v| now.duration_since(v.created_at) < self.pending_timeout);
    }

    pub fn set_pending(&mut self, node_id: &str, username: String) {
        self.pending.insert(node_id.to_string(), PendingLogin { requested_username: username, created_at: Instant::now() });
    }

    pub fn take_pending(&mut self, node_id: &str) -> Option<String> {
        self.pending.remove(node_id).map(|p| p.requested_username)
    }

    pub fn should_reply(&mut self, node_id: &str) -> bool {
        let now = Instant::now();
        match self.last_public_reply.get(node_id) {
            Some(last) if now.duration_since(*last) < self.reply_cooldown => false,
            _ => { self.last_public_reply.insert(node_id.to_string(), now); true }
        }
    }
}

/// Minimal public channel command parser
pub struct PublicCommandParser;

impl PublicCommandParser {
    pub fn new() -> Self { Self }

    pub fn parse(&self, raw: &str) -> PublicCommand {
        let trimmed = raw.trim();
        // Require caret prefix for public commands to reduce accidental noise
        if !trimmed.starts_with('^') { return PublicCommand::Unknown; }
        let body = &trimmed[1..];
        if body.eq_ignore_ascii_case("HELP") || body == "?" { return PublicCommand::Help; }
        if body.len() >= 5 && body[..5].eq_ignore_ascii_case("LOGIN") {
            if body.len() == 5 { return PublicCommand::Invalid("Username required".into()); }
            let after = &body[5..];
            if after.chars().next().map(|c| c.is_whitespace()).unwrap_or(false) {
                let user = after.trim();
                if user.is_empty() { return PublicCommand::Invalid("Username required".into()); }
                return PublicCommand::Login(user.to_string());
            }
        }
        PublicCommand::Unknown
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PublicCommand {
    Help,
    Login(String),
    Unknown,
    Invalid(String),
}
