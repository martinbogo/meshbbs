//! Storage module for persisting BBS data

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use uuid::Uuid;
use password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Argon2, Params, Algorithm, Version};

/// Main storage interface
pub struct Storage {
    data_dir: String,
    argon2: Argon2<'static>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(
        rename = "access_level",
        default = "default_user_level",
        alias = "access_level"
    )]
    pub user_level: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    pub first_login: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    pub total_messages: u32,
}

fn default_user_level() -> u8 { 1 }

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
            argon2: Argon2::default(),
        })
    }

    /// Initialize storage with explicit Argon2 params
    pub async fn new_with_params(data_dir: &str, params: Option<Params>) -> Result<Self> {
        // Ensure base and subdirectories just like `new`
        fs::create_dir_all(data_dir).await?;
        let messages_dir = Path::new(data_dir).join("messages");
        let users_dir = Path::new(data_dir).join("users");
        let files_dir = Path::new(data_dir).join("files");
        fs::create_dir_all(&messages_dir).await?;
        fs::create_dir_all(&users_dir).await?;
        fs::create_dir_all(&files_dir).await?;
        let argon2 = if let Some(p) = params { Argon2::new(Algorithm::Argon2id, Version::V0x13, p) } else { Argon2::default() };
        Ok(Storage { data_dir: data_dir.to_string(), argon2 })
    }

    /// Return the base data directory path used by this storage instance
    pub fn base_dir(&self) -> &str { &self.data_dir }

    fn argon2_configured(&self) -> &Argon2<'static> { &self.argon2 }

    /// Register a new user with password; fails if user exists.
    pub async fn register_user(&mut self, username: &str, password: &str, maybe_node: Option<&str>) -> Result<()> {
        // Basic username sanity
        if username.len() < 2 { return Err(anyhow!("Username too short")); }
        if self.get_user(username).await?.is_some() { return Err(anyhow!("User already exists")); }
        let users_dir = Path::new(&self.data_dir).join("users");
        let user_file = users_dir.join(format!("{}.json", username));
        let now = Utc::now();
        let salt = password_hash::SaltString::generate(&mut rand::thread_rng());
        let hash = self.argon2_configured().hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Password hash failure: {e}"))?;
        let user = User {
            username: username.to_string(),
            node_id: maybe_node.map(|s| s.to_string()),
            user_level: 1,
            password_hash: Some(hash.to_string()),
            first_login: now,
            last_login: now,
            total_messages: 0,
        };
        let json_content = serde_json::to_string_pretty(&user)?;
        fs::write(user_file, json_content).await?;
        Ok(())
    }

    /// Verify user password; returns (user, bool match)
    pub async fn verify_user_password(&self, username: &str, password: &str) -> Result<(Option<User>, bool)> {
        if let Some(user) = self.get_user(username).await? {
            if let Some(stored) = &user.password_hash {
                let parsed = password_hash::PasswordHash::new(stored)
                    .map_err(|e| anyhow!("Corrupt password hash: {e}"))?;
                let ok = argon2::Argon2::default()
                    .verify_password(password.as_bytes(), &parsed).is_ok();
                return Ok((Some(user), ok));
            }
            return Ok((Some(user), false));
        }
        Ok((None, false))
    }

    /// Bind a user to a node id if not already bound. Returns updated user.
    pub async fn bind_user_node(&mut self, username: &str, node_id: &str) -> Result<User> {
        let users_dir = Path::new(&self.data_dir).join("users");
        let user_file = users_dir.join(format!("{}.json", username));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        if user.node_id.is_none() { user.node_id = Some(node_id.to_string()); }
        user.last_login = Utc::now();
        let json_content = serde_json::to_string_pretty(&user)?;
        fs::write(user_file, json_content).await?;
        Ok(user)
    }

    /// Update (set or change) a user's password. Always overwrites existing hash.
    pub async fn update_user_password(&mut self, username: &str, new_password: &str) -> Result<()> {
        if new_password.len() < 8 { return Err(anyhow!("Password too short (min 8)")); }
        if new_password.len() > 128 { return Err(anyhow!("Password too long")); }
        let users_dir = Path::new(&self.data_dir).join("users");
        let user_file = users_dir.join(format!("{}.json", username));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        let salt = password_hash::SaltString::generate(&mut rand::thread_rng());
        let hash = self.argon2_configured().hash_password(new_password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Password hash failure: {e}"))?;
        user.password_hash = Some(hash.to_string());
        user.last_login = Utc::now(); // treat as activity
        let json_content = serde_json::to_string_pretty(&user)?;
        fs::write(user_file, json_content).await?;
        Ok(())
    }

    /// Update a user's access level (e.g., promote/demote). Returns updated user.
    pub async fn update_user_level(&mut self, username: &str, new_level: u8) -> Result<User> {
        if new_level == 0 { return Err(anyhow!("Invalid level")); }
        let users_dir = Path::new(&self.data_dir).join("users");
        let user_file = users_dir.join(format!("{}.json", username));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        // Prevent changing sysop level (level 10) via storage API to enforce immutability
        if user.user_level == 10 && user.username == username && new_level != 10 {
            return Err(anyhow!("Cannot modify sysop level"));
        }
        user.user_level = new_level;
        user.last_login = Utc::now(); // treat promotion as activity
        let json_content = serde_json::to_string_pretty(&user)?;
        fs::write(&user_file, json_content).await?;
        Ok(user)
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
        
        let mut user = if user_file.exists() {
            let content = fs::read_to_string(&user_file).await?;
            serde_json::from_str::<User>(&content)?
        } else {
            User {
                username: username.to_string(),
                node_id: Some(node_id.to_string()),
                user_level: 1,
                password_hash: None,
                first_login: now,
                last_login: now,
                total_messages: 0,
            }
        };
        user.last_login = now;
        // Only overwrite node_id if not bound yet
        if user.node_id.is_none() { user.node_id = Some(node_id.to_string()); }
        
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