//! Magic 8-Ball response management endpoint for the admin dashboard.
//!
//! Allows administrators to view and edit 8-ball responses with category support.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::bbs::eightball::{get_all_responses, save_responses, EightballResponse};

use super::auth::AppState;

/// Response payload for 8-ball list operations
#[derive(Debug, Serialize)]
pub struct EightballListResponse {
    pub responses: Vec<EightballResponse>,
    pub count: usize,
    pub categories: CategoryCounts,
}

#[derive(Debug, Serialize)]
pub struct CategoryCounts {
    pub positive: usize,
    pub neutral: usize,
    pub negative: usize,
}

/// Request payload for updating 8-ball responses
#[derive(Debug, Deserialize)]
pub struct UpdateEightballRequest {
    pub responses: Vec<EightballResponse>,
}

/// GET /api/apps/eightball/responses - Get all 8-ball responses with category breakdown
///
/// Returns responses even if the feature is disabled, allowing admins to view and edit
/// data files regardless of whether the feature is currently active.
pub async fn get_eightball_responses(State(_state): State<Arc<AppState>>) -> Response {
    match get_all_responses() {
        Some(responses) => {
            let count = responses.len();
            let positive = responses
                .iter()
                .filter(|r| r.category == "positive")
                .count();
            let neutral = responses.iter().filter(|r| r.category == "neutral").count();
            let negative = responses
                .iter()
                .filter(|r| r.category == "negative")
                .count();

            (
                StatusCode::OK,
                Json(EightballListResponse {
                    responses: responses.to_vec(),
                    count,
                    categories: CategoryCounts {
                        positive,
                        neutral,
                        negative,
                    },
                }),
            )
                .into_response()
        }
        None => {
            // If not loaded in memory, return empty list with zero counts
            // This allows creating new response files through the admin interface
            (
                StatusCode::OK,
                Json(EightballListResponse {
                    responses: vec![],
                    count: 0,
                    categories: CategoryCounts {
                        positive: 0,
                        neutral: 0,
                        negative: 0,
                    },
                }),
            )
                .into_response()
        }
    }
}

/// PUT /api/apps/eightball/responses - Update 8-ball responses
pub async fn update_eightball_responses(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<UpdateEightballRequest>,
) -> Response {
    // Validation
    if payload.responses.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Response list cannot be empty"
            })),
        )
            .into_response();
    }

    // Check for empty or whitespace-only text
    for (idx, resp) in payload.responses.iter().enumerate() {
        if resp.text.trim().is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Response at index {} has empty text", idx)
                })),
            )
                .into_response();
        }

        // Validate category
        let cat = resp.category.as_str();
        if cat != "positive" && cat != "neutral" && cat != "negative" {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!(
                        "Response at index {} has invalid category '{}'. Must be 'positive', 'neutral', or 'negative'",
                        idx, cat
                    )
                })),
            )
                .into_response();
        }
    }

    // Save to disk
    match save_responses(payload.responses.clone()) {
        Ok(_) => {
            info!(
                "[api] 8-Ball responses updated: {} responses saved",
                payload.responses.len()
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "count": payload.responses.len(),
                    "message": "8-Ball responses saved. Restart BBS to apply changes."
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("[api] Failed to save 8-ball responses: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to save responses: {}", e)
                })),
            )
                .into_response()
        }
    }
}
