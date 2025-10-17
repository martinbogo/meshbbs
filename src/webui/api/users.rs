//! User management API endpoints for BBS administration.
//!
//! Provides CRUD operations for BBS user accounts:
//! - List all users with filtering
//! - View user details
//! - Update user levels
//! - View user activity

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;

use super::auth::AppState;

/// Response for a single user record
#[derive(Debug, Clone, Serialize)]
pub struct UserRecord {
    pub username: String,
    pub role: String,           // Derived from level (User/Moderator/Admin/Sysop)
    pub level: u8,
    pub last_seen: String,      // ISO 8601 timestamp (last_login)
    pub message_count: u32,
    pub has_password: bool,
    pub created_at: String,     // ISO 8601 timestamp (first_login)
    pub node_id: Option<String>,
}

/// Request to update a user's level
#[derive(Debug, Deserialize)]
pub struct UpdateLevelRequest {
    pub level: u8,  // 1-10
}

/// Response for successful operations
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

/// Query parameters for user list filtering
#[derive(Debug, Deserialize)]
pub struct UserListQuery {
    pub min_level: Option<u8>,     // Filter by minimum level
    pub max_level: Option<u8>,     // Filter by maximum level
    pub limit: Option<usize>,       // Pagination
    pub offset: Option<usize>,      // Pagination
}

/// Convert user level to human-readable role
fn level_to_role(level: u8) -> String {
    match level {
        10 => "Sysop".to_string(),
        6..=9 => "Admin".to_string(),
        3..=5 => "Moderator".to_string(),
        _ => "User".to_string(),
    }
}

/// List all BBS users with optional filtering
///
/// GET /api/users?min_level=5&max_level=10&limit=50&offset=0
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UserListQuery>,
) -> Result<Json<Vec<UserRecord>>, StatusCode> {
    // Get storage from AppState
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => {
            error!("Storage not available in AppState");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let storage = storage_arc.lock().await;

    // Get all users from storage
    let users = storage.list_all_users()
        .await
        .map_err(|e| {
            error!("Failed to list users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Convert to UserRecord format with filtering
    let mut records: Vec<UserRecord> = users
        .into_iter()
        .filter_map(|user| {
            // Apply level filters if specified
            if let Some(min_level) = query.min_level {
                if user.user_level < min_level {
                    return None;
                }
            }
            if let Some(max_level) = query.max_level {
                if user.user_level > max_level {
                    return None;
                }
            }

            Some(UserRecord {
                username: user.username.clone(),
                role: level_to_role(user.user_level),
                level: user.user_level,
                last_seen: user.last_login.to_rfc3339(),
                message_count: user.total_messages,
                has_password: user.password_hash.is_some(),
                created_at: user.first_login.to_rfc3339(),
                node_id: user.node_id.clone(),
            })
        })
        .collect();

    // Apply pagination
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).min(1000); // Max 1000 records

    if offset < records.len() {
        records = records.into_iter().skip(offset).take(limit).collect();
    } else {
        records.clear();
    }

    drop(storage); // Release lock before audit log

    state.audit_logger.log_user_list(&state.sysop_username, "webui_session");

    Ok(Json(records))
}

/// Get details for a specific user
///
/// GET /api/users/:username
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> Result<Json<UserRecord>, StatusCode> {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let storage = storage_arc.lock().await;

    let user = storage.get_user(&username)
        .await
        .map_err(|e| {
            error!("Failed to get user {}: {}", username, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let record = UserRecord {
        username: user.username.clone(),
        role: level_to_role(user.user_level),
        level: user.user_level,
        last_seen: user.last_login.to_rfc3339(),
        message_count: user.total_messages,
        has_password: user.password_hash.is_some(),
        created_at: user.first_login.to_rfc3339(),
        node_id: user.node_id.clone(),
    };

    drop(storage); // Release lock

    state.audit_logger.log_user_view(&state.sysop_username, &username, "webui_session");

    Ok(Json(record))
}

/// Update a user's level
///
/// PUT /api/users/:username/level
pub async fn update_user_level(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
    Json(req): Json<UpdateLevelRequest>,
) -> Result<Json<SuccessResponse>, StatusCode> {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Validate level (1-10)
    if req.level == 0 || req.level > 10 {
        return Ok(Json(SuccessResponse {
            success: false,
            message: "Invalid level. Must be between 1 and 10.".to_string(),
        }));
    }

    let mut storage = storage_arc.lock().await;

    // Use storage's update_user_level method which includes sysop protection
    let updated_user = storage.update_user_level(&username, req.level, &state.sysop_username)
        .await
        .map_err(|e| {
            error!("Failed to update user level for {}: {}", username, e);
            // Check if it's a sysop protection error
            if e.to_string().contains("sysop") {
                return StatusCode::FORBIDDEN;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    drop(storage); // Release lock

    state.audit_logger.log_user_update(
        &state.sysop_username,
        &username,
        &format!("level changed to {}", req.level),
        "webui_session",
    );

    Ok(Json(SuccessResponse {
        success: true,
        message: format!(
            "Updated {} to level {} ({})",
            username,
            req.level,
            level_to_role(updated_user.user_level)
        ),
    }))
}
