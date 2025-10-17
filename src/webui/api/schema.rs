//! Schema introspection API
//!
//! Provides endpoints for clients to discover entity schemas, field definitions,
//! validation rules, and role configurations dynamically.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::RoleDefinition;
use crate::webui::api::AppState;
use crate::webui::audit::{AuditAction, AuditEntry};
use crate::webui::schema::SchemaDefinition;

/// Complete schema information response
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaResponse {
    /// All available entity schemas
    pub schemas: Vec<SchemaDefinition>,
    /// Role definitions
    pub roles: Vec<RoleDefinition>,
}

/// Single schema response
#[derive(Debug, Serialize, Deserialize)]
pub struct SingleSchemaResponse {
    /// The requested schema
    pub schema: SchemaDefinition,
    /// Role definitions (for context)
    pub roles: Vec<RoleDefinition>,
}

/// GET /api/schema - Get all entity schemas and role definitions
pub async fn get_all_schemas(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SchemaResponse>, StatusCode> {
    // Get all schemas from registry
    let schemas = state.schema_registry.get_all_schemas()
        .into_iter()
        .cloned()
        .collect();
    
    // Get role definitions from config
    let roles = state.config.roles.clone();
    
    // Log audit entry
    state.audit_logger.log(AuditEntry {
        action: AuditAction::View,
        username: state.sysop_username.clone(),
        resource: Some("schema/all".to_string()),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });
    
    Ok(Json(SchemaResponse { schemas, roles }))
}

/// GET /api/schema/:type - Get schema for a specific entity type
pub async fn get_schema_by_type(
    State(state): State<Arc<AppState>>,
    Path(entity_type): Path<String>,
) -> Result<Json<SingleSchemaResponse>, StatusCode> {
    // Get schema from registry
    let schema = state.schema_registry.get_schema(&entity_type)
        .ok_or_else(|| {
            eprintln!("Schema not found for entity type: {}", entity_type);
            StatusCode::NOT_FOUND
        })?
        .clone();
    
    // Get role definitions from config
    let roles = state.config.roles.clone();
    
    // Log audit entry
    state.audit_logger.log(AuditEntry {
        action: AuditAction::View,
        username: state.sysop_username.clone(),
        resource: Some(format!("schema/{}", entity_type)),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });
    
    Ok(Json(SingleSchemaResponse { schema, roles }))
}

/// GET /api/roles - Get role definitions
pub async fn get_roles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<RoleDefinition>>, StatusCode> {
    let roles = state.config.roles.clone();
    
    // Log audit entry
    state.audit_logger.log(AuditEntry {
        action: AuditAction::View,
        username: state.sysop_username.clone(),
        resource: Some("roles".to_string()),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });
    
    Ok(Json(roles))
}
