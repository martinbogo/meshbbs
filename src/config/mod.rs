//! Configuration management module

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bbs: BbsConfig,
    pub meshtastic: MeshtasticConfig,
    pub storage: StorageConfig,
    pub message_areas: HashMap<String, MessageAreaConfig>,
    pub web: WebConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbsConfig {
    pub name: String,
    pub sysop: String,
    pub location: String,
    pub zipcode: String,
    pub description: String,
    pub max_users: u32,
    pub welcome_message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sysop_password_hash: Option<String>,
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
pub struct MessageAreaConfig {
    pub name: String,
    pub description: String,
    pub read_level: u8,
    pub post_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub port: u16,
    pub admin_username: String,
    pub admin_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<String>,
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
        let mut message_areas = HashMap::new();
        
        message_areas.insert("general".to_string(), MessageAreaConfig {
            name: "General Discussion".to_string(),
            description: "General chat and discussion".to_string(),
            read_level: 0,
            post_level: 0,
        });
        
        message_areas.insert("technical".to_string(), MessageAreaConfig {
            name: "Technical Support".to_string(),
            description: "Technical help and support".to_string(),
            read_level: 0,
            post_level: 0,
        });
        
        message_areas.insert("announcements".to_string(), MessageAreaConfig {
            name: "Announcements".to_string(),
            description: "Important announcements".to_string(),
            read_level: 0,
            post_level: 10,
        });

        Config {
            bbs: BbsConfig {
                name: "MeshBBS Station".to_string(),
                sysop: "Your Name".to_string(),
                location: "Your Location".to_string(),
                zipcode: "97210".to_string(),
                description: "A bulletin board system for mesh networks".to_string(),
                max_users: 100,
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
                max_message_size: 1024,
                message_retention_days: 30,
                max_messages_per_area: 1000,
            },
            message_areas,
            web: WebConfig {
                enabled: false,
                bind_address: "127.0.0.1".to_string(),
                port: 8080,
                admin_username: "admin".to_string(),
                admin_password: "changeme".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                file: Some("meshbbs.log".to_string()),
            },
        }
    }
}