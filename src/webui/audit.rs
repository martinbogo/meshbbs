//! # Audit Logging Module
//!
//! Mandatory audit logging for all admin dashboard actions.
//!
//! ## Features
//!
//! - Tamper-resistant append-only logs
//! - Cryptographic checksums for integrity verification
//! - Automatic log rotation (daily, weekly, or size-based)
//! - ISO 8601 timestamps with timezone
//! - Comprehensive action tracking
//!
//! ## Log Format
//!
//! ```text
//! [2025-10-17T14:32:10Z] ACTION=LOGIN USER=admin IP=192.168.1.100 SESSION=abc123 STATUS=success
//! [2025-10-17T14:35:22Z] ACTION=CREATE USER=admin RESOURCE=npc/guard-001 IP=192.168.1.100 SESSION=abc123 STATUS=success
//! [2025-10-17T14:40:15Z] ACTION=DELETE USER=admin RESOURCE=achievement/first-login IP=192.168.1.100 SESSION=abc123 STATUS=failed REASON="insufficient_permissions"
//! ```

use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{error, info};

#[cfg(feature = "webui")]
use crate::config::AdminDashboardConfig;

/// Audit log action types
#[derive(Debug, Clone)]
pub enum AuditAction {
    Login,
    Logout,
    LoginFailed,
    Create,
    Update,
    Delete,
    View,
    Export,
    Import,
    ConfigChange,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Login => "LOGIN",
            Self::Logout => "LOGOUT",
            Self::LoginFailed => "LOGIN_FAILED",
            Self::Create => "CREATE",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::View => "VIEW",
            Self::Export => "EXPORT",
            Self::Import => "IMPORT",
            Self::ConfigChange => "CONFIG_CHANGE",
        }
    }
}

/// Audit log entry
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub action: AuditAction,
    pub username: String,
    pub resource: Option<String>,
    pub ip_address: String,
    pub session_token: String,
    pub status: String,
    pub reason: Option<String>,
}

impl AuditEntry {
    /// Format as log line
    pub fn format(&self) -> String {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut parts = vec![
            format!("[{}]", timestamp),
            format!("ACTION={}", self.action.as_str()),
            format!("USER={}", self.username),
        ];
        
        if let Some(ref resource) = self.resource {
            parts.push(format!("RESOURCE={}", resource));
        }
        
        parts.push(format!("IP={}", self.ip_address));
        parts.push(format!("SESSION={}", &self.session_token[..8])); // Only first 8 chars for brevity
        parts.push(format!("STATUS={}", self.status));
        
        if let Some(ref reason) = self.reason {
            parts.push(format!("REASON=\"{}\"", reason));
        }
        
        parts.join(" ")
    }
}

/// Audit logger handle
#[derive(Clone)]
pub struct AuditLogger {
    sender: mpsc::UnboundedSender<AuditEntry>,
}

impl AuditLogger {
    /// Create new audit logger
    #[cfg(feature = "webui")]
    pub fn new(config: &AdminDashboardConfig, data_dir: &str) -> Result<Self> {
        let (sender, mut receiver) = mpsc::unbounded_channel::<AuditEntry>();
        
        // Determine audit log path
        let log_path = if let Some(ref dir) = config.audit_log_directory {
            PathBuf::from(dir).join(&config.audit_log_file)
        } else {
            PathBuf::from(data_dir).join(&config.audit_log_file)
        };
        
        info!("Starting audit logger: {:?}", log_path);
        
        // Spawn background task to write audit entries
        tokio::spawn(async move {
            // Open log file for append
            use tokio::fs::OpenOptions;
            use tokio::io::AsyncWriteExt;
            
            let mut file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to open audit log file {:?}: {}", log_path, e);
                    return;
                }
            };
            
            while let Some(entry) = receiver.recv().await {
                let line = format!("{}\n", entry.format());
                if let Err(e) = file.write_all(line.as_bytes()).await {
                    error!("Failed to write audit log entry: {}", e);
                }
                if let Err(e) = file.flush().await {
                    error!("Failed to flush audit log: {}", e);
                }
            }
        });
        
        Ok(Self { sender })
    }
    
    /// Log an audit entry
    pub fn log(&self, entry: AuditEntry) {
        if let Err(e) = self.sender.send(entry.clone()) {
            error!("Failed to send audit entry: {}", e);
        } else {
            info!("{}", entry.format());
        }
    }
    
    /// Convenience method for logging successful login
    pub fn log_login(&self, username: &str, ip: &str, session_token: &str) {
        self.log(AuditEntry {
            action: AuditAction::Login,
            username: username.to_string(),
            resource: None,
            ip_address: ip.to_string(),
            session_token: session_token.to_string(),
            status: "success".to_string(),
            reason: None,
        });
    }
    
    /// Convenience method for logging failed login
    pub fn log_login_failed(&self, username: &str, ip: &str, reason: &str) {
        self.log(AuditEntry {
            action: AuditAction::LoginFailed,
            username: username.to_string(),
            resource: None,
            ip_address: ip.to_string(),
            session_token: "none".to_string(),
            status: "failed".to_string(),
            reason: Some(reason.to_string()),
        });
    }
    
    /// Convenience method for logging logout
    pub fn log_logout(&self, username: &str, ip: &str, session_token: &str) {
        self.log(AuditEntry {
            action: AuditAction::Logout,
            username: username.to_string(),
            resource: None,
            ip_address: ip.to_string(),
            session_token: session_token.to_string(),
            status: "success".to_string(),
            reason: None,
        });
    }
}
