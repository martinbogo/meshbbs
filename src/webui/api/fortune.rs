//! Fortune cookie statistics endpoint for the admin dashboard.
//!
//! Exposes metadata about the built-in fortune database so administrators
//! can verify content quality without dumping all 400 entries to the UI.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::bbs::fortune::{fortune_count, fortunes, max_fortune_length};

use super::auth::AppState;

/// Response payload summarising the fortune database.
#[derive(Debug, Serialize, PartialEq)]
pub struct FortuneStatsResponse {
    pub total: usize,
    pub max_length: usize,
    pub average_length: f32,
    pub sample: Vec<String>,
}

/// Return aggregate statistics for the fortune cookie module.
pub async fn get_fortune_stats(State(_state): State<Arc<AppState>>) -> Response {
    (StatusCode::OK, Json(compute_fortune_stats())).into_response()
}

fn compute_fortune_stats() -> FortuneStatsResponse {
    let entries = fortunes();
    let total = fortune_count();
    let max_length = max_fortune_length();

    let cumulative_len: usize = entries.iter().map(|fortune| fortune.chars().count()).sum();
    let average_length = if total > 0 {
        cumulative_len as f32 / total as f32
    } else {
        0.0
    };

    let sample = entries.iter().take(5).map(|s| (*s).to_string()).collect();

    FortuneStatsResponse {
        total,
        max_length,
        average_length,
        sample,
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
