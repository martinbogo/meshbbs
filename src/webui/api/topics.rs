//! Topics and messages API endpoints for BBS administration.
//!
//! Provides read-only access to message boards:
//! - List all topics
//! - Get messages in a topic
//! - Get message statistics

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;

use super::auth::AppState;

/// Response for a topic list entry
#[derive(Debug, Clone, Serialize)]
pub struct TopicSummary {
    pub name: String,
    pub message_count: usize,
    pub last_message_time: Option<String>,  // ISO 8601
}

/// Response for a single message
#[derive(Debug, Clone, Serialize)]
pub struct MessageRecord {
    pub id: String,
    pub topic: String,
    pub author: String,
    pub title: Option<String>,
    pub content: String,
    pub timestamp: String,  // ISO 8601
    pub reply_count: usize,
    pub pinned: bool,
}

/// Response for message statistics
#[derive(Debug, Clone, Serialize)]
pub struct MessageStats {
    pub total_messages: usize,
    pub total_replies: usize,
    pub authors: Vec<String>,
    pub first_message: Option<String>,  // ISO 8601
    pub last_message: Option<String>,   // ISO 8601
}

/// Query parameters for message list
#[derive(Debug, Deserialize)]
pub struct MessageListQuery {
    pub limit: Option<usize>,   // Max messages to return (default: 50, max: 200)
    pub offset: Option<usize>,  // Pagination offset
}

/// List all message topics with statistics
///
/// GET /api/topics
pub async fn list_topics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TopicSummary>>, StatusCode> {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => {
            error!("Storage not available in AppState");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let storage = storage_arc.lock().await;

    // Get list of topic names
    let topic_names = storage.list_message_topics()
        .await
        .map_err(|e| {
            error!("Failed to list topics: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // For each topic, get messages to calculate stats
    let mut topics = Vec::new();
    for topic_name in topic_names {
        let messages = storage.get_messages(&topic_name, 1000) // Get all for stats
            .await
            .map_err(|e| {
                error!("Failed to get messages for topic {}: {}", topic_name, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let last_message_time = messages.last().map(|m| m.timestamp.to_rfc3339());

        topics.push(TopicSummary {
            name: topic_name,
            message_count: messages.len(),
            last_message_time,
        });
    }

    // Sort by last message time (most recent first)
    topics.sort_by(|a, b| {
        b.last_message_time.cmp(&a.last_message_time)
    });

    drop(storage);

    state.audit_logger.log(crate::webui::audit::AuditEntry {
        action: crate::webui::audit::AuditAction::View,
        username: state.sysop_username.clone(),
        resource: Some("topics".to_string()),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });

    Ok(Json(topics))
}

/// List messages in a specific topic
///
/// GET /api/topics/:topic/messages?limit=50&offset=0
pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    Path(topic): Path<String>,
    Query(query): Query<MessageListQuery>,
) -> Result<Json<Vec<MessageRecord>>, StatusCode> {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let storage = storage_arc.lock().await;

    // Get messages from storage
    let limit = query.limit.unwrap_or(50).min(200); // Default 50, max 200
    let messages = storage.get_messages(&topic, limit)
        .await
        .map_err(|e| {
            error!("Failed to get messages for topic {}: {}", topic, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Convert to response format
    let mut records: Vec<MessageRecord> = messages
        .into_iter()
        .map(|msg| MessageRecord {
            id: msg.id.clone(),
            topic: msg.topic.clone(),
            author: msg.author.clone(),
            title: msg.title.clone(),
            content: msg.content.clone(),
            timestamp: msg.timestamp.to_rfc3339(),
            reply_count: msg.replies.len(),
            pinned: msg.pinned,
        })
        .collect();

    // Apply pagination offset
    let offset = query.offset.unwrap_or(0);
    if offset < records.len() {
        records = records.into_iter().skip(offset).take(limit).collect();
    } else {
        records.clear();
    }

    drop(storage);

    state.audit_logger.log(crate::webui::audit::AuditEntry {
        action: crate::webui::audit::AuditAction::View,
        username: state.sysop_username.clone(),
        resource: Some(format!("topic/{}/messages", topic)),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });

    Ok(Json(records))
}

/// Get statistics for a specific topic
///
/// GET /api/topics/:topic/stats
pub async fn get_topic_stats(
    State(state): State<Arc<AppState>>,
    Path(topic): Path<String>,
) -> Result<Json<MessageStats>, StatusCode> {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let storage = storage_arc.lock().await;

    // Get all messages for stats calculation
    let messages = storage.get_messages(&topic, 10000)
        .await
        .map_err(|e| {
            error!("Failed to get messages for topic {}: {}", topic, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Calculate statistics
    let total_messages = messages.len();
    let total_replies: usize = messages.iter().map(|m| m.replies.len()).sum();
    
    let mut authors: Vec<String> = messages.iter()
        .map(|m| m.author.clone())
        .collect();
    authors.sort();
    authors.dedup();

    let first_message = messages.first().map(|m| m.timestamp.to_rfc3339());
    let last_message = messages.last().map(|m| m.timestamp.to_rfc3339());

    drop(storage);

    Ok(Json(MessageStats {
        total_messages,
        total_replies,
        authors,
        first_message,
        last_message,
    }))
}
