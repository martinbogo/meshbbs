//! Activity feed API for dashboard - shows recent messages, user changes, and admin actions
//!
//! Provides a unified view of recent system activity for the dashboard.

use crate::webui::api::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::Arc,
};

/// Activity item type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityType {
    Message,
    UserChange,
    AdminAction,
}

/// Unified activity entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// Activity type
    #[serde(rename = "type")]
    pub activity_type: ActivityType,
    /// When the activity occurred
    pub timestamp: String,
    /// Actor (user who performed the action)
    pub actor: String,
    /// Description of the activity
    pub description: String,
    /// Optional icon/emoji
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Optional link to resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// Query parameters for activity feed
#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    /// Limit number of results (default 10, max 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Response for activity feed
#[derive(Debug, Serialize)]
pub struct ActivityResponse {
    /// Activity entries
    pub activities: Vec<ActivityEntry>,
    /// Total number of activities
    pub total: usize,
}

/// Parse an audit log line into an ActivityEntry
fn parse_audit_activity(line: &str) -> Option<ActivityEntry> {
    // Format: [TIMESTAMP] ACTION=X USER=Y RESOURCE=Z IP=A SESSION=B STATUS=C REASON="D"
    let timestamp_end = line.find(']')?;
    let timestamp = line[1..timestamp_end].trim();

    let rest = &line[timestamp_end + 1..];
    let mut action = None;
    let mut user = None;
    let mut resource = None;
    let mut reason = None;

    // Parse key=value pairs
    let mut current_pos = 0;
    while current_pos < rest.len() {
        // Find next key
        let key_start = rest[current_pos..]
            .find(|c: char| c.is_alphabetic())
            .map(|i| current_pos + i)?;
        let key_end = rest[key_start..].find('=').map(|i| key_start + i)?;
        let key = &rest[key_start..key_end];

        // Find value
        let value_start = key_end + 1;
        let (value, next_pos) = if rest[value_start..].starts_with('"') {
            // Quoted value
            let quote_end = rest[value_start + 1..]
                .find('"')
                .map(|i| value_start + 1 + i)?;
            (&rest[value_start + 1..quote_end], quote_end + 1)
        } else {
            // Unquoted value
            let value_end = rest[value_start..]
                .find(' ')
                .map(|i| value_start + i)
                .unwrap_or(rest.len());
            (&rest[value_start..value_end], value_end)
        };

        match key {
            "ACTION" => action = Some(value.to_string()),
            "USER" => user = Some(value.to_string()),
            "RESOURCE" => resource = Some(value.to_string()),
            "REASON" => reason = Some(value.to_string()),
            _ => {}
        }

        current_pos = next_pos;
    }

    let action = action?;
    let user = user?;

    // Format description based on action
    let (description, icon) = match action.as_str() {
        "LOGIN" => (format!("logged in"), Some("🔐".to_string())),
        "LOGOUT" => (format!("logged out"), Some("🚪".to_string())),
        "VIEW" => {
            if let Some(res) = resource {
                (format!("viewed {}", res), Some("👁️".to_string()))
            } else {
                return None;
            }
        }
        "UPDATE" => {
            if let Some(res) = resource {
                let detail = reason.unwrap_or_default();
                (
                    format!("updated {} ({})", res, detail),
                    Some("✏️".to_string()),
                )
            } else {
                return None;
            }
        }
        "DELETE" => {
            if let Some(res) = resource {
                (format!("deleted {}", res), Some("🗑️".to_string()))
            } else {
                return None;
            }
        }
        _ => (format!("performed {}", action), None),
    };

    Some(ActivityEntry {
        activity_type: ActivityType::AdminAction,
        timestamp: timestamp.to_string(),
        actor: user,
        description,
        icon,
        link: None,
    })
}

/// Get recent activity feed
pub async fn get_activity_feed(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityQuery>,
) -> impl IntoResponse {
    let limit = query.limit.min(50).max(1);
    let mut activities = Vec::new();

    // Read recent messages from general topic
    if let Some(ref storage) = state.storage {
        if let Ok(messages) = storage.lock().await.get_messages("general", 5).await {
            for msg in messages {
                let timestamp = msg.timestamp.format("%Y-%m-%dT%H:%M:%S").to_string();
                let preview = if msg.content.len() > 60 {
                    format!("{}...", &msg.content[..60])
                } else {
                    msg.content.clone()
                };

                activities.push(ActivityEntry {
                    activity_type: ActivityType::Message,
                    timestamp,
                    actor: msg.author.clone(),
                    description: format!("posted: {}", preview),
                    icon: Some("💬".to_string()),
                    link: Some(format!("messages.html?topic=general&highlight={}", msg.id)),
                });
            }
        }
    }

    // Read recent audit log entries
    let audit_path = if let Some(ref storage) = state.storage {
        let guard = storage.lock().await;
        let data_dir = guard.base_dir().to_string();
        drop(guard);
        PathBuf::from(data_dir).join(&state.config.audit_log_file)
    } else {
        let total = activities.len();
        return (StatusCode::OK, Json(ActivityResponse { activities, total }));
    };

    if let Ok(file) = File::open(&audit_path) {
        let reader = BufReader::new(file);
        let mut audit_entries: Vec<ActivityEntry> = reader
            .lines()
            .filter_map(|line| line.ok())
            .filter_map(|line| parse_audit_activity(&line))
            .collect();

        // Take most recent audit entries
        audit_entries.reverse();
        audit_entries.truncate(10);
        activities.extend(audit_entries);
    }

    // Sort all activities by timestamp (newest first)
    activities.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Truncate to limit
    let total = activities.len();
    activities.truncate(limit);

    (StatusCode::OK, Json(ActivityResponse { activities, total }))
}
