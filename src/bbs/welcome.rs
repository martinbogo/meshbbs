//! New User Welcome System
//!
//! Automatically greets nodes with default Meshtastic names and helps them get started.
//!
//! Features:
//! - Detects default "Meshtastic XXXX" node names
//! - Sends friendly public greeting to mesh
//! - Sends private DM with setup instructions and fun name suggestion
//! - Rate-limited to prevent spam (5 minutes between welcomes)
//! - Persistent tracking to avoid re-welcoming same nodes

use log::{debug, info, warn};
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

// Re-export for compatibility
pub use crate::config::WelcomeAppConfig as WelcomeConfig;

/// Persistent record of a welcomed node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeRecord {
    /// When first welcomed
    pub timestamp: SystemTime,
    /// Number of times welcomed
    pub count: u32,
    /// Node name at time of welcome
    pub name: String,
}

/// Runtime state for welcome system
pub struct WelcomeState {
    /// Map of node_id -> welcome record
    welcomed_nodes: HashMap<u32, WelcomeRecord>,
    /// Last time any welcome was sent (rate limiting)
    last_welcome_time: Option<Instant>,
    /// Path to persistent state file
    state_path: std::path::PathBuf,
}

impl WelcomeState {
    /// Create new state, loading from disk if available
    pub fn new(data_dir: &str) -> Self {
        let state_path = std::path::Path::new(data_dir).join("welcomed_nodes.json");
        let welcomed_nodes = Self::load_state(&state_path).unwrap_or_default();

        info!(
            "Welcome system initialized: {} nodes tracked",
            welcomed_nodes.len()
        );

        Self {
            welcomed_nodes,
            last_welcome_time: None,
            state_path,
        }
    }

    /// Load welcomed nodes from disk
    fn load_state(path: &std::path::Path) -> Option<HashMap<u32, WelcomeRecord>> {
        match std::fs::read_to_string(path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(data) => Some(data),
                Err(e) => {
                    warn!("Failed to parse welcomed_nodes.json: {}; starting fresh", e);
                    None
                }
            },
            Err(_) => {
                debug!("No welcomed_nodes.json found; starting with empty state");
                None
            }
        }
    }

    /// Save state to disk atomically
    fn save_state(&self) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.welcomed_nodes)?;
        std::fs::write(&self.state_path, json)?;
        Ok(())
    }

    /// Check if node should be welcomed
    pub fn should_welcome(
        &self,
        node_id: u32,
        node_name: &str,
        config: &WelcomeConfig,
        skip_rate_limit: bool, // true for startup queue, false for real-time
    ) -> bool {
        // Must have default name pattern
        if !is_default_name(node_name) {
            return false;
        }

        // Check global rate limit (skip for startup queue welcomes)
        if !skip_rate_limit {
            if let Some(last_time) = self.last_welcome_time {
                let elapsed = last_time.elapsed();
                let cooldown = Duration::from_secs(config.cooldown_minutes as u64 * 60);
                if elapsed < cooldown {
                    let remaining = cooldown.saturating_sub(elapsed).as_secs();
                    debug!("Welcome rate limit active: {}s remaining", remaining);
                    return false;
                }
            }
        }

        // Check per-node limit
        if let Some(record) = self.welcomed_nodes.get(&node_id) {
            if record.count >= config.max_welcomes_per_node {
                debug!(
                    "Node 0x{:08X} already welcomed {} times (max: {})",
                    node_id, record.count, config.max_welcomes_per_node
                );
                return false;
            }
        }

        true
    }

    /// Record that a node was welcomed
    pub fn record_welcome(&mut self, node_id: u32, node_name: &str) {
        let record = self
            .welcomed_nodes
            .entry(node_id)
            .or_insert_with(|| WelcomeRecord {
                timestamp: SystemTime::now(),
                count: 0,
                name: node_name.to_string(),
            });

        record.count += 1;
        record.timestamp = SystemTime::now();
        record.name = node_name.to_string();

        self.last_welcome_time = Some(Instant::now());

        info!(
            "Recorded welcome for node 0x{:08X} ({}): count={}",
            node_id, node_name, record.count
        );

        // Persist to disk
        if let Err(e) = self.save_state() {
            warn!("Failed to save welcome state: {}", e);
        }
    }

    /// Get stats for logging/debugging
    pub fn stats(&self) -> WelcomeStats {
        let cooldown_remaining = self
            .last_welcome_time
            .map(|t| {
                let elapsed = t.elapsed().as_secs();
                (5u64 * 60).saturating_sub(elapsed)
            })
            .unwrap_or(0);

        WelcomeStats {
            total_nodes_welcomed: self.welcomed_nodes.len(),
            cooldown_remaining_secs: cooldown_remaining,
        }
    }
}

/// Statistics about welcome system state
#[derive(Debug)]
pub struct WelcomeStats {
    pub total_nodes_welcomed: usize,
    pub cooldown_remaining_secs: u64,
}

/// Check if a node name matches the default pattern
pub fn is_default_name(name: &str) -> bool {
    // Pattern: "Meshtastic XXXX" where XXXX is 4 hex digits
    if !name.starts_with("Meshtastic ") {
        return false;
    }

    let suffix = &name[11..]; // After "Meshtastic "
    if suffix.len() != 4 {
        return false;
    }

    suffix.chars().all(|c| c.is_ascii_hexdigit())
}

/// Generate a fun random callsign (adjective + animal) with emoji
pub fn generate_callsign() -> (String, String) {
    let mut rng = rand::thread_rng();

    let adjective = ADJECTIVES.choose(&mut rng).unwrap();
    let animal = ANIMALS.choose(&mut rng).unwrap();
    let emoji = get_animal_emoji(animal);

    let callsign = format!("{} {}", adjective, animal);
    let emoji_string = emoji.to_string();

    (callsign, emoji_string)
}

/// Generate public greeting message
pub fn public_greeting(node_name: &str) -> String {
    let templates = [
        format!(
            "🎉 Welcome to the mesh, {}! Check your DMs for a quick setup tip.",
            node_name
        ),
        format!(
            "👋 Hey {}, welcome aboard! MeshBBS sent you a getting-started message.",
            node_name
        ),
        format!(
            "🌐 {} just joined the mesh! Welcome - check your DMs for help.",
            node_name
        ),
    ];

    let mut rng = rand::thread_rng();
    templates[rng.gen_range(0..templates.len())].clone()
}

/// Generate private guide DM
pub fn private_guide(
    node_name: &str,
    suggested_callsign: &str,
    emoji: &str,
    cmd_prefix: char,
    help_cmd: &str,
) -> String {
    format!(
        "👋 Welcome to Meshtastic, {}!\n\
        \n\
        We noticed you're using the default name. Personalize it:\n\
        \n\
        📱 App → CONFIG → USER → \"Long Name\"\n\
        \n\
        Suggestion: \"{} {}\"\n\
        \n\
        Questions? Send {}{} to explore MeshBBS!",
        node_name, emoji, suggested_callsign, cmd_prefix, help_cmd
    )
}

// Clean, friendly adjectives
const ADJECTIVES: &[&str] = &[
    "Adventurous",
    "Brave",
    "Clever",
    "Daring",
    "Eager",
    "Friendly",
    "Gentle",
    "Happy",
    "Inventive",
    "Jolly",
    "Kind",
    "Lively",
    "Merry",
    "Noble",
    "Optimistic",
    "Playful",
    "Quick",
    "Radiant",
    "Swift",
    "Thoughtful",
    "Upbeat",
    "Valiant",
    "Wise",
    "Zesty",
    "Cheerful",
    "Bold",
    "Cosmic",
    "Dynamic",
    "Electric",
    "Fearless",
    "Graceful",
    "Heroic",
    "Intrepid",
    "Joyful",
    "Keen",
    "Lucky",
    "Mighty",
    "Nimble",
    "Outstanding",
    "Peaceful",
    "Quirky",
    "Resilient",
    "Spirited",
    "Tenacious",
    "Unstoppable",
    "Vibrant",
    "Wonderful",
    "Xtraordinary",
    "Youthful",
    "Zealous",
];

// Fun animal names
const ANIMALS: &[&str] = &[
    "Albatross",
    "Bear",
    "Cheetah",
    "Dolphin",
    "Eagle",
    "Fox",
    "Giraffe",
    "Hawk",
    "Iguana",
    "Jaguar",
    "Kangaroo",
    "Lion",
    "Moose",
    "Narwhal",
    "Otter",
    "Panda",
    "Quail",
    "Raven",
    "Seal",
    "Tiger",
    "Unicorn",
    "Vulture",
    "Wolf",
    "Xerus",
    "Yak",
    "Zebra",
    "Alpaca",
    "Beaver",
    "Condor",
    "Deer",
    "Elephant",
    "Falcon",
    "Gazelle",
    "Heron",
    "Impala",
    "Koala",
    "Leopard",
    "Meerkat",
    "Newt",
    "Owl",
    "Penguin",
    "Rabbit",
    "Squirrel",
    "Turtle",
    "Uakari",
    "Viper",
    "Wombat",
    "Xenops",
    "Yellowhammer",
    "Zebu",
];

/// Get the emoji for a given animal name
fn get_animal_emoji(animal: &str) -> &'static str {
    match animal {
        "Albatross" => "🦅",
        "Bear" => "🐻",
        "Cheetah" => "🐆",
        "Dolphin" => "🐬",
        "Eagle" => "🦅",
        "Fox" => "🦊",
        "Giraffe" => "🦒",
        "Hawk" => "🦅",
        "Iguana" => "🦎",
        "Jaguar" => "🐆",
        "Kangaroo" => "🦘",
        "Lion" => "🦁",
        "Moose" => "🦌",
        "Narwhal" => "🐋",
        "Otter" => "🦦",
        "Panda" => "🐼",
        "Quail" => "🐦",
        "Raven" => "🐦‍⬛",
        "Seal" => "🦭",
        "Tiger" => "🐯",
        "Unicorn" => "🦄",
        "Vulture" => "🦅",
        "Wolf" => "🐺",
        "Xerus" => "🐿️",
        "Yak" => "🦬",
        "Zebra" => "🦓",
        "Alpaca" => "🦙",
        "Beaver" => "🦫",
        "Condor" => "🦅",
        "Deer" => "🦌",
        "Elephant" => "🐘",
        "Falcon" => "🦅",
        "Gazelle" => "🦌",
        "Heron" => "🦩",
        "Impala" => "🦌",
        "Koala" => "🐨",
        "Leopard" => "🐆",
        "Meerkat" => "🦡",
        "Newt" => "🦎",
        "Owl" => "🦉",
        "Penguin" => "🐧",
        "Rabbit" => "🐰",
        "Squirrel" => "🐿️",
        "Turtle" => "🐢",
        "Uakari" => "🐵",
        "Viper" => "🐍",
        "Wombat" => "🦡",
        "Xenops" => "🐦",
        "Yellowhammer" => "🐦",
        "Zebu" => "🐂",
        _ => "🎭", // Fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_name_detection() {
        assert!(is_default_name("Meshtastic A3F2"));
        assert!(is_default_name("Meshtastic 1234"));
        assert!(is_default_name("Meshtastic ABCD"));
        assert!(is_default_name("Meshtastic 0000"));

        assert!(!is_default_name("CoolNode A3F2"));
        assert!(!is_default_name("Meshtastic"));
        assert!(!is_default_name("Meshtastic 123"));
        assert!(!is_default_name("Meshtastic ABCDE"));
        assert!(!is_default_name("Meshtastic GHIJ")); // Not all hex
        assert!(!is_default_name("meshtastic A3F2")); // Wrong case
    }

    #[test]
    fn test_callsign_generation() {
        for _ in 0..10 {
            let (name, emoji) = generate_callsign();
            assert!(name.contains(' '));
            assert!(name.len() < 40);
            assert!(name.len() > 5);

            // Verify it's from our lists
            let parts: Vec<&str> = name.split(' ').collect();
            assert_eq!(parts.len(), 2);
            assert!(ADJECTIVES.contains(&parts[0]));
            assert!(ANIMALS.contains(&parts[1]));

            // Verify emoji is not empty
            assert!(!emoji.is_empty());
        }
    }

    #[test]
    fn test_public_greeting_reasonable_length() {
        let msg = public_greeting("Meshtastic A3F2");
        assert!(msg.len() < 230); // Fits in Meshtastic limit
        assert!(msg.len() > 20);
        assert!(msg.contains("Meshtastic A3F2"));
    }

    #[test]
    fn test_private_guide_contains_key_info() {
        let guide = private_guide("Meshtastic B4E1", "Brave Eagle", "🦅", '^', "HELP");
        assert!(guide.contains("CONFIG"));
        assert!(guide.contains("USER"));
        assert!(guide.contains("Brave Eagle"));
        assert!(guide.contains("🦅"));
        assert!(guide.contains("^HELP"));

        // Test with different prefix and help command
        let guide_alt = private_guide("Meshtastic B4E1", "Brave Eagle", "🦅", '!', "MENU");
        assert!(guide_alt.contains("!MENU"));
    }

    #[test]
    fn test_welcome_state_rate_limiting() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let mut state = WelcomeState::new(tmp.path().to_str().unwrap());
        let config = WelcomeConfig {
            enabled: true,
            cooldown_minutes: 5,
            max_welcomes_per_node: 1,
            ..Default::default()
        };

        // First welcome should be allowed (test with rate limit enabled)
        assert!(state.should_welcome(0xA3F2, "Meshtastic A3F2", &config, false));

        // Record the welcome
        state.record_welcome(0xA3F2, "Meshtastic A3F2");

        // Same node should not be welcomed again (per-node limit)
        assert!(!state.should_welcome(0xA3F2, "Meshtastic A3F2", &config, false));

        // Different node should also be blocked (global cooldown)
        assert!(!state.should_welcome(0xB4E1, "Meshtastic B4E1", &config, false));
    }

    #[test]
    fn test_welcome_state_persistence() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let path = tmp.path().to_str().unwrap();

        // Create state and welcome a node
        {
            let mut state = WelcomeState::new(path);
            state.record_welcome(0xA3F2, "Meshtastic A3F2");
        }

        // Load state again and verify node is tracked
        {
            let state = WelcomeState::new(path);
            let config = WelcomeConfig::default();
            assert!(!state.should_welcome(0xA3F2, "Meshtastic A3F2", &config, false));
        }
    }
}
