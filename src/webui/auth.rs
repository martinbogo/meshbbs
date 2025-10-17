//! # Authentication Module
//!
//! Session-based authentication using BBS admin credentials.
//!
//! ## Features
//!
//! - Reuses existing BBS admin password hash (Argon2)
//! - 24-hour session tokens with rotation
//! - Token replay prevention via rotation
//! - Max concurrent sessions per admin
//! - Strict expiry enforcement
//!
//! ## Token Format
//!
//! JWT tokens with claims:
//! - `sub`: Username
//! - `exp`: Expiration timestamp
//! - `iat`: Issued at timestamp
//! - `jti`: Token ID (for rotation tracking)

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "webui")]
use crate::config::AdminDashboardConfig;

#[cfg(feature = "webui")]
use argon2::{
    password_hash::{PasswordHash, PasswordVerifier},
    Argon2,
};

#[cfg(feature = "webui")]
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

/// JWT claims for session tokens
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionClaims {
    pub sub: String,        // Username
    pub exp: i64,           // Expiration timestamp
    pub iat: i64,           // Issued at timestamp
    pub jti: String,        // Token ID (UUID)
    pub admin_level: u8,    // Admin level for authorization
}

/// Session information
#[derive(Debug, Clone)]
pub struct Session {
    pub username: String,
    pub admin_level: u8,
    pub token_id: String,
    pub created_at: i64,
    pub last_activity: i64,
}

/// Authentication manager
pub struct AuthManager {
    config: AdminDashboardConfig,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    jwt_secret: Vec<u8>,
}

impl AuthManager {
    /// Create new authentication manager
    #[cfg(feature = "webui")]
    pub fn new(config: AdminDashboardConfig) -> Self {
        // Generate random JWT secret (in production, this should be persisted)
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let jwt_secret: Vec<u8> = (0..64).map(|_| rng.gen::<u8>()).collect();
        
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret,
        }
    }
    
    /// Verify admin credentials using BBS password hash
    #[cfg(feature = "webui")]
    pub async fn verify_credentials(
        &self,
        _username: &str,
        password: &str,
        bbs_password_hash: &str,
        admin_level: u8,
    ) -> Result<bool> {
        // Check if user has required admin level
        if admin_level < self.config.require_admin_level {
            return Ok(false);
        }
        
        // Verify password using Argon2
        let parsed_hash = PasswordHash::new(bbs_password_hash)
            .map_err(|e| anyhow!("Invalid password hash: {}", e))?;
        
        let argon2 = Argon2::default();
        Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }
    
    /// Create new session and generate JWT token
    #[cfg(feature = "webui")]
    pub async fn create_session(&self, username: &str, admin_level: u8) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        let exp = now + self.config.session_timeout as i64;
        let token_id = uuid::Uuid::new_v4().to_string();
        
        let claims = SessionClaims {
            sub: username.to_string(),
            exp,
            iat: now,
            jti: token_id.clone(),
            admin_level,
        };
        
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| anyhow!("Failed to generate JWT: {}", e))?;
        
        // Store session
        let session = Session {
            username: username.to_string(),
            admin_level,
            token_id: token_id.clone(),
            created_at: now,
            last_activity: now,
        };
        
        let mut sessions = self.sessions.write().await;
        
        // Enforce max sessions per admin
        let user_sessions: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| s.username == username)
            .map(|(k, _)| k.clone())
            .collect();
        
        if user_sessions.len() >= self.config.max_sessions_per_admin as usize {
            // Remove oldest session
            if let Some(oldest) = user_sessions.first() {
                sessions.remove(oldest);
            }
        }
        
        sessions.insert(token.clone(), session);
        
        Ok(token)
    }
    
    /// Validate and potentially rotate session token
    #[cfg(feature = "webui")]
    pub async fn validate_token(&self, token: &str) -> Result<(SessionClaims, Option<String>)> {
        // Decode and validate JWT
        let token_data = decode::<SessionClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
        )
        .map_err(|e| anyhow!("Invalid token: {}", e))?;
        
        let claims = token_data.claims;
        
        // Check expiry
        let now = chrono::Utc::now().timestamp();
        if self.config.enforce_token_expiry && claims.exp < now {
            return Err(anyhow!("Token expired"));
        }
        
        // Update last activity
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(token) {
            session.last_activity = now;
        } else {
            return Err(anyhow!("Session not found"));
        }
        
        // Rotate token if enabled
        let new_token = if self.config.session_token_rotation {
            drop(sessions); // Release lock before creating new session
            let rotated = self.create_session(&claims.sub, claims.admin_level).await?;
            
            // Remove old token
            let mut sessions = self.sessions.write().await;
            sessions.remove(token);
            
            Some(rotated)
        } else {
            None
        };
        
        Ok((claims, new_token))
    }
    
    /// Invalidate session (logout)
    pub async fn invalidate_session(&self, token: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(token);
    }
    
    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) {
        let now = chrono::Utc::now().timestamp();
        let mut sessions = self.sessions.write().await;
        
        sessions.retain(|_, session| {
            let age = now - session.created_at;
            age < self.config.session_timeout as i64
        });
    }
}
