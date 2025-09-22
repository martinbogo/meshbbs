//! Storage module for persisting BBS data

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

/// Main storage interface
pub struct Storage {
    data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub area: String,
    pub author: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub replies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub node_id: String,
    pub access_level: u8,
    pub first_login: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    pub total_messages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbsStatistics {
    pub total_messages: u32,
    pub total_users: u32,
    pub uptime_start: DateTime<Utc>,
}

impl Storage {
    /// Initialize storage with the given data directory
    pub async fn new(data_dir: &str) -> Result<Self> {
        // Create data directory if it doesn't exist
        fs::create_dir_all(data_dir).await
            .map_err(|e| anyhow!("Failed to create data directory {}: {}", data_dir, e))?;
        
        // Create subdirectories
        let messages_dir = Path::new(data_dir).join("messages");
        let users_dir = Path::new(data_dir).join("users");
        let files_dir = Path::new(data_dir).join("files");
        
        fs::create_dir_all(&messages_dir).await?;
        fs::create_dir_all(&users_dir).await?;
        fs::create_dir_all(&files_dir).await?;
        
        Ok(Storage {
            data_dir: data_dir.to_string(),
        })
    }

    /// Store a new message
    pub async fn store_message(&mut self, area: &str, author: &str, content: &str) -> Result<String> {
        let message = Message {
            id: Uuid::new_v4().to_string(),
            area: area.to_string(),
            author: author.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            replies: Vec::new(),
        };
        
        let area_dir = Path::new(&self.data_dir).join("messages").join(area);
        fs::create_dir_all(&area_dir).await?;
        
        let message_file = area_dir.join(format!("{}.json", message.id));
        let json_content = serde_json::to_string_pretty(&message)?;
        
        fs::write(message_file, json_content).await?;
        
        Ok(message.id)
    }

    /// Get recent messages from an area
    pub async fn get_messages(&self, area: &str, limit: usize) -> Result<Vec<Message>> {
        let area_dir = Path::new(&self.data_dir).join("messages").join(area);
        
        if !area_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut messages = Vec::new();
        let mut entries = fs::read_dir(&area_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(entry.path()).await?;
                if let Ok(message) = serde_json::from_str::<Message>(&content) {
                    messages.push(message);
                }
            }
        }
        
        // Sort by timestamp, newest first
        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        messages.truncate(limit);
        
        Ok(messages)
    }

    /// List available message areas
    pub async fn list_message_areas(&self) -> Result<Vec<String>> {
        let messages_dir = Path::new(&self.data_dir).join("messages");
        
        if !messages_dir.exists() {
            return Ok(vec!["general".to_string()]);
        }
        
        let mut areas = Vec::new();
        let mut entries = fs::read_dir(&messages_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(area_name) = entry.file_name().to_str() {
                    areas.push(area_name.to_string());
                }
            }
        }
        
        if areas.is_empty() {
            areas.push("general".to_string());
        }
        
        areas.sort();
        Ok(areas)
    }

    /// Create or update a user
    pub async fn create_or_update_user(&mut self, username: &str, node_id: &str) -> Result<()> {
        let users_dir = Path::new(&self.data_dir).join("users");
        let user_file = users_dir.join(format!("{}.json", username));
        
        let now = Utc::now();
        
        let user = if user_file.exists() {
            // Update existing user
            let content = fs::read_to_string(&user_file).await?;
            let mut user: User = serde_json::from_str(&content)?;
            user.last_login = now;
            user.node_id = node_id.to_string(); // Update node ID in case it changed
            user
        } else {
            // Create new user
            User {
                username: username.to_string(),
                node_id: node_id.to_string(),
                access_level: 1,
                first_login: now,
                last_login: now,
                total_messages: 0,
            }
        };
        
        let json_content = serde_json::to_string_pretty(&user)?;
        fs::write(user_file, json_content).await?;
        
        Ok(())
    }

    /// Get user information
    pub async fn get_user(&self, username: &str) -> Result<Option<User>> {
        let user_file = Path::new(&self.data_dir).join("users").join(format!("{}.json", username));
        
        if !user_file.exists() {
            return Ok(None);
        }
        
        let content = fs::read_to_string(user_file).await?;
        let user: User = serde_json::from_str(&content)?;
        
        Ok(Some(user))
    }

    /// Get BBS statistics
    pub async fn get_statistics(&self) -> Result<BbsStatistics> {
        let mut total_messages = 0;
        let mut total_users = 0;
        
        // Count messages
        let messages_dir = Path::new(&self.data_dir).join("messages");
        if messages_dir.exists() {
            let mut area_entries = fs::read_dir(&messages_dir).await?;
            while let Some(area_entry) = area_entries.next_entry().await? {
                if area_entry.file_type().await?.is_dir() {
                    let mut message_entries = fs::read_dir(area_entry.path()).await?;
                    while let Some(_) = message_entries.next_entry().await? {
                        total_messages += 1;
                    }
                }
            }
        }
        
        // Count users
        let users_dir = Path::new(&self.data_dir).join("users");
        if users_dir.exists() {
            let mut user_entries = fs::read_dir(&users_dir).await?;
            while let Some(_) = user_entries.next_entry().await? {
                total_users += 1;
            }
        }
        
        Ok(BbsStatistics {
            total_messages,
            total_users,
            uptime_start: Utc::now(), // This would be stored persistently in a real implementation
        })
    }

    /// Clean up old messages based on retention policy
    pub async fn cleanup_old_messages(&mut self, retention_days: u32) -> Result<u32> {
        let cutoff_date = Utc::now() - chrono::Duration::days(retention_days as i64);
        let mut deleted_count = 0;
        
        let messages_dir = Path::new(&self.data_dir).join("messages");
        if !messages_dir.exists() {
            return Ok(0);
        }
        
        let mut area_entries = fs::read_dir(&messages_dir).await?;
        while let Some(area_entry) = area_entries.next_entry().await? {
            if area_entry.file_type().await?.is_dir() {
                let mut message_entries = fs::read_dir(area_entry.path()).await?;
                while let Some(message_entry) = message_entries.next_entry().await? {
                    if message_entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                        let content = fs::read_to_string(message_entry.path()).await?;
                        if let Ok(message) = serde_json::from_str::<Message>(&content) {
                            if message.timestamp < cutoff_date {
                                fs::remove_file(message_entry.path()).await?;
                                deleted_count += 1;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(deleted_count)
    }
}