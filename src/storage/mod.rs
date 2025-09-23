//! Storage module for persisting BBS data

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::collections::{HashSet, HashMap};
use std::io::ErrorKind;
use tokio::fs;
use uuid::Uuid;
use crate::bbs::roles;
use crate::validation::{validate_user_name, safe_filename, validate_area_name, sanitize_message_content, secure_message_path, secure_area_path, secure_json_parse, validate_file_size};
use password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Argon2, Params, Algorithm, Version};
use log::warn;
use fs2::FileExt;

/// Main storage interface
pub struct Storage {
    data_dir: String,
    argon2: Argon2<'static>,
    locked_areas: HashSet<String>,
    #[allow(dead_code)]
    area_levels: std::collections::HashMap<String, (u8,u8)>, // area -> (read_level, post_level)
    max_message_bytes: usize,
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
pub struct DeletionAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub area: String,
    pub id: String,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub target: Option<String>, // user being affected, if applicable
    pub actor: String,         // admin performing the action
    pub details: Option<String>, // additional context
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
    #[serde(default)]
    pub welcome_shown_on_registration: bool,
    #[serde(default)]
    pub welcome_shown_on_first_login: bool,
}

fn default_user_level() -> u8 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbsStatistics {
    pub total_messages: u32,
    pub total_users: u32,
    pub uptime_start: DateTime<Utc>,
    pub moderator_count: u32,
    pub recent_registrations: u32, // Users registered in last 7 days
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
        
        let locked = Self::load_locked_areas(data_dir).await?;
        Ok(Storage {
            data_dir: data_dir.to_string(),
            argon2: Argon2::default(),
            locked_areas: locked,
            area_levels: HashMap::new(),
            max_message_bytes: 230,
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
        let locked = Self::load_locked_areas(data_dir).await?;
        Ok(Storage { data_dir: data_dir.to_string(), argon2, locked_areas: locked, area_levels: HashMap::new(), max_message_bytes: 230 })
    }

    pub fn set_area_levels(&mut self, map: std::collections::HashMap<String,(u8,u8)>) { self.area_levels = map; }
    pub fn get_area_levels(&self, area: &str) -> Option<(u8,u8)> { self.area_levels.get(area).copied() }
    pub fn set_max_message_bytes(&mut self, max: usize) { self.max_message_bytes = max.min(230); }

    async fn load_locked_areas(data_dir: &str) -> Result<HashSet<String>> {
        let path = Path::new(data_dir).join("locked_areas.json");
        match fs::read_to_string(&path).await {
            Ok(data) => {
                let v: Vec<String> = serde_json::from_str(&data).unwrap_or_default();
                Ok(v.into_iter().collect())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(HashSet::new()),
            Err(e) => Err(anyhow!("Failed reading locked areas: {e}")),
        }
    }

    /// Helper function to write content to a file with exclusive locking
    async fn write_file_locked(path: &Path, content: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;
        
        // Use synchronous I/O for file locking since fs2 doesn't support async
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
            
        // Acquire exclusive lock
        file.lock_exclusive()?;
        
        // Write content
        file.write_all(content.as_bytes())?;
        file.flush()?;
        
        // Lock is automatically released when file is dropped
        Ok(())
    }

    /// Helper function to append content to a file with exclusive locking
    async fn append_file_locked(path: &Path, content: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
            
        // Acquire exclusive lock
        file.lock_exclusive()?;
        
        // Write content
        file.write_all(content.as_bytes())?;
        file.flush()?;
        
        // Lock is automatically released when file is dropped
        Ok(())
    }

    async fn persist_locked_areas(&self) -> Result<()> {
        let path = Path::new(&self.data_dir).join("locked_areas.json");
        let mut list: Vec<String> = self.locked_areas.iter().cloned().collect();
        list.sort();
        let data = serde_json::to_string_pretty(&list)?;
        Self::write_file_locked(&path, &data).await?;
        Ok(())
    }

    /// Return the base data directory path used by this storage instance
    pub fn base_dir(&self) -> &str { &self.data_dir }

    fn argon2_configured(&self) -> &Argon2<'static> { &self.argon2 }

    /// Register a new user with password; fails if user exists.
    pub async fn register_user(&mut self, username: &str, password: &str, maybe_node: Option<&str>) -> Result<()> {
        // Validate username with security rules
        let validated_username = validate_user_name(username)
            .map_err(|e| anyhow!("Invalid username: {}", e))?;
        
        // Password validation
        if password.len() < 8 { return Err(anyhow!("Password too short (minimum 8 characters)")); }
        
        // Check if user already exists
        if self.get_user(&validated_username).await?.is_some() { 
            return Err(anyhow!("Username '{}' is already taken", validated_username)); 
        }
        
        let users_dir = Path::new(&self.data_dir).join("users");
        let safe_name = safe_filename(&validated_username);
        let user_file = users_dir.join(format!("{}.json", safe_name));
        let now = Utc::now();
        let salt = password_hash::SaltString::generate(&mut rand::thread_rng());
        let hash = self.argon2_configured().hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Password hash failure: {e}"))?;
        let user = User {
            username: validated_username,
            node_id: maybe_node.map(|s| s.to_string()),
            user_level: 1,
            password_hash: Some(hash.to_string()),
            first_login: now,
            last_login: now,
            total_messages: 0,
            welcome_shown_on_registration: false,
            welcome_shown_on_first_login: false,
        };
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        Ok(())
    }

    /// Verify user password; returns (user, bool match)
    pub async fn verify_user_password(&self, username: &str, password: &str) -> Result<(Option<User>, bool)> {
        if let Some(user) = self.get_user(username).await? {
            if let Some(stored) = &user.password_hash {
                let parsed = password_hash::PasswordHash::new(stored)
                    .map_err(|e| anyhow!("Corrupt password hash: {e}"))?;
                let ok = self.argon2_configured()
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
        let safe_filename = safe_filename(username);
        let user_file = users_dir.join(format!("{}.json", safe_filename));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        if user.node_id.is_none() { user.node_id = Some(node_id.to_string()); }
        user.last_login = Utc::now();
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        Ok(user)
    }

    /// Set password for an existing (possibly passwordless) user. Overwrites existing hash.
    pub async fn set_user_password(&mut self, username: &str, password: &str) -> Result<User> {
        if password.len() < 8 { return Err(anyhow!("Password too short (minimum 8 characters)")); }
        let users_dir = Path::new(&self.data_dir).join("users");
        let safe_filename = safe_filename(username);
        let user_file = users_dir.join(format!("{}.json", safe_filename));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        let salt = password_hash::SaltString::generate(&mut rand::thread_rng());
        let hash = self.argon2_configured().hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Password hash failure: {e}"))?;
        user.password_hash = Some(hash.to_string());
        user.last_login = Utc::now();
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        Ok(user)
    }

    /// Update (set or change) a user's password. Always overwrites existing hash.
    pub async fn update_user_password(&mut self, username: &str, new_password: &str) -> Result<()> {
        if new_password.len() < 8 { return Err(anyhow!("Password too short (min 8)")); }
        if new_password.len() > 128 { return Err(anyhow!("Password too long")); }
        let users_dir = Path::new(&self.data_dir).join("users");
        let safe_filename = safe_filename(username);
        let user_file = users_dir.join(format!("{}.json", safe_filename));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        let salt = password_hash::SaltString::generate(&mut rand::thread_rng());
        let hash = self.argon2_configured().hash_password(new_password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Password hash failure: {e}"))?;
        user.password_hash = Some(hash.to_string());
        user.last_login = Utc::now(); // treat as activity
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        Ok(())
    }

    /// Update a user's access level (e.g., promote/demote). Returns updated user.
    pub async fn update_user_level(&mut self, username: &str, new_level: u8, actor: &str) -> Result<User> {
        if new_level == 0 { return Err(anyhow!("Invalid level")); }
        let users_dir = Path::new(&self.data_dir).join("users");
        let safe_filename = safe_filename(username);
        let user_file = users_dir.join(format!("{}.json", safe_filename));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = serde_json::from_str(&content)?;
        // Prevent changing sysop level (level 10) via storage API to enforce immutability
        if user.user_level == 10 && user.username == username && new_level != 10 {
            return Err(anyhow!("Cannot modify sysop level"));
        }
        let old_level = user.user_level;
        user.user_level = new_level;
        user.last_login = Utc::now(); // treat promotion as activity
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        
        // Log the administrative action
        let action = if new_level > old_level { "PROMOTE" } else { "DEMOTE" };
        let details = format!("Level changed from {} to {}", old_level, new_level);
        self.log_admin_action(action, Some(username), actor, Some(&details)).await?;
        
        Ok(user)
    }

    /// Store a new message
    pub async fn store_message(&mut self, area: &str, author: &str, content: &str) -> Result<String> {
        // Validate and sanitize inputs
        let validated_area = validate_area_name(area)
            .map_err(|e| anyhow!("Invalid area name: {}", e))?;
        
        let sanitized_content = sanitize_message_content(content, self.max_message_bytes)
            .map_err(|e| anyhow!("Invalid message content: {}", e))?;
        
        if self.locked_areas.contains(&validated_area) { 
            return Err(anyhow!("Area locked")); 
        }
        
        if let Some((_, post_level)) = self.get_area_levels(&validated_area) {
            // Determine author level (missing user defaults to 0/1?) default user level is 1 but config might use 0; use 0 baseline
            let author_level = if let Some(user) = self.get_user(author).await? { user.user_level } else { 0 };
            if author_level < post_level { return Err(anyhow!("Insufficient level to post")); }
        }

        let message = Message {
            id: Uuid::new_v4().to_string(),
            area: validated_area.clone(),
            author: author.to_string(),
            content: sanitized_content,
            timestamp: Utc::now(),
            replies: Vec::new(),
        };
        
        // Use secure path construction
        let area_dir = secure_area_path(&self.data_dir, &validated_area)
            .map_err(|e| anyhow!("Path validation failed: {}", e))?;
        fs::create_dir_all(&area_dir).await?;
        
        let message_file = secure_message_path(&self.data_dir, &validated_area, &message.id)
            .map_err(|e| anyhow!("Message path validation failed: {}", e))?;
        
        let json_content = serde_json::to_string_pretty(&message)?;
        
        Self::write_file_locked(&message_file, &json_content).await?;
        
        Ok(message.id)
    }

    /// Count messages whose timestamp is strictly greater than the supplied instant.
    /// This performs a linear scan of all message JSON files. For typical small BBS
    /// deployments this is acceptable; if performance becomes an issue we can
    /// introduce a lightweight global index or per-area cached high-water marks.
    pub async fn count_messages_since(&self, since: DateTime<Utc>) -> Result<u32> {
        let mut count: u32 = 0;
        let messages_dir = Path::new(&self.data_dir).join("messages");
        if !messages_dir.exists() { return Ok(0); }
        let mut area_entries = fs::read_dir(&messages_dir).await?;
        while let Some(area_entry) = area_entries.next_entry().await? {
            if area_entry.file_type().await?.is_dir() {
                // Validate area name to prevent processing invalid directories
                if let Some(area_name) = area_entry.file_name().to_str() {
                    if validate_area_name(area_name).is_err() {
                        warn!("Skipping invalid area directory: {}", area_name);
                        continue;
                    }
                } else {
                    warn!("Skipping directory with invalid name: {:?}", area_entry.path());
                    continue;
                }
                
                let mut message_entries = fs::read_dir(area_entry.path()).await?;
                while let Some(message_entry) = message_entries.next_entry().await? {
                    if message_entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                        // Check file size before reading to prevent DoS
                        if let Ok(metadata) = message_entry.metadata().await {
                            if metadata.len() > 1_000_000 { // 1MB limit per message file
                                warn!("Skipping oversized message file: {:?}", message_entry.path());
                                continue;
                            }
                        }
                        
                        // Read then parse timestamp only; if parsing fails, skip file.
                        if let Ok(content) = fs::read_to_string(message_entry.path()).await {
                            if let Ok(msg) = serde_json::from_str::<Message>(&content) {
                                if msg.timestamp > since { count += 1; }
                            }
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Record a successful user login (updating last_login) and return updated user.
    pub async fn record_user_login(&self, username: &str) -> Result<User> {
        let users_dir = Path::new(&self.data_dir).join("users");
        let safe_filename = safe_filename(username);
        let user_file = users_dir.join(format!("{}.json", safe_filename));
        if !user_file.exists() { return Err(anyhow!("User not found")); }
        
        // Check file size before reading
        let metadata = fs::metadata(&user_file).await?;
        validate_file_size(metadata.len(), 100_000) // 100KB limit for user files
            .map_err(|e| anyhow!("User file too large: {:?}", e))?;
        
        let content = fs::read_to_string(&user_file).await?;
        let mut user: User = secure_json_parse(&content, 100_000)
            .map_err(|e| anyhow!("Failed to parse user file: {:?}", e))?;
        
        user.last_login = Utc::now();
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        Ok(user)
    }

    /// Delete a message by area and id
    pub async fn delete_message(&mut self, area: &str, id: &str) -> Result<bool> {
        // Validate inputs to prevent path traversal
        let message_file = secure_message_path(&self.data_dir, area, id)
            .map_err(|e| anyhow!("Invalid path parameters: {}", e))?;
        
        if message_file.exists() { 
            fs::remove_file(message_file).await?; 
            return Ok(true); 
        }
        Ok(false)
    }

    /// Append a deletion audit entry (caller ensures deletion occurred)
    pub async fn append_deletion_audit(&self, area: &str, id: &str, actor: &str) -> Result<()> {
        let path = Path::new(&self.data_dir).join("deletion_audit.log");
        let entry = DeletionAuditEntry { timestamp: Utc::now(), area: area.to_string(), id: id.to_string(), actor: actor.to_string() };
        let line = serde_json::to_string(&entry)? + "\n";
        Self::append_file_locked(&path, &line).await?;
        Ok(())
    }

    /// Fetch a page of deletion audit entries (newest first). page is 1-based.
    pub async fn get_deletion_audit_page(&self, page: usize, page_size: usize) -> Result<Vec<DeletionAuditEntry>> {
        if page == 0 { return Ok(vec![]); }
        let path = Path::new(&self.data_dir).join("deletion_audit.log");
        if !path.exists() { return Ok(vec![]); }
        let content = fs::read_to_string(path).await?;
        let mut entries: Vec<DeletionAuditEntry> = content.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        // Newest first: original order is append older->newer; reverse
        entries.reverse();
        let start = (page - 1) * page_size;
        if start >= entries.len() { return Ok(vec![]); }
        let end = (start + page_size).min(entries.len());
        Ok(entries[start..end].to_vec())
    }

    /// Log an administrative action to the audit trail
    pub async fn log_admin_action(&self, action: &str, target: Option<&str>, actor: &str, details: Option<&str>) -> Result<()> {
        let path = Path::new(&self.data_dir).join("admin_audit.log");
        let entry = AdminAuditEntry {
            timestamp: Utc::now(),
            action: action.to_string(),
            target: target.map(|t| t.to_string()),
            actor: actor.to_string(),
            details: details.map(|d| d.to_string()),
        };
        let line = serde_json::to_string(&entry)? + "\n";
        Self::append_file_locked(&path, &line).await?;
        Ok(())
    }

    /// Fetch a page of admin audit entries (newest first). page is 1-based.
    pub async fn get_admin_audit_page(&self, page: usize, page_size: usize) -> Result<Vec<AdminAuditEntry>> {
        let path = Path::new(&self.data_dir).join("admin_audit.log");
        if !path.exists() { return Ok(vec![]); }
        let content = fs::read_to_string(&path).await?;
        let mut entries = Vec::new();
        for line in content.lines() {
            if !line.trim().is_empty() {
                match serde_json::from_str::<AdminAuditEntry>(line) {
                    Ok(entry) => entries.push(entry),
                    Err(_) => continue, // Skip malformed lines
                }
            }
        }
        
        // Sort by timestamp descending (newest first)
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Paginate
        let start = (page.saturating_sub(1)) * page_size;
        Ok(entries.into_iter().skip(start).take(page_size).collect())
    }

    /// Lock a message area (prevent posting)
    pub fn lock_area(&mut self, area: &str) { self.locked_areas.insert(area.to_string()); }
    /// Unlock a message area
    pub fn unlock_area(&mut self, area: &str) { self.locked_areas.remove(area); }
    /// Check if area locked
    pub fn is_area_locked(&self, area: &str) -> bool { self.locked_areas.contains(area) }

        /// Lock area and persist
        pub async fn lock_area_persist(&mut self, area: &str) -> Result<()> {
            self.lock_area(area);
            self.persist_locked_areas().await
        }
        /// Unlock area and persist
        pub async fn unlock_area_persist(&mut self, area: &str) -> Result<()> {
            self.unlock_area(area);
            self.persist_locked_areas().await
        }

    /// Get recent messages from an area
    pub async fn get_messages(&self, area: &str, limit: usize) -> Result<Vec<Message>> {
        // Validate area name to prevent path traversal
        let area_dir = secure_area_path(&self.data_dir, area)
            .map_err(|e| anyhow!("Invalid area name: {}", e))?;
        
        if !area_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut messages = Vec::new();
        let mut entries = fs::read_dir(&area_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                // Check file size before reading to prevent DoS
                let metadata = entry.metadata().await?;
                if metadata.len() > 1_000_000 { // 1MB limit per message file
                    warn!("Skipping oversized message file: {:?}", entry.path());
                    continue;
                }
                
                match fs::read_to_string(entry.path()).await {
                    Ok(content) => {
                        match serde_json::from_str::<Message>(&content) {
                            Ok(message) => {
                                // Validate the message ID matches the filename
                                if let Some(filename) = entry.path().file_stem().and_then(|s| s.to_str()) {
                                    if message.id == filename {
                                        messages.push(message);
                                    } else {
                                        warn!("Message ID mismatch in file: {:?}", entry.path());
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse message file {:?}: {}", entry.path(), e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read message file {:?}: {}", entry.path(), e);
                    }
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
                    // Validate area name to only include legitimate areas
                    if validate_area_name(area_name).is_ok() {
                        areas.push(area_name.to_string());
                    } else {
                        warn!("Skipping invalid area directory: {}", area_name);
                    }
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
        let safe_filename = safe_filename(username);
        let user_file = users_dir.join(format!("{}.json", safe_filename));
        
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
                welcome_shown_on_registration: false,
                welcome_shown_on_first_login: false,
            }
        };
        user.last_login = now;
        // Only overwrite node_id if not bound yet
        if user.node_id.is_none() { user.node_id = Some(node_id.to_string()); }
        
        let json_content = serde_json::to_string_pretty(&user)?;
        Self::write_file_locked(&user_file, &json_content).await?;
        
        Ok(())
    }

    /// Get user information
    pub async fn get_user(&self, username: &str) -> Result<Option<User>> {
        let safe_filename = safe_filename(username);
        let user_file = Path::new(&self.data_dir).join("users").join(format!("{}.json", safe_filename));
        
        if !user_file.exists() {
            return Ok(None);
        }
        
        // Check file size before reading
        let metadata = fs::metadata(&user_file).await?;
        validate_file_size(metadata.len(), 100_000) // 100KB limit for user files
            .map_err(|e| anyhow!("User file too large: {:?}", e))?;
        
        let content = fs::read_to_string(user_file).await?;
        let user: User = secure_json_parse(&content, 100_000)
            .map_err(|e| anyhow!("Failed to parse user file: {:?}", e))?;
        
        Ok(Some(user))
    }

    /// Get BBS statistics
    pub async fn get_statistics(&self) -> Result<BbsStatistics> {
        let mut total_messages = 0;
        let mut total_users = 0;
        let mut moderator_count = 0;
        let mut recent_registrations = 0;
        
        let seven_days_ago = Utc::now() - chrono::Duration::days(7);
        
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
        
        // Count users and analyze roles/registrations
        let users_dir = Path::new(&self.data_dir).join("users");
        if users_dir.exists() {
            let mut user_entries = fs::read_dir(&users_dir).await?;
            while let Some(entry) = user_entries.next_entry().await? {
                if entry.file_type().await?.is_file() && entry.path().extension().map_or(false, |ext| ext == "json") {
                    total_users += 1;
                    
                    // Parse user file to get details
                    if let Ok(content) = fs::read_to_string(entry.path()).await {
                        if let Ok(user) = serde_json::from_str::<User>(&content) {
                            if user.user_level >= roles::LEVEL_MODERATOR {
                                moderator_count += 1;
                            }
                            if user.first_login >= seven_days_ago {
                                recent_registrations += 1;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(BbsStatistics {
            total_messages,
            total_users,
            uptime_start: Utc::now(), // This would be stored persistently in a real implementation
            moderator_count,
            recent_registrations,
        })
    }

    /// List all users with their basic information
    pub async fn list_all_users(&self) -> Result<Vec<User>> {
        let users_dir = Path::new(&self.data_dir).join("users");
        let mut users = Vec::new();
        
        if users_dir.exists() {
            let mut user_entries = fs::read_dir(&users_dir).await?;
            while let Some(entry) = user_entries.next_entry().await? {
                if entry.file_type().await?.is_file() && entry.path().extension().map_or(false, |ext| ext == "json") {
                    let content = fs::read_to_string(entry.path()).await?;
                    match serde_json::from_str::<User>(&content) {
                        Ok(user) => users.push(user),
                        Err(e) => log::warn!("Failed to parse user file {:?}: {}", entry.path(), e),
                    }
                }
            }
        }
        
        // Sort by username for consistent output
        users.sort_by(|a, b| a.username.cmp(&b.username));
        Ok(users)
    }

    /// Get enhanced user information including post count in specific areas
    pub async fn get_user_details(&self, username: &str) -> Result<Option<User>> {
        self.get_user(username).await
    }

    /// Count total posts by a specific user across all areas
    pub async fn count_user_posts(&self, username: &str) -> Result<u32> {
        let messages_dir = Path::new(&self.data_dir).join("messages");
        let mut post_count = 0;
        
        if messages_dir.exists() {
            let mut area_entries = fs::read_dir(&messages_dir).await?;
            while let Some(area_entry) = area_entries.next_entry().await? {
                if area_entry.file_type().await?.is_dir() {
                    let mut message_entries = fs::read_dir(area_entry.path()).await?;
                    while let Some(message_entry) = message_entries.next_entry().await? {
                        if message_entry.file_type().await?.is_file() && message_entry.path().extension().map_or(false, |ext| ext == "json") {
                            // Check file size before reading
                            if let Ok(metadata) = message_entry.metadata().await {
                                if metadata.len() > 1_000_000 { // 1MB limit per message file
                                    warn!("Skipping oversized message file: {:?}", message_entry.path());
                                    continue;
                                }
                            }
                            
                            let content = fs::read_to_string(message_entry.path()).await?;
                            if let Ok(msg) = secure_json_parse::<Message>(&content, 1_000_000) {
                                if msg.author == username {
                                    post_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(post_count)
    }

    /// Mark that the welcome message has been shown to a user
    pub async fn mark_welcome_shown(&self, username: &str, registration_welcome: bool, first_login_welcome: bool) -> Result<()> {
        let user_file = std::path::Path::new(&self.data_dir).join("users").join(format!("{}.json", username));
        if let Some(mut user) = self.get_user(username).await? {
            if registration_welcome {
                user.welcome_shown_on_registration = true;
            }
            if first_login_welcome {
                user.welcome_shown_on_first_login = true;
            }
            let json_content = serde_json::to_string_pretty(&user)?;
            Self::write_file_locked(&user_file, &json_content).await?;
        }
        Ok(())
    }

}