//! # Web UI Server
//!
//! Axum-based web server for the admin dashboard.

use anyhow::Result;
use axum::{
    routing::{get, post, put},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing::{error, info};

use crate::config::Config;
use crate::storage::Storage;
use crate::webui::api::{
    login, logout, list_npcs, 
    list_users, get_user, update_user_level,
    list_topics, list_messages, get_topic_stats,
    get_system_stats,
    get_all_schemas, get_schema_by_type, get_roles,
    AppState
};
use crate::webui::audit::AuditLogger;
use crate::webui::auth::AuthManager;
use crate::webui::schema::SchemaRegistry;
use crate::webui::tls::TlsConfig;

/// Start the web UI server
pub async fn start_webui_server(config: Config, storage: Option<Storage>) -> Result<()> {
    let dashboard_config = &config.admin_dashboard;
    
    // Validate configuration
    dashboard_config.validate()?;
    
    if !dashboard_config.enabled {
        info!("Admin dashboard is disabled in configuration");
        return Ok(());
    }
    
    info!("Starting admin dashboard...");
    
    // Initialize audit logger
    let audit_logger = AuditLogger::new(dashboard_config, &config.storage.data_dir)?;
    
    // Initialize authentication manager
    let auth_manager = Arc::new(AuthManager::new(dashboard_config.clone()));
    
    // Get sysop credentials from BBS config
    let sysop_password_hash = config.bbs.sysop_password_hash
        .clone()
        .unwrap_or_else(|| {
            error!("No sysop password hash found in configuration!");
            String::new()
        });
    
    // Initialize schema registry
    let schema_registry = Arc::new(SchemaRegistry::new());
    
    // Create shared application state
    let app_state = Arc::new(AppState {
        auth_manager,
        audit_logger: audit_logger.clone(),
        sysop_password_hash,
        sysop_username: config.bbs.sysop.clone(),
        storage: storage.map(|s| Arc::new(Mutex::new(s))),  // Wrap storage in Arc<Mutex> for shared mutable access
        config: dashboard_config.clone(),
        schema_registry,
    });
    
    // Build router
    let app = Router::new()
        // Authentication endpoints
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        
        // Schema introspection endpoints
        .route("/api/schema", get(get_all_schemas))
        .route("/api/schema/:type", get(get_schema_by_type))
        .route("/api/roles", get(get_roles))
        
        // System statistics
        .route("/api/stats", get(get_system_stats))
        
        // User management endpoints
        .route("/api/users", get(list_users))
        .route("/api/users/:username", get(get_user))
        .route("/api/users/:username/level", put(update_user_level))
        
        // Topics and messages endpoints
        .route("/api/topics", get(list_topics))
        .route("/api/topics/:topic/messages", get(list_messages))
        .route("/api/topics/:topic/stats", get(get_topic_stats))
        
        // NPC endpoints
        .route("/api/npcs", get(list_npcs))
        
        // Static files (frontend)
        .nest_service("/", ServeDir::new("static"))
        
        // Add state
        .with_state(app_state.clone());
    
    // Setup TLS if configured
    let tls_config = TlsConfig::from_dashboard_config(dashboard_config, &config.storage.data_dir).await?;
    
    // Bind to configured addresses
    for bind_addr in &dashboard_config.bind_addresses {
        let addr: SocketAddr = bind_addr.parse()
            .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {}", bind_addr, e))?;
        
        let listener = TcpListener::bind(&addr).await
            .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;
        
        let app_clone = app.clone();
        let tls_clone = tls_config.clone();
        
        tokio::spawn(async move {
            info!("Admin dashboard listening on {} (TLS: {})", 
                addr, 
                if tls_clone.is_some() { "enabled" } else { "disabled" }
            );
            
            if let Some(_tls_cfg) = tls_clone {
                // TODO: Implement TLS acceptor
                // For now, just run HTTP
                info!("TLS support not yet fully implemented, using HTTP for now");
                if let Err(e) = axum::serve(listener, app_clone).await {
                    error!("Server error on {}: {}", addr, e);
                }
            } else {
                if let Err(e) = axum::serve(listener, app_clone).await {
                    error!("Server error on {}: {}", addr, e);
                }
            }
        });
    }
    
    // Spawn session cleanup task
    let auth_mgr = app_state.auth_manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // Every 5 minutes
        loop {
            interval.tick().await;
            auth_mgr.cleanup_expired_sessions().await;
        }
    });
    
    Ok(())
}
