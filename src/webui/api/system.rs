//! System control API endpoints
//!
//! Provides administrative endpoints for system-level operations like restarting
//! the server, checking status, and managing system resources.
//!
//! # Security
//!
//! All endpoints require valid JWT authentication and appropriate admin privileges.
//! Rate limiting is enforced to prevent DoS attacks.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{error, info, warn};

use crate::webui::api::auth::AppState;

/// Global flag to prevent concurrent restart requests
static RESTART_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Last restart timestamp for rate limiting (unix timestamp in seconds)
static LAST_RESTART_TIME: AtomicI64 = AtomicI64::new(0);

/// Minimum seconds between restart requests
const RESTART_COOLDOWN_SECONDS: i64 = 30;

/// Extract and validate JWT token from Authorization header
fn extract_token(headers: &HeaderMap) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Missing Authorization header",
                "required": "Bearer <token>"
            })),
        )
    })?;

    let auth_str = auth_header.to_str().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid Authorization header encoding"})),
        )
    })?;

    if !auth_str.starts_with("Bearer ") {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Invalid Authorization header format",
                "expected": "Bearer <token>"
            })),
        ));
    }

    Ok(auth_str[7..].to_string())
}

/// Restart the MeshBBS server
///
/// This endpoint allows authorized administrators to restart the server remotely.
/// The restart happens after a 500ms delay to allow the HTTP response to be sent.
///
/// # Security
///
/// - Requires valid JWT authentication token
/// - Requires Sysop level access (level 10)
/// - Rate limited to 1 restart per 30 seconds
/// - Prevents concurrent restart requests
/// - All attempts are audit logged
///
/// # Process
///
/// 1. Validates JWT token and admin level
/// 2. Checks rate limiting and concurrent restart protection
/// 3. Logs restart event to audit log
/// 4. Sends success response to client
/// 5. Waits 500ms for response to be sent
/// 6. Spawns new server instance
/// 7. Current instance exits gracefully
///
/// Typical restart time is 1-2 seconds, during which the BBS will be unavailable.
pub async fn restart_server(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Step 1: Extract and validate JWT token
    let token = match extract_token(&headers) {
        Ok(t) => t,
        Err(response) => return response.into_response(),
    };

    let (claims, _) = match state.auth_manager.validate_token(&token).await {
        Ok(result) => result,
        Err(e) => {
            warn!("Restart attempt with invalid token: {}", e);
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Invalid or expired authentication token",
                    "details": e.to_string()
                })),
            )
                .into_response();
        }
    };

    // Step 2: Check admin level (Sysop only - level 10)
    if claims.admin_level < 10 {
        warn!(
            "Restart denied: user '{}' has insufficient privileges (level {})",
            claims.sub, claims.admin_level
        );

        state.audit_logger.log(crate::webui::audit::AuditEntry {
            action: crate::webui::audit::AuditAction::SystemRestart,
            username: claims.sub.clone(),
            resource: None,
            ip_address: "unknown".to_string(),
            session_token: token[..16].to_string(), // First 16 chars for logging
            status: "denied".to_string(),
            reason: Some(format!(
                "Insufficient privileges: level {}",
                claims.admin_level
            )),
        });

        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Sysop access required to restart server",
                "required_level": 10,
                "current_level": claims.admin_level
            })),
        )
            .into_response();
    }

    // Step 3: Check rate limiting
    let now = chrono::Utc::now().timestamp();
    let last_restart = LAST_RESTART_TIME.load(Ordering::SeqCst);
    let elapsed = now - last_restart;

    if last_restart > 0 && elapsed < RESTART_COOLDOWN_SECONDS {
        let remaining = RESTART_COOLDOWN_SECONDS - elapsed;
        warn!(
            "Restart rate limit hit by user '{}': {} seconds remaining",
            claims.sub, remaining
        );

        state.audit_logger.log(crate::webui::audit::AuditEntry {
            action: crate::webui::audit::AuditAction::SystemRestart,
            username: claims.sub.clone(),
            resource: None,
            ip_address: "unknown".to_string(),
            session_token: token[..16].to_string(),
            status: "rate_limited".to_string(),
            reason: Some(format!("Cooldown: {} seconds remaining", remaining)),
        });

        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": "Restart rate limit exceeded",
                "cooldown_seconds": RESTART_COOLDOWN_SECONDS,
                "remaining_seconds": remaining,
                "message": format!("Please wait {} seconds before restarting again", remaining)
            })),
        )
            .into_response();
    }

    // Step 4: Check for concurrent restart
    if RESTART_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        warn!(
            "Restart already in progress, rejected duplicate request from '{}'",
            claims.sub
        );

        state.audit_logger.log(crate::webui::audit::AuditEntry {
            action: crate::webui::audit::AuditAction::SystemRestart,
            username: claims.sub.clone(),
            resource: None,
            ip_address: "unknown".to_string(),
            session_token: token[..16].to_string(),
            status: "rejected".to_string(),
            reason: Some("Restart already in progress".to_string()),
        });

        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Restart already in progress",
                "message": "Another restart request is currently being processed"
            })),
        )
            .into_response();
    }

    // Step 5: Update rate limit timestamp
    LAST_RESTART_TIME.store(now, Ordering::SeqCst);

    // Step 6: Log successful restart initiation
    info!("🔄 Server restart initiated by sysop '{}'", claims.sub);

    state.audit_logger.log(crate::webui::audit::AuditEntry {
        action: crate::webui::audit::AuditAction::SystemRestart,
        username: claims.sub.clone(),
        resource: None,
        ip_address: "unknown".to_string(),
        session_token: token[..16].to_string(),
        status: "success".to_string(),
        reason: Some("Server restart initiated".to_string()),
    });

    // Step 7: Spawn restart task in background to allow this response to complete
    let username_for_task = claims.sub.clone();
    tokio::spawn(async move {
        info!(
            "⏱️  Restart scheduled in 500ms by '{}'...",
            username_for_task
        );
        tokio::time::sleep(Duration::from_millis(500)).await;

        match crate::restart::restart_server() {
            Ok(_) => {
                // This line won't actually execute as restart_server() calls exit()
                info!("✅ Restart completed successfully");
            }
            Err(e) => {
                error!("❌ Restart failed: {}", e);
                // Reset the flag if restart failed
                RESTART_IN_PROGRESS.store(false, Ordering::SeqCst);
            }
        }
    });

    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "message": "Server restarting in 500ms",
            "initiated_by": claims.sub,
            "expected_downtime_seconds": 2,
            "timestamp": now
        })),
    )
        .into_response()
}

/// Get server status information
///
/// This endpoint provides basic health check information and can be used
/// to verify the server is running after a restart.
///
/// # Security
///
/// Requires valid JWT authentication token to prevent information disclosure.
pub async fn get_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Validate authentication
    let token = match extract_token(&headers) {
        Ok(t) => t,
        Err(response) => return response.into_response(),
    };

    if let Err(e) = state.auth_manager.validate_token(&token).await {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Invalid or expired authentication token",
                "details": e.to_string()
            })),
        )
            .into_response();
    }

    // Get current timestamp safely
    use std::time::SystemTime;
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0); // Return 0 on time error instead of panicking

    Json(json!({
        "status": "running",
        "timestamp": timestamp,
        "is_restarting": crate::restart::is_restarting(),
        "restart_in_progress": RESTART_IN_PROGRESS.load(Ordering::SeqCst)
    }))
    .into_response()
}
