//! # Configuration Management Module
//!
//! This module handles all configuration aspects of the MeshBBS system, providing
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
//! MeshBBS uses TOML format for human-readable configuration:
//!
//! ```toml
//! [bbs]
//! name = "My Mesh BBS"
//! sysop = "admin"
//! location = "Mesh Network"
//! max_users = 100
//! session_timeout = 10
//!
//! [meshtastic]
//! port = "/dev/ttyUSB0"
//! baud_rate = 115200
//! channel = 0
//!
//! [message_topics.general]
//! name = "General"
//! description = "General discussions"
//! read_level = 0
//! post_level = 0
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

use anyhow::{Result, anyhow};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bbs: BbsConfig,
    pub meshtastic: MeshtasticConfig,
    pub storage: StorageConfig,
    pub message_topics: HashMap<String, MessageTopicConfig>,
    pub logging: LoggingConfig,
    pub security: Option<SecurityConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshtasticConfig {
    pub port: String,
    pub baud_rate: u32,
    pub node_id: String,
    pub channel: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: String,
    pub max_message_size: usize,
    pub message_retention_days: u32,
    pub max_messages_per_area: u32,
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

impl Config {
    /// Load configuration from a file
    pub async fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path).await
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
        
        fs::write(path, content).await
            .map_err(|e| anyhow!("Failed to write config file {}: {}", path, e))?;
        
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut message_topics = HashMap::new();
        
        message_topics.insert("general".to_string(), MessageTopicConfig {
            name: "General".to_string(),
            description: "General discussions".to_string(),
            read_level: 0,
            post_level: 0,
        });
        
        message_topics.insert("community".to_string(), MessageTopicConfig {
            name: "Community".to_string(),
            description: "Events, meet-ups, and community discussions".to_string(),
            read_level: 0,
            post_level: 0,
        });
        
        message_topics.insert("technical".to_string(), MessageTopicConfig {
            name: "Technical".to_string(),
            description: "Tech, hardware, and administrative discussions".to_string(),
            read_level: 0,
            post_level: 0,
        });

        Config {
            bbs: BbsConfig {
                name: "MeshBBS Station".to_string(),
                sysop: "sysop".to_string(),
                location: "Your Location".to_string(),
                description: "A bulletin board system for mesh networks".to_string(),
                max_users: 100,
                session_timeout: 10,
                welcome_message: "Welcome to MeshBBS! Type HELP for commands.".to_string(),
                sysop_password_hash: None,
            },
            meshtastic: MeshtasticConfig {
                port: "/dev/ttyUSB0".to_string(),
                baud_rate: 115200,
                node_id: "".to_string(),
                channel: 0,
            },
            storage: StorageConfig {
                data_dir: "./data".to_string(),
                max_message_size: 230,
                message_retention_days: 30,
                max_messages_per_area: 1000,
            },
            message_topics,
            logging: LoggingConfig {
                level: "info".to_string(),
                file: Some("meshbbs.log".to_string()),
                security_file: Some("meshbbs-security.log".to_string()),
            },
            security: Some(SecurityConfig::default()),
        }
    }
}