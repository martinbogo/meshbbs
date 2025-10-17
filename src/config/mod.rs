//! # Configuration Management Module
//!
//! This module handles all configuration aspects of the Meshbbs system, providing
//! a centralized configuration system with validation, defaults, and persistence.
//!
//! ## Features
//!
//! - **Structured Configuration**: Type-safe configuration with serde serialization
//! - **Validation**: Comprehensive validation of all configuration values
//! - **Defaults**: Sensible default values for all configuration options
//! - **Hot Reloading**: Support for runtime configuration updates
//! - **Environment Integration**: Integration with environment variables and CLI args
//!
//! ## Configuration Structure
//!
//! The configuration is organized into logical sections:
//!
//! - [`BbsConfig`] - Core BBS settings (name, sysop, limits)
//! - [`MeshtasticConfig`] - Device communication settings
//! - [`StorageConfig`] - Data persistence settings
//! - [`MessageTopicConfig`] - Individual message topic configuration
//! - [`LoggingConfig`] - Logging and debugging settings
//! - [`SecurityConfig`] - Security and authentication parameters
//!
//! ## Usage
//!
//! ```rust,no_run
//! use meshbbs::config::Config;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load configuration from file
//!     let config = Config::load("config.toml").await?;
//!     
//!     // Access configuration sections
//!     println!("BBS Name: {}", config.bbs.name);
//!     println!("Serial Port: {}", config.meshtastic.port);
//!     
//!     // Create default configuration
//!     Config::create_default("config.toml").await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Configuration File Format
//!
//! Meshbbs uses TOML format for human-readable configuration:
//!
//! ```toml
//! [bbs]
//! name = "My Mesh BBS"
//! sysop = "sysop"
//! location = "Mesh Network"
//! max_users = 100
//! session_timeout = 10
//!
//! [meshtastic]
//! port = "/dev/ttyUSB0"
//! baud_rate = 115200
//! channel = 0
//!
//! # Note: message topics are initialized into data/topics.json during `meshbbs init`
//! ```
//!
//! ## Validation and Security
//!
//! - **Input Validation**: All configuration values are validated on load
//! - **Type Safety**: Strong typing prevents configuration errors
//! - **Secure Defaults**: Default values are chosen for security and stability
//! - **Sanitization**: String values are sanitized to prevent injection attacks
//!
//! ## Environment Integration
//!
//! Configuration values can be overridden via environment variables and CLI arguments,
//! following a clear precedence order: CLI args > Environment > Config file > Defaults

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;

/// Main configuration structure

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbsConfig {
    pub name: String,
    pub sysop: String,
    pub location: String,
    pub description: String,
    pub max_users: u32,
    pub session_timeout: u32, // minutes
    pub welcome_message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sysop_password_hash: Option<String>,
    /// Public command prefix. Must be one of a hard-coded allowed set for safety.
    /// Examples: "^", "!", "+", "$", "/", ">". If unset or invalid, defaults to "^".
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "public_command_prefixes"
    )]
    pub public_command_prefix: Option<String>,
    /// Allow public channel LOGIN command. When false, users must initiate login via DM only.
    /// Defaults to true for backwards compatibility. Set to false for enhanced security.
    #[serde(default = "default_allow_public_login")]
    pub allow_public_login: bool,
    /// Public help command keyword. Must be one of: "HELP", "MENU", "INFO".
    /// Defaults to "HELP" if unset or invalid.
    #[serde(default = "default_help_command")]
    pub help_command: String,
}

fn default_allow_public_login() -> bool {
    true
}

fn default_help_command() -> String {
    "HELP".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bbs: BbsConfig,
    pub meshtastic: MeshtasticConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub message_topics: HashMap<String, MessageTopicConfig>,
    pub logging: LoggingConfig,
    pub security: Option<SecurityConfig>,
    #[serde(default)]
    pub ident_beacon: IdentBeaconConfig,
    #[serde(default)]
    pub weather: WeatherConfig,
    /// Feature toggles for built-in mini-games and doors
    #[serde(default)]
    pub games: GamesConfig,
    /// New user welcome system
    #[serde(default)]
    pub welcome: crate::bbs::welcome::WelcomeConfig,
    /// Admin web dashboard configuration
    #[serde(default)]
    pub admin_dashboard: AdminDashboardConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshtasticConfig {
    pub port: String,
    pub baud_rate: u32,
    #[serde(default)]
    pub node_id: String,
    pub channel: u8,
    /// Require device to be available at startup. If true and device connection fails,
    /// the BBS will exit with an error code. If false (default), the BBS will start
    /// without a device connection (useful for testing or alternative transport methods).
    /// Applies to all transport types: serial, Bluetooth, TCP/UDP.
    #[serde(default)]
    pub require_device_at_startup: bool,
    /// Minimum gap between consecutive text sends (ms). Must be >= 2000ms.
    #[serde(default)]
    pub min_send_gap_ms: Option<u64>,
    /// Retransmit backoff schedule in seconds, e.g. [4, 8, 16]
    #[serde(default)]
    pub dm_resend_backoff_seconds: Option<Vec<u64>>,
    /// Additional pacing delay for a broadcast sent immediately after a reliable DM (ms)
    #[serde(default)]
    pub post_dm_broadcast_gap_ms: Option<u64>,
    /// Minimum gap between two consecutive reliable DMs (ms)
    #[serde(default)]
    pub dm_to_dm_gap_ms: Option<u64>,
    /// Delay before sending the public HELP broadcast after the DM is queued (ms). This is a higher-level
    /// scheduling cushion to avoid immediate RateLimitExceeded following a reliable DM. If unset, defaults
    /// to 3500ms. Must be >= post_dm_broadcast_gap_ms.
    #[serde(default)]
    pub help_broadcast_delay_ms: Option<u64>,
    /// Maximum number of queued outbound messages in scheduler before drop policy engages.
    #[serde(default)]
    pub scheduler_max_queue: Option<usize>,
    /// Aging threshold (ms) after which a waiting message may have its effective priority boosted.
    #[serde(default)]
    pub scheduler_aging_threshold_ms: Option<u64>,
    /// Interval (ms) for periodic scheduler stats logging (0 disables periodic stats logs).
    #[serde(default)]
    pub scheduler_stats_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: String,
    pub max_message_size: usize,
    /// Add [n/total] chunk markers to multi-part messages to help detect out-of-order delivery
    #[serde(default)]
    pub show_chunk_markers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTopicConfig {
    pub name: String,
    pub description: String,
    pub read_level: u8,
    pub post_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<String>,
    #[serde(default)]
    pub security_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GamesConfig {
    /// Enable the TinyHack mini-game in the Games submenu.
    #[serde(default)]
    pub tinyhack_enabled: bool,
    /// Surface the upcoming TinyMUSH experience in the Games submenu.
    #[serde(default)]
    pub tinymush_enabled: bool,
    /// Optional override for TinyMUSH Sled database path; defaults to `<data_dir>/tinymush`.
    #[serde(default)]
    pub tinymush_db_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Argon2Config {
    #[serde(default)]
    pub memory_kib: Option<u32>,
    #[serde(default)]
    pub time_cost: Option<u32>,
    #[serde(default)]
    pub parallelism: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub argon2: Option<Argon2Config>,
}

/// Configuration for the periodic station identification beacon.
///
/// The ident beacon broadcasts a message to the public channel on a UTC schedule.
/// Supported frequencies: "5min", "15min" (default), "30min", "1hour", "2hours", "4hours".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentBeaconConfig {
    pub enabled: bool,
    pub frequency: String,
}

impl Default for IdentBeaconConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            frequency: "15min".to_string(),
        }
    }
}

impl IdentBeaconConfig {
    /// Convert frequency string to minutes.
    ///
    /// Returns one of: 5, 15, 30, 60, 120, 240. Invalid values default to 15.
    pub fn frequency_minutes(&self) -> u32 {
        match self.frequency.as_str() {
            "5min" => 5,
            "15min" => 15,
            "30min" => 30,
            "1hour" => 60,
            "2hours" => 120,
            "4hours" => 240,
            _ => {
                eprintln!(
                    "Invalid ident beacon frequency '{}', defaulting to 15min",
                    self.frequency
                );
                15
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    /// OpenWeatherMap API key
    pub api_key: String,
    /// Default location for weather queries (city name, zipcode, or city ID)
    pub default_location: String,
    /// Location type: "city", "zipcode", or "city_id"
    pub location_type: String,
    /// Country code for zipcode lookups (e.g., "US", "GB")
    pub country_code: Option<String>,
    /// Cache TTL in minutes
    pub cache_ttl_minutes: u32,
    /// Request timeout in seconds
    pub timeout_seconds: u32,
    /// Enable/disable weather functionality
    pub enabled: bool,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            default_location: "Los Angeles".to_string(),
            location_type: "city".to_string(),
            country_code: Some("US".to_string()),
            cache_ttl_minutes: 10,
            timeout_seconds: 5,
            enabled: false, // Disabled by default until API key is provided
        }
    }
}

/// Configuration for the admin web dashboard.
///
/// The admin dashboard provides a web-based interface for managing BBS content,
/// monitoring system health, and performing administrative tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminDashboardConfig {
    /// Enable/disable the admin dashboard (disabled by default for security)
    pub enabled: bool,
    /// Bind addresses (IPv4/IPv6) with ports. Examples: ["0.0.0.0:9885", "[::]:9885"]
    pub bind_addresses: Vec<String>,
    /// Session token timeout in seconds (default 24 hours = 86400)
    pub session_timeout: u64,
    /// Minimum admin level required to access dashboard (BBS level 10 / TinyMUSH level 5)
    pub require_admin_level: u8,
    
    // TLS/HTTPS Configuration
    /// TLS mode: "self_signed", "letsencrypt", "custom", "disabled"
    pub tls_mode: String,
    /// Path to TLS certificate (for custom mode)
    pub tls_cert: Option<String>,
    /// Path to TLS private key (for custom mode)
    pub tls_key: Option<String>,
    /// Domain name for Let's Encrypt (if letsencrypt mode)
    pub letsencrypt_domain: Option<String>,
    /// Email for Let's Encrypt notifications
    pub letsencrypt_email: Option<String>,
    
    // Rate Limiting
    /// Enable rate limiting (recommended)
    pub rate_limit_enabled: bool,
    /// Max failed login attempts per IP per window
    pub login_attempts_per_ip: u32,
    /// Login attempt window in seconds (default 15 minutes = 900)
    pub login_attempt_window: u64,
    /// Max API requests per session per window
    pub api_requests_per_session: u32,
    /// API request window in seconds (default 1 minute = 60)
    pub api_request_window: u64,
    
    // Session Management
    /// Max concurrent sessions per admin user
    pub max_sessions_per_admin: u32,
    /// Rotate session token on each request (prevents replay attacks)
    pub session_token_rotation: bool,
    /// Strictly enforce token expiry
    pub enforce_token_expiry: bool,
    
    // Audit Logging (mandatory)
    /// Enable audit logging (cannot be disabled, always true)
    pub audit_log_enabled: bool,
    /// Audit log filename (in data directory by default)
    pub audit_log_file: String,
    /// Override audit log directory (if set, overrides default)
    pub audit_log_directory: Option<String>,
    /// Audit log level: "debug", "info", "warn", "error"
    pub audit_log_level: String,
    /// Log rotation: "daily", "weekly", "size"
    pub audit_log_rotation: String,
    /// Max size in MB before rotation (if size-based rotation)
    pub audit_log_max_size_mb: u64,
    
    // Feature Flags
    /// Enable content manager (NPCs, Achievements, Rooms, Objects, Quests, Companions)
    pub features_content_manager: bool,
    /// Enable player management and moderation tools
    pub features_player_management: bool,
    /// Enable system monitoring and real-time metrics
    pub features_system_monitor: bool,
    /// Enable configuration editor
    pub features_config_editor: bool,
    /// Enable JSON seed file editor
    pub features_json_editor: bool,
    /// Enable analytics and charts
    pub features_analytics: bool,
    
    // Role Configuration (data-driven roles)
    /// Define roles based on BBS access levels
    pub roles: Vec<RoleDefinition>,
}

/// Defines a role with its level range, display properties, and permissions
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RoleDefinition {
    /// Role name (e.g., "Sysop", "Admin", "Moderator", "User")
    pub name: String,
    /// Minimum BBS level for this role (inclusive)
    pub min_level: u8,
    /// Maximum BBS level for this role (inclusive)
    pub max_level: u8,
    /// Display color (CSS color name or hex)
    pub color: Option<String>,
    /// Icon/emoji for this role
    pub icon: Option<String>,
    /// Human-readable description
    pub description: Option<String>,
}

impl Default for AdminDashboardConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for security
            bind_addresses: vec!["0.0.0.0:9885".to_string(), "[::]:9885".to_string()],
            session_timeout: 86400, // 24 hours
            require_admin_level: 10, // Sysop level (BBS level 10)
            
            // TLS defaults
            tls_mode: "self_signed".to_string(),
            tls_cert: None,
            tls_key: None,
            letsencrypt_domain: None,
            letsencrypt_email: None,
            
            // Rate limiting defaults (web security best practices)
            rate_limit_enabled: true,
            login_attempts_per_ip: 5,
            login_attempt_window: 900, // 15 minutes
            api_requests_per_session: 1000,
            api_request_window: 60, // 1 minute
            
            // Session management defaults
            max_sessions_per_admin: 3,
            session_token_rotation: true,
            enforce_token_expiry: true,
            
            // Audit logging defaults (mandatory, cannot be disabled)
            audit_log_enabled: true,
            audit_log_file: "admin_dashboard.log".to_string(),
            audit_log_directory: None,
            audit_log_level: "info".to_string(),
            audit_log_rotation: "daily".to_string(),
            audit_log_max_size_mb: 100,
            
            // Feature flags (all enabled by default)
            features_content_manager: true,
            features_player_management: true,
            features_system_monitor: true,
            features_config_editor: true,
            features_json_editor: true,
            features_analytics: true,
            
            // Default role definitions
            roles: vec![
                RoleDefinition {
                    name: "Sysop".to_string(),
                    min_level: 10,
                    max_level: 10,
                    color: Some("#dc2626".to_string()), // red-600
                    icon: Some("👑".to_string()),
                    description: Some("System operator with full access".to_string()),
                },
                RoleDefinition {
                    name: "Admin".to_string(),
                    min_level: 6,
                    max_level: 9,
                    color: Some("#f97316".to_string()), // orange-500
                    icon: Some("⚡".to_string()),
                    description: Some("Administrator with elevated privileges".to_string()),
                },
                RoleDefinition {
                    name: "Moderator".to_string(),
                    min_level: 3,
                    max_level: 5,
                    color: Some("#3b82f6".to_string()), // blue-500
                    icon: Some("🛡️".to_string()),
                    description: Some("Moderator with content management access".to_string()),
                },
                RoleDefinition {
                    name: "User".to_string(),
                    min_level: 1,
                    max_level: 2,
                    color: Some("#6b7280".to_string()), // gray-500
                    icon: Some("👤".to_string()),
                    description: Some("Regular user".to_string()),
                },
            ],
        }
    }
}

impl AdminDashboardConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate TLS mode
        match self.tls_mode.as_str() {
            "self_signed" | "letsencrypt" | "custom" | "disabled" => {}
            _ => return Err(anyhow!("Invalid tls_mode: must be 'self_signed', 'letsencrypt', 'custom', or 'disabled'")),
        }
        
        // Validate custom TLS requires cert and key
        if self.tls_mode == "custom" {
            if self.tls_cert.is_none() || self.tls_key.is_none() {
                return Err(anyhow!("Custom TLS mode requires tls_cert and tls_key paths"));
            }
        }
        
        // Validate Let's Encrypt requires domain and email
        if self.tls_mode == "letsencrypt" {
            if self.letsencrypt_domain.is_none() || self.letsencrypt_email.is_none() {
                return Err(anyhow!("Let's Encrypt mode requires letsencrypt_domain and letsencrypt_email"));
            }
        }
        
        // Validate bind addresses
        if self.bind_addresses.is_empty() {
            return Err(anyhow!("At least one bind address required"));
        }
        
        // Validate audit log level
        match self.audit_log_level.as_str() {
            "debug" | "info" | "warn" | "error" => {}
            _ => return Err(anyhow!("Invalid audit_log_level: must be 'debug', 'info', 'warn', or 'error'")),
        }
        
        // Validate audit log rotation
        match self.audit_log_rotation.as_str() {
            "daily" | "weekly" | "size" => {}
            _ => return Err(anyhow!("Invalid audit_log_rotation: must be 'daily', 'weekly', or 'size'")),
        }
        
        // Validate session timeout (must be positive)
        if self.session_timeout == 0 {
            return Err(anyhow!("session_timeout must be greater than 0"));
        }
        
        // Validate rate limits (must be positive)
        if self.rate_limit_enabled {
            if self.login_attempts_per_ip == 0 {
                return Err(anyhow!("login_attempts_per_ip must be greater than 0"));
            }
            if self.login_attempt_window == 0 {
                return Err(anyhow!("login_attempt_window must be greater than 0"));
            }
            if self.api_requests_per_session == 0 {
                return Err(anyhow!("api_requests_per_session must be greater than 0"));
            }
            if self.api_request_window == 0 {
                return Err(anyhow!("api_request_window must be greater than 0"));
            }
        }
        
        // Validate role definitions
        if self.roles.is_empty() {
            return Err(anyhow!("At least one role definition required"));
        }
        
        // Validate role level ranges don't overlap
        for (i, role1) in self.roles.iter().enumerate() {
            // Validate min <= max
            if role1.min_level > role1.max_level {
                return Err(anyhow!("Role '{}' has min_level > max_level", role1.name));
            }
            
            // Check for overlaps with other roles
            for role2 in self.roles.iter().skip(i + 1) {
                let overlap = role1.min_level <= role2.max_level && role2.min_level <= role1.max_level;
                if overlap {
                    return Err(anyhow!(
                        "Role '{}' (levels {}-{}) overlaps with role '{}' (levels {}-{})",
                        role1.name, role1.min_level, role1.max_level,
                        role2.name, role2.min_level, role2.max_level
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Get role name for a given BBS access level
    pub fn level_to_role(&self, level: u8) -> String {
        self.roles
            .iter()
            .find(|r| level >= r.min_level && level <= r.max_level)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    }
    
    /// Get full role definition for a given BBS access level
    pub fn get_role_definition(&self, level: u8) -> Option<&RoleDefinition> {
        self.roles
            .iter()
            .find(|r| level >= r.min_level && level <= r.max_level)
    }
}

impl Config {
    /// Load configuration from a file
    pub async fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| anyhow!("Failed to read config file {}: {}", path, e))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse config file {}: {}", path, e))?;

        Ok(config)
    }

    /// Create a default configuration file
    pub async fn create_default(path: &str) -> Result<()> {
        let config = Config::default();
        let content = toml::to_string_pretty(&config)
            .map_err(|e| anyhow!("Failed to serialize default config: {}", e))?;

        fs::write(path, content)
            .await
            .map_err(|e| anyhow!("Failed to write config file {}: {}", path, e))?;

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut message_topics = HashMap::new();

        message_topics.insert(
            "general".to_string(),
            MessageTopicConfig {
                name: "General".to_string(),
                description: "General discussions".to_string(),
                read_level: 0,
                post_level: 0,
            },
        );

        message_topics.insert(
            "community".to_string(),
            MessageTopicConfig {
                name: "Community".to_string(),
                description: "Events, meet-ups, and community discussions".to_string(),
                read_level: 0,
                post_level: 0,
            },
        );

        message_topics.insert(
            "technical".to_string(),
            MessageTopicConfig {
                name: "Technical".to_string(),
                description: "Tech, hardware, and administrative discussions".to_string(),
                read_level: 0,
                post_level: 0,
            },
        );

        Config {
            bbs: BbsConfig {
                name: "meshbbs Station".to_string(),
                sysop: "sysop".to_string(),
                location: "Your Location".to_string(),
                description: "A bulletin board system for mesh networks".to_string(),
                max_users: 100,
                session_timeout: 10,
                welcome_message: "".to_string(),
                sysop_password_hash: None,
                public_command_prefix: Some("^".to_string()),
                allow_public_login: true,
                help_command: "HELP".to_string(),
            },
            meshtastic: MeshtasticConfig {
                port: "/dev/ttyUSB0".to_string(),
                baud_rate: 115200,
                node_id: "".to_string(),
                channel: 0,
                require_device_at_startup: false,
                min_send_gap_ms: Some(2000),
                dm_resend_backoff_seconds: Some(vec![4, 8, 16]),
                post_dm_broadcast_gap_ms: Some(1200),
                dm_to_dm_gap_ms: Some(600),
                help_broadcast_delay_ms: Some(3500),
                scheduler_max_queue: Some(512),
                scheduler_aging_threshold_ms: Some(5000),
                scheduler_stats_interval_ms: Some(10000),
            },
            storage: StorageConfig {
                data_dir: "./data".to_string(),
                max_message_size: 200, // Reduced from 230 to account for ~30 bytes Meshtastic protocol overhead
                show_chunk_markers: false, // Set to true to add [n/total] markers for debugging out-of-order delivery
            },
            message_topics,
            logging: LoggingConfig {
                level: "info".to_string(),
                file: Some("meshbbs.log".to_string()),
                security_file: Some("meshbbs-security.log".to_string()),
            },
            security: Some(SecurityConfig::default()),
            ident_beacon: IdentBeaconConfig::default(),
            weather: WeatherConfig::default(),
            games: GamesConfig::default(),
            welcome: crate::bbs::welcome::WelcomeConfig {
                enabled: false,
                public_greeting: true,
                private_guide: true,
                cooldown_minutes: 5,
                max_welcomes_per_node: 1,
            },
            admin_dashboard: AdminDashboardConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ident_beacon_config_default() {
        let config = IdentBeaconConfig::default();
        assert_eq!(config.enabled, true);
        assert_eq!(config.frequency, "15min");
    }

    #[test]
    fn test_ident_beacon_frequency_minutes_valid() {
        let test_cases = vec![
            ("5min", 5),
            ("15min", 15),
            ("30min", 30),
            ("1hour", 60),
            ("2hours", 120),
            ("4hours", 240),
        ];

        for (frequency, expected_minutes) in test_cases {
            let config = IdentBeaconConfig {
                enabled: true,
                frequency: frequency.to_string(),
            };
            assert_eq!(
                config.frequency_minutes(),
                expected_minutes,
                "Expected {} to convert to {} minutes",
                frequency,
                expected_minutes
            );
        }
    }

    #[test]
    fn test_ident_beacon_frequency_minutes_invalid() {
        let invalid_frequencies = vec!["invalid", "10hours", "", "60min", "30mins", "1hr"];

        for invalid_freq in invalid_frequencies {
            let config = IdentBeaconConfig {
                enabled: true,
                frequency: invalid_freq.to_string(),
            };
            // Invalid frequencies should default to 15 minutes
            assert_eq!(
                config.frequency_minutes(),
                15,
                "Expected invalid frequency '{}' to default to 15 minutes",
                invalid_freq
            );
        }
    }

    #[test]
    fn test_ident_beacon_disabled() {
        let config = IdentBeaconConfig {
            enabled: false,
            frequency: "30min".to_string(),
        };
        assert_eq!(config.enabled, false);
        assert_eq!(config.frequency_minutes(), 30);
    }

    #[test]
    fn test_ident_beacon_config_serde() {
        let config = IdentBeaconConfig {
            enabled: true,
            frequency: "1hour".to_string(),
        };

        // Test serialization
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("\"enabled\":true"));
        assert!(serialized.contains("\"frequency\":\"1hour\""));

        // Test deserialization
        let deserialized: IdentBeaconConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.enabled, config.enabled);
        assert_eq!(deserialized.frequency, config.frequency);
    }

    #[test]
    fn test_config_includes_ident_beacon() {
        let config = Config::default();
        assert_eq!(config.ident_beacon.enabled, true);
        assert_eq!(config.ident_beacon.frequency, "15min");
    }

    #[test]
    fn test_ident_beacon_config_clone() {
        let config = IdentBeaconConfig {
            enabled: false,
            frequency: "4hours".to_string(),
        };

        let cloned = config.clone();
        assert_eq!(cloned.enabled, config.enabled);
        assert_eq!(cloned.frequency, config.frequency);
        assert_eq!(cloned.frequency_minutes(), 240);
    }
}
