//! System statistics API endpoint for BBS administration.
//!
//! Provides comprehensive system health and usage metrics.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::error;

use super::auth::AppState;

/// System statistics response
#[derive(Debug, Clone, Serialize)]
pub struct SystemStats {
    // User statistics
    pub total_users: usize,
    pub users_with_passwords: usize,
    pub sysops: usize,
    pub admins: usize,
    pub moderators: usize,
    pub regular_users: usize,
    
    // Message statistics
    pub total_topics: usize,
    pub total_messages: usize,
    pub total_replies: usize,
    
    // Activity metrics
    pub unique_message_authors: usize,
    pub messages_per_topic: Vec<TopicMessageCount>,
    
    // System info
    pub bbs_name: String,
    pub bbs_location: String,
    pub uptime_seconds: u64,
}

/// Message count per topic
#[derive(Debug, Clone, Serialize)]
pub struct TopicMessageCount {
    pub topic: String,
    pub count: usize,
}

/// Get comprehensive system statistics
///
/// GET /api/stats
pub async fn get_system_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SystemStats>, StatusCode> {
    let storage_arc = match &state.storage {
        Some(s) => s,
        None => {
            error!("Storage not available in AppState");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let storage = storage_arc.lock().await;

    // Get all users for statistics
    let users = storage.list_all_users()
        .await
        .map_err(|e| {
            error!("Failed to list users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total_users = users.len();
    let users_with_passwords = users.iter().filter(|u| u.password_hash.is_some()).count();
    let sysops = users.iter().filter(|u| u.user_level == 10).count();
    let admins = users.iter().filter(|u| u.user_level >= 6 && u.user_level < 10).count();
    let moderators = users.iter().filter(|u| u.user_level >= 3 && u.user_level < 6).count();
    let regular_users = users.iter().filter(|u| u.user_level < 3).count();

    // Get topic statistics
    let topic_names = storage.list_message_topics()
        .await
        .map_err(|e| {
            error!("Failed to list topics: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total_topics = topic_names.len();
    let mut total_messages = 0;
    let mut total_replies = 0;
    let mut all_authors = std::collections::HashSet::new();
    let mut messages_per_topic = Vec::new();

    for topic_name in topic_names {
        let messages = storage.get_messages(&topic_name, 10000) // Get all for stats
            .await
            .map_err(|e| {
                error!("Failed to get messages for topic {}: {}", topic_name, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let msg_count = messages.len();
        total_messages += msg_count;

        for msg in &messages {
            total_replies += msg.replies.len();
            all_authors.insert(msg.author.clone());
        }

        messages_per_topic.push(TopicMessageCount {
            topic: topic_name,
            count: msg_count,
        });
    }

    // Sort topics by message count (descending)
    messages_per_topic.sort_by(|a, b| b.count.cmp(&a.count));

    let unique_message_authors = all_authors.len();

    drop(storage);

    // Calculate uptime (we'll use a simple approximation - in production this would track actual start time)
    let uptime_seconds = 0; // TODO: Track actual server start time

    state.audit_logger.log(crate::webui::audit::AuditEntry {
        action: crate::webui::audit::AuditAction::View,
        username: state.sysop_username.clone(),
        resource: Some("system/stats".to_string()),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });

    Ok(Json(SystemStats {
        total_users,
        users_with_passwords,
        sysops,
        admins,
        moderators,
        regular_users,
        total_topics,
        total_messages,
        total_replies,
        unique_message_authors,
        messages_per_topic,
        bbs_name: "MeshBBS".to_string(), // TODO: Get from config
        bbs_location: "Unknown".to_string(), // TODO: Get from config
        uptime_seconds,
    }))
}
