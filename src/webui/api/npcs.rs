//! # NPC API Endpoints
//!
//! CRUD operations for NPCs.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::Arc;

use super::auth::AppState;

/// List NPCs response
#[derive(Debug, Serialize)]
pub struct ListNpcsResponse {
    pub npcs: Vec<NpcSummary>,
    pub total: usize,
}

/// NPC summary (for list view)
#[derive(Debug, Serialize)]
pub struct NpcSummary {
    pub id: String,
    pub name: String,
    pub npc_type: String,
    pub room_id: Option<String>,
}

/// List all NPCs
pub async fn list_npcs(
    State(_state): State<Arc<AppState>>,
) -> Response {
    // TODO: Query sled database for NPCs
    let response = ListNpcsResponse {
        npcs: vec![],
        total: 0,
    };
    
    (StatusCode::OK, Json(response)).into_response()
}

/// Get NPC by ID
pub async fn get_npc(
    State(_state): State<Arc<AppState>>,
    // TODO: Extract ID from path
) -> Response {
    // TODO: Implement
    (StatusCode::NOT_IMPLEMENTED, "Not yet implemented").into_response()
}

/// Create new NPC
pub async fn create_npc(
    State(_state): State<Arc<AppState>>,
    // TODO: Extract NPC data from body
) -> Response {
    // TODO: Implement
    (StatusCode::NOT_IMPLEMENTED, "Not yet implemented").into_response()
}

/// Update NPC
pub async fn update_npc(
    State(_state): State<Arc<AppState>>,
    // TODO: Extract ID and data
) -> Response {
    // TODO: Implement
    (StatusCode::NOT_IMPLEMENTED, "Not yet implemented").into_response()
}

/// Delete NPC
pub async fn delete_npc(
    State(_state): State<Arc<AppState>>,
    // TODO: Extract ID
) -> Response {
    // TODO: Implement
    (StatusCode::NOT_IMPLEMENTED, "Not yet implemented").into_response()
}
