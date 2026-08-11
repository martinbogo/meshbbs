//! # Authentication API Endpoints
//!
//! Login, logout, and session management endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{AdminDashboardConfig, AppsConfig};
use crate::storage::Storage;
use crate::tmush::storage::TinyMushStore;
use crate::webui::audit::{AuditAction, AuditEntry, AuditLogger};
use crate::webui::auth::AuthManager;
use crate::webui::schema::SchemaRegistry;
use tokio::sync::{Mutex, RwLock};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub auth_manager: Arc<AuthManager>,
    pub audit_logger: AuditLogger,
    pub sysop_password_hash: String,
    pub sysop_username: String,
    pub storage: Option<Arc<Mutex<Storage>>>, // BBS storage for user management (wrapped in Arc<Mutex> for shared mutable access)
    pub config: AdminDashboardConfig,
    pub schema_registry: Arc<SchemaRegistry>,
    pub data_dir: PathBuf,
    pub config_path: Option<PathBuf>,
    pub games: Arc<RwLock<AppsConfig>>,
    pub tinymush_store: Option<Arc<TinyMushStore>>,
    pub tinymush_store_error: Option<String>,
    pub tinymush_db_path: PathBuf,
}

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub admin_level: u8,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Login endpoint
pub async fn login(State(state): State<Arc<AppState>>, Json(req): Json<LoginRequest>) -> Response {
    // For now, only support sysop login
    // TODO: Support multiple admin users from database
    if req.username != state.sysop_username {
        state
            .audit_logger
            .log_login_failed(&req.username, "unknown", "invalid_username");
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid credentials".to_string(),
            }),
        )
            .into_response();
    }

    // Verify credentials
    match state
        .auth_manager
        .verify_credentials(
            &req.username,
            &req.password,
            &state.sysop_password_hash,
            10, // Sysop level
        )
        .await
    {
        Ok(true) => {
            // Create session
            match state.auth_manager.create_session(&req.username, 10).await {
                Ok(token) => {
                    state
                        .audit_logger
                        .log_login(&req.username, "unknown", &token);
                    (
                        StatusCode::OK,
                        Json(LoginResponse {
                            token: token.clone(),
                            username: req.username,
                            admin_level: 10,
                        }),
                    )
                        .into_response()
                }
                Err(e) => {
                    state.audit_logger.log_login_failed(
                        &req.username,
                        "unknown",
                        &format!("session_creation_failed: {}", e),
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Failed to create session".to_string(),
                        }),
                    )
                        .into_response()
                }
            }
        }
        Ok(false) => {
            state
                .audit_logger
                .log_login_failed(&req.username, "unknown", "invalid_password");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Invalid credentials".to_string(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            state.audit_logger.log_login_failed(
                &req.username,
                "unknown",
                &format!("verification_error: {}", e),
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Authentication error".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Logout endpoint
pub async fn logout(
    State(state): State<Arc<AppState>>,
    // TODO: Extract token from Authorization header
) -> Response {
    // TODO: Implement logout
    state.audit_logger.log(AuditEntry {
        action: AuditAction::Logout,
        username: "unknown".to_string(),
        resource: None,
        ip_address: "unknown".to_string(),
        session_token: "unknown".to_string(),
        status: "success".to_string(),
        reason: None,
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "logged_out" })),
    )
        .into_response()
}
