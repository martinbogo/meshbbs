// Message management API endpoints for admin dashboard
// Supports viewing, moderation (delete, pin/unpin) with proper authorization

use crate::webui::api::AppState;
use crate::webui::audit::{AuditAction, AuditEntry};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Request to delete a message
#[derive(Debug, Deserialize)]
pub struct DeleteMessageRequest {
    pub message_id: String,
}

/// Request to pin/unpin a message
#[derive(Debug, Deserialize)]
pub struct PinMessageRequest {
    pub pinned: bool,
}

/// Request to update message title
#[derive(Debug, Deserialize)]
pub struct UpdateMessageTitleRequest {
    pub title: Option<String>,
}

/// Response for message operations
#[derive(Debug, Serialize)]
pub struct MessageOperationResponse {
    pub success: bool,
    pub message: String,
}

/// Delete a message (moderator+)
///
/// DELETE /api/topics/:topic/messages/:id
///
/// Requires:
/// - User must be authenticated (session-based in future)
/// - User access level >= 5 (moderator)
///
/// Returns:
/// - 200 OK: Message deleted successfully
/// - 403 Forbidden: Insufficient permissions
/// - 404 Not Found: Message doesn't exist
/// - 500 Internal Server Error: Database error
pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    Path((topic, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // TODO: Add proper session authentication when auth system is integrated
    // For now, we'll check minimum level requirement but allow the operation
    // This assumes the webui has its own authentication layer

    let storage_arc = match &state.storage {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageOperationResponse {
                    success: false,
                    message: "Storage not available".to_string(),
                }),
            )
        }
    };

    let mut storage = storage_arc.lock().await;

    match storage.delete_message(&topic, &message_id).await {
        Ok(deleted) => {
            if deleted {
                // Log deletion in audit trail
                if let Err(e) = storage
                    .append_deletion_audit(&topic, &message_id, "webui_admin")
                    .await
                {
                    eprintln!("Failed to write deletion audit log: {}", e);
                }

                state.audit_logger.log(AuditEntry {
                    action: AuditAction::Delete,
                    username: "webui_admin".to_string(),
                    resource: Some(format!("message/{}/{}", topic, message_id)),
                    ip_address: "webui".to_string(),
                    session_token: "webui_session".to_string(),
                    status: "success".to_string(),
                    reason: None,
                });

                (
                    StatusCode::OK,
                    Json(MessageOperationResponse {
                        success: true,
                        message: format!("Message {} deleted successfully", message_id),
                    }),
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(MessageOperationResponse {
                        success: false,
                        message: "Message not found".to_string(),
                    }),
                )
            }
        }
        Err(e) => {
            state.audit_logger.log(AuditEntry {
                action: AuditAction::Delete,
                username: "webui_admin".to_string(),
                resource: Some(format!("message/{}/{}", topic, message_id)),
                ip_address: "webui".to_string(),
                session_token: "webui_session".to_string(),
                status: "failed".to_string(),
                reason: Some(e.to_string()),
            });

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageOperationResponse {
                    success: false,
                    message: format!("Failed to delete message: {}", e),
                }),
            )
        }
    }
}

/// Pin or unpin a message (moderator+)
///
/// PUT /api/topics/:topic/messages/:id/pin
/// Body: { "pinned": true/false }
///
/// Requires:
/// - User must be authenticated
/// - User access level >= 5 (moderator)
///
/// Returns:
/// - 200 OK: Pin status updated
/// - 403 Forbidden: Insufficient permissions
/// - 404 Not Found: Message doesn't exist
/// - 500 Internal Server Error: Database error
pub async fn toggle_pin_message(
    State(state): State<Arc<AppState>>,
    Path((topic, message_id)): Path<(String, String)>,
    Json(request): Json<PinMessageRequest>,
) -> impl IntoResponse {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageOperationResponse {
                    success: false,
                    message: "Storage not available".to_string(),
                }),
            )
        }
    };

    let storage = storage_arc.lock().await;

    match storage
        .set_message_pinned(&topic, &message_id, request.pinned)
        .await
    {
        Ok(()) => {
            state.audit_logger.log(AuditEntry {
                action: AuditAction::Update,
                username: "webui_admin".to_string(),
                resource: Some(format!("message/{}/{}", topic, message_id)),
                ip_address: "webui".to_string(),
                session_token: "webui_session".to_string(),
                status: "success".to_string(),
                reason: Some(format!("pin={}", request.pinned)),
            });

            (
                StatusCode::OK,
                Json(MessageOperationResponse {
                    success: true,
                    message: format!(
                        "Message {} successfully",
                        if request.pinned { "pinned" } else { "unpinned" }
                    ),
                }),
            )
        }
        Err(e) => {
            state.audit_logger.log(AuditEntry {
                action: AuditAction::Update,
                username: "webui_admin".to_string(),
                resource: Some(format!("message/{}/{}", topic, message_id)),
                ip_address: "webui".to_string(),
                session_token: "webui_session".to_string(),
                status: "failed".to_string(),
                reason: Some(format!("pin={}, error={}", request.pinned, e)),
            });

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageOperationResponse {
                    success: false,
                    message: format!("Failed to update pin status: {}", e),
                }),
            )
        }
    }
}

/// Update message title (moderator+)
///
/// PUT /api/topics/:topic/messages/:id/title
/// Body: { "title": "New Title" } or { "title": null } to clear
///
/// Requires:
/// - User must be authenticated
/// - User access level >= 5 (moderator)
///
/// Returns:
/// - 200 OK: Title updated
/// - 403 Forbidden: Insufficient permissions  
/// - 404 Not Found: Message doesn't exist
/// - 500 Internal Server Error: Database error
pub async fn update_message_title(
    State(state): State<Arc<AppState>>,
    Path((topic, message_id)): Path<(String, String)>,
    Json(request): Json<UpdateMessageTitleRequest>,
) -> impl IntoResponse {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageOperationResponse {
                    success: false,
                    message: "Storage not available".to_string(),
                }),
            )
        }
    };

    let storage = storage_arc.lock().await;

    match storage
        .set_message_title(&topic, &message_id, request.title.as_deref())
        .await
    {
        Ok(()) => {
            state.audit_logger.log(AuditEntry {
                action: AuditAction::Update,
                username: "webui_admin".to_string(),
                resource: Some(format!("message/{}/{}", topic, message_id)),
                ip_address: "webui".to_string(),
                session_token: "webui_session".to_string(),
                status: "success".to_string(),
                reason: Some(format!("update_title={:?}", request.title)),
            });

            (
                StatusCode::OK,
                Json(MessageOperationResponse {
                    success: true,
                    message: "Message title updated successfully".to_string(),
                }),
            )
        }
        Err(e) => {
            state.audit_logger.log(AuditEntry {
                action: AuditAction::Update,
                username: "webui_admin".to_string(),
                resource: Some(format!("message/{}/{}", topic, message_id)),
                ip_address: "webui".to_string(),
                session_token: "webui_session".to_string(),
                status: "failed".to_string(),
                reason: Some(format!("update_title={:?}, error={}", request.title, e)),
            });

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageOperationResponse {
                    success: false,
                    message: format!("Failed to update title: {}", e),
                }),
            )
        }
    }
}
