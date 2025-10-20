// Audit log API endpoints for admin dashboard
// Provides read access to admin_dashboard.log with filtering and pagination

use crate::webui::api::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::fs;

/// Parsed audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: String,
    pub action: String,
    pub user: String,
    pub resource: Option<String>,
    pub ip_address: String,
    pub session_token: String,
    pub status: String,
    pub reason: Option<String>,
}

/// Query parameters for audit log filtering
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Filter by action type (LOGIN, LOGOUT, VIEW, UPDATE, DELETE)
    #[serde(default)]
    pub action: Option<String>,

    /// Filter by username
    #[serde(default)]
    pub user: Option<String>,

    /// Filter by status (success, failed)
    #[serde(default)]
    pub status: Option<String>,

    /// Search in resource or reason fields
    #[serde(default)]
    pub search: Option<String>,

    /// Page number (1-based)
    #[serde(default = "default_page")]
    pub page: usize,

    /// Page size
    #[serde(default = "default_page_size")]
    pub limit: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    50
}

/// Response with paginated audit log entries
#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub entries: Vec<AuditLogEntry>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// Parse a single audit log line
/// Format: [TIMESTAMP] ACTION=X USER=Y RESOURCE=Z IP=A SESSION=B STATUS=C REASON="D"
fn parse_audit_line(line: &str) -> Option<AuditLogEntry> {
    if line.trim().is_empty() {
        return None;
    }

    // Extract timestamp
    let timestamp = if let Some(end) = line.find(']') {
        line[1..end].to_string()
    } else {
        return None;
    };

    // Parse key-value pairs
    let rest = if let Some(start) = line.find(']') {
        &line[start + 1..]
    } else {
        return None;
    };

    let mut action = String::new();
    let mut user = String::new();
    let mut resource = None;
    let mut ip_address = String::new();
    let mut session_token = String::new();
    let mut status = String::new();
    let mut reason = None;

    // Simple parser for key=value format
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];

        if let Some(eq_pos) = part.find('=') {
            let key = &part[..eq_pos];
            let mut value = part[eq_pos + 1..].to_string();

            // Handle quoted values (REASON="...")
            if value.starts_with('"') {
                value = value[1..].to_string();
                // Collect rest of quoted string
                i += 1;
                while i < parts.len() && !parts[i - 1].ends_with('"') {
                    value.push(' ');
                    value.push_str(parts[i]);
                    i += 1;
                }
                // Remove trailing quote
                if value.ends_with('"') {
                    value.pop();
                }
                i -= 1; // Adjust because we'll increment at loop end
            }

            match key {
                "ACTION" => action = value,
                "USER" => user = value,
                "RESOURCE" => resource = Some(value),
                "IP" => ip_address = value,
                "SESSION" => session_token = value,
                "STATUS" => status = value,
                "REASON" => reason = Some(value),
                _ => {}
            }
        }

        i += 1;
    }

    Some(AuditLogEntry {
        timestamp,
        action,
        user,
        resource,
        ip_address,
        session_token,
        status,
        reason,
    })
}

/// Get audit log entries with filtering and pagination
///
/// GET /api/audit/logs
/// Query params:
/// - action: Filter by action type
/// - user: Filter by username
/// - status: Filter by status (success/failed)
/// - search: Search in resource/reason fields
/// - page: Page number (default 1)
/// - limit: Page size (default 50)
pub async fn get_audit_logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuditLogQuery>,
) -> impl IntoResponse {
    // Determine log file path from config
    let log_path = if let Some(ref dir) = state.config.audit_log_directory {
        std::path::PathBuf::from(dir).join(&state.config.audit_log_file)
    } else {
        // Default to data directory
        let storage = match &state.storage {
            Some(s) => s.lock().await,
            None => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuditLogResponse {
                        entries: vec![],
                        total: 0,
                        page: query.page,
                        page_size: query.limit,
                        total_pages: 0,
                    }),
                )
            }
        };
        let data_dir = storage.base_dir();
        std::path::PathBuf::from(data_dir).join(&state.config.audit_log_file)
    };

    // Read the log file
    let content = match fs::read_to_string(&log_path).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read audit log {:?}: {}", log_path, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditLogResponse {
                    entries: vec![],
                    total: 0,
                    page: query.page,
                    page_size: query.limit,
                    total_pages: 0,
                }),
            );
        }
    };

    // Parse all lines
    let mut all_entries: Vec<AuditLogEntry> =
        content.lines().filter_map(parse_audit_line).collect();

    // Reverse to show newest first
    all_entries.reverse();

    // Apply filters
    let filtered: Vec<AuditLogEntry> = all_entries
        .into_iter()
        .filter(|entry| {
            // Filter by action
            if let Some(ref action) = query.action {
                if !entry.action.eq_ignore_ascii_case(action) {
                    return false;
                }
            }

            // Filter by user
            if let Some(ref user) = query.user {
                if !entry.user.eq_ignore_ascii_case(user) {
                    return false;
                }
            }

            // Filter by status
            if let Some(ref status) = query.status {
                if !entry.status.eq_ignore_ascii_case(status) {
                    return false;
                }
            }

            // Search in resource and reason
            if let Some(ref search) = query.search {
                let search_lower = search.to_lowercase();
                let in_resource = entry
                    .resource
                    .as_ref()
                    .map(|r| r.to_lowercase().contains(&search_lower))
                    .unwrap_or(false);
                let in_reason = entry
                    .reason
                    .as_ref()
                    .map(|r| r.to_lowercase().contains(&search_lower))
                    .unwrap_or(false);

                if !in_resource && !in_reason {
                    return false;
                }
            }

            true
        })
        .collect();

    let total = filtered.len();
    let total_pages = (total + query.limit - 1) / query.limit;

    // Apply pagination
    let start = (query.page.saturating_sub(1)) * query.limit;
    let entries: Vec<AuditLogEntry> = filtered.into_iter().skip(start).take(query.limit).collect();

    (
        StatusCode::OK,
        Json(AuditLogResponse {
            entries,
            total,
            page: query.page,
            page_size: query.limit,
            total_pages,
        }),
    )
}
