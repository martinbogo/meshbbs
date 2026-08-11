//! Fortune cookie statistics and management endpoint for the admin dashboard.
//!
//! Exposes metadata about the fortune database loaded from JSON,
//! and allows administrators to view/edit fortune content.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::bbs::fortune::{fortune_count, get_all_fortunes, is_available, save_fortunes};

use super::auth::AppState;

/// Response payload summarising the fortune database.
#[derive(Debug, Serialize, PartialEq)]
pub struct FortuneStatsResponse {
    pub total: usize,
    pub max_length: usize,
    pub average_length: f32,
    pub sample: Vec<String>,
    pub available: bool,
}

/// Return aggregate statistics for the fortune cookie module.
pub async fn get_fortune_stats(State(_state): State<Arc<AppState>>) -> Response {
    (StatusCode::OK, Json(compute_fortune_stats())).into_response()
}

fn compute_fortune_stats() -> FortuneStatsResponse {
    let available = is_available();

    if !available {
        return FortuneStatsResponse {
            total: 0,
            max_length: 0,
            average_length: 0.0,
            sample: vec![],
            available: false,
        };
    }

    let entries = get_all_fortunes().unwrap_or_default();
    let total = fortune_count();
    let max_length = entries.iter().map(|f| f.chars().count()).max().unwrap_or(0);

    let cumulative_len: usize = entries.iter().map(|fortune| fortune.chars().count()).sum();
    let average_length = if total > 0 {
        cumulative_len as f32 / total as f32
    } else {
        0.0
    };

    let sample: Vec<String> = entries.iter().take(5).cloned().collect();

    FortuneStatsResponse {
        total,
        max_length,
        average_length,
        sample,
        available,
    }
}

/// Request payload for updating fortune responses
#[derive(Debug, Deserialize)]
pub struct UpdateFortunesRequest {
    pub fortunes: Vec<String>,
}

/// Response payload for fortune list operations
#[derive(Debug, Serialize)]
pub struct FortuneListResponse {
    pub fortunes: Vec<String>,
    pub count: usize,
}

/// GET /api/apps/fortune/responses - Get all fortune responses
///
/// Returns fortunes even if the feature is disabled, allowing admins to view and edit
/// data files regardless of whether the feature is currently active.
pub async fn get_fortune_responses(State(_state): State<Arc<AppState>>) -> Response {
    match get_all_fortunes() {
        Some(fortunes) => {
            let count = fortunes.len();
            (
                StatusCode::OK,
                Json(FortuneListResponse { fortunes, count }),
            )
                .into_response()
        }
        None => {
            // If not loaded in memory, return empty list (file might not exist yet)
            // This allows creating new fortune files through the admin interface
            (
                StatusCode::OK,
                Json(FortuneListResponse {
                    fortunes: vec![],
                    count: 0,
                }),
            )
                .into_response()
        }
    }
}

/// PUT /api/apps/fortune/responses - Update fortune responses
pub async fn update_fortune_responses(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateFortunesRequest>,
) -> Response {
    // Validation
    if payload.fortunes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Fortune list cannot be empty"
            })),
        )
            .into_response();
    }

    // Check for empty or whitespace-only fortunes
    for (idx, fortune) in payload.fortunes.iter().enumerate() {
        if fortune.trim().is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Fortune at index {} is empty or whitespace-only", idx)
                })),
            )
                .into_response();
        }
    }

    // Save to disk
    let data_dir = &state.data_dir;
    match save_fortunes(data_dir, payload.fortunes.clone()) {
        Ok(_) => {
            info!(
                "[api] Fortune responses updated: {} fortunes saved",
                payload.fortunes.len()
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "count": payload.fortunes.len(),
                    "message": "Fortune responses saved. Restart BBS to apply changes."
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("[api] Failed to save fortune responses: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to save fortunes: {}", e)
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::compute_fortune_stats;

    #[test]
    fn fortune_stats_reports_expected_counts() {
        let stats = compute_fortune_stats();
        assert_eq!(stats.total, crate::bbs::fortune::fortune_count());
        assert!(stats.max_length > 0);
        assert!(!stats.sample.is_empty());
        assert!(stats.sample.len() <= 5);
        assert!(stats.average_length > 0.0);
    }
}
