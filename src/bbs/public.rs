use log::trace;
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
    // Separate, lighter cooldown for high-churn public commands like ^SLOT
    pub slot_last_spin: HashMap<String, Instant>, // node_id -> last spin time
    pub slot_cooldown: Duration,
    // Lightweight cooldown for ^8BALL
    pub eightball_last: HashMap<String, Instant>,
    pub eightball_cooldown: Duration,
    // Lightweight cooldown for ^FORTUNE
    pub fortune_last: HashMap<String, Instant>,
    pub fortune_cooldown: Duration,
}

impl PublicState {
    pub fn new(reply_cooldown: Duration, pending_timeout: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            last_public_reply: HashMap::new(),
            reply_cooldown,
            pending_timeout,
            slot_last_spin: HashMap::new(),
            slot_cooldown: Duration::from_secs(3),
            eightball_last: HashMap::new(),
            eightball_cooldown: Duration::from_secs(2),
            fortune_last: HashMap::new(),
            fortune_cooldown: Duration::from_secs(5),
        }
    }

    pub fn prune_expired(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, v| now.duration_since(v.created_at) < self.pending_timeout);
        // Keep slot entries reasonably small; drop entries not touched for 30 minutes
        let slot_ttl = Duration::from_secs(30 * 60);
        self.slot_last_spin.retain(|_, t| now.duration_since(*t) < slot_ttl);
        // Same TTL policy for eightball
        self.eightball_last.retain(|_, t| now.duration_since(*t) < slot_ttl);
        // Same TTL policy for fortune
        self.fortune_last.retain(|_, t| now.duration_since(*t) < slot_ttl);
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

    /// Lightweight, per-node rate limit for ^SLOT. Defaults to 3s between spins.
    pub fn allow_slot(&mut self, node_id: &str) -> bool {
        let now = Instant::now();
        match self.slot_last_spin.get(node_id) {
            Some(last) if now.duration_since(*last) < self.slot_cooldown => false,
            _ => { self.slot_last_spin.insert(node_id.to_string(), now); true }
        }
    }

    /// Lightweight, per-node rate limit for ^8BALL. Defaults to 2s between questions.
    pub fn allow_8ball(&mut self, node_id: &str) -> bool {
        let now = Instant::now();
        match self.eightball_last.get(node_id) {
            Some(last) if now.duration_since(*last) < self.eightball_cooldown => false,
            _ => { self.eightball_last.insert(node_id.to_string(), now); true }
        }
    }

    /// Lightweight, per-node rate limit for ^FORTUNE. Defaults to 5s between fortunes.
    pub fn allow_fortune(&mut self, node_id: &str) -> bool {
        let now = Instant::now();
        match self.fortune_last.get(node_id) {
            Some(last) if now.duration_since(*last) < self.fortune_cooldown => false,
            _ => { self.fortune_last.insert(node_id.to_string(), now); true }
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
    if body.eq_ignore_ascii_case("HELP") || body == "?" { trace!("Parsed HELP from '{}'" , raw); return PublicCommand::Help; }
    // WEATHER command: accept optional trailing arguments (ignored for now)
    if body.len() >= 7 && body[..7].eq_ignore_ascii_case("WEATHER")
        && (body.len() == 7 || body.chars().nth(7).map(|c| c.is_whitespace()).unwrap_or(false)) {
        trace!("Parsed WEATHER from '{}' (args ignored)", raw);
        return PublicCommand::Weather;
    }
        // SLOT machine command: ^SLOTMACHINE or ^SLOT
        if body.eq_ignore_ascii_case("SLOTMACHINE") || body.eq_ignore_ascii_case("SLOT") {
            trace!("Parsed SLOTMACHINE from '{}'", raw);
            return PublicCommand::SlotMachine;
        }
        // Magic 8-Ball: ^8BALL
        if body.eq_ignore_ascii_case("8BALL") {
            trace!("Parsed 8BALL from '{}'", raw);
            return PublicCommand::EightBall;
        }
        // Fortune cookies: ^FORTUNE
        if body.eq_ignore_ascii_case("FORTUNE") {
            trace!("Parsed FORTUNE from '{}'", raw);
            return PublicCommand::Fortune;
        }
        // Slot stats: ^SLOTSTATS
        if body.eq_ignore_ascii_case("SLOTSTATS") {
            trace!("Parsed SLOTSTATS from '{}'", raw);
            return PublicCommand::SlotStats;
        }
        if body.len() >= 5 && body[..5].eq_ignore_ascii_case("LOGIN") {
            if body.len() == 5 { return PublicCommand::Invalid("Username required".into()); }
            let after = &body[5..];
            if after.chars().next().map(|c| c.is_whitespace()).unwrap_or(false) {
                let user = after.trim();
                if user.is_empty() { return PublicCommand::Invalid("Username required".into()); }
                trace!("Parsed LOGIN '{}' from '{}'", user, raw);
                return PublicCommand::Login(user.to_string());
            }
        }
        PublicCommand::Unknown
    }
}

impl Default for PublicCommandParser { fn default() -> Self { Self::new() } }

#[derive(Debug, PartialEq, Eq)]
pub enum PublicCommand {
    Help,
    Login(String),
    Weather,
    SlotMachine,
    SlotStats,
    EightBall,
    Fortune,
    Unknown,
    Invalid(String),
}
