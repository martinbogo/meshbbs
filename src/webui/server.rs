//! # Web UI Server
//!
//! Axum-based web server for the admin dashboard.

use anyhow::Result;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tower_http::services::ServeDir;
use tracing::{error, info};

use crate::config::{AppsConfig, Config};
use crate::storage::Storage;
use crate::tmush::errors::TinyMushError;
use crate::tmush::storage::TinyMushStore;
use crate::webui::api::{
    create_collection_item, delete_collection_item, delete_message, get_activity_feed,
    get_all_schemas, get_apps, get_audit_logs, get_collection, get_collection_item,
    get_eightball_responses, get_fortune_responses, get_fortune_stats, get_roles,
    get_schema_by_type, get_status, get_system_stats, get_tinymush_status, get_topic_stats,
    get_user, get_world_overview, list_messages, list_npcs, list_topics, list_users, login, logout,
    restart_server, toggle_app_config, toggle_pin_message, update_collection_item,
    update_eightball_responses, update_fortune_responses, update_message_title, update_user_level,
    AppState,
};
use crate::webui::audit::AuditLogger;
use crate::webui::auth::AuthManager;
use crate::webui::schema::SchemaRegistry;
use crate::webui::tls::TlsConfig;

/// Start the web UI server
pub async fn start_webui_server(
    config: Config,
    storage: Option<Storage>,
    shared_tinymush_store: Option<TinyMushStore>,
) -> Result<()> {
    let dashboard_config = &config.admin_dashboard;

    dashboard_config.validate()?;

    if !dashboard_config.enabled {
        info!("Admin dashboard is disabled in configuration");
        return Ok(());
    }

    info!("Starting admin dashboard...");

    let audit_logger = AuditLogger::new(dashboard_config, &config.storage.data_dir)?;
    let auth_manager = Arc::new(AuthManager::new(dashboard_config.clone()));

    let sysop_password_hash = config.bbs.sysop_password_hash.clone().unwrap_or_else(|| {
        error!("No sysop password hash found in configuration!");
        String::new()
    });

    let schema_registry = Arc::new(SchemaRegistry::new());
    let runtime_config_path = config.admin_dashboard.runtime_config_path.clone();
    let apps_config = config.apps.clone();
    let games_state = Arc::new(RwLock::new(apps_config.clone()));
    let data_dir = PathBuf::from(&config.storage.data_dir);
    let tinymush_db_path = apps_config
        .tinymush
        .db_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("tinymush"));

    let (tinymush_store, tinymush_store_error) =
        resolve_tinymush_store(shared_tinymush_store, &apps_config, &tinymush_db_path);

    let app_state = Arc::new(AppState {
        auth_manager,
        audit_logger: audit_logger.clone(),
        sysop_password_hash,
        sysop_username: config.bbs.sysop.clone(),
        storage: storage.map(|s| Arc::new(Mutex::new(s))),
        config: dashboard_config.clone(),
        schema_registry,
        data_dir: data_dir.clone(),
        config_path: runtime_config_path.map(PathBuf::from),
        games: games_state,
        tinymush_store,
        tinymush_store_error,
        tinymush_db_path: tinymush_db_path.clone(),
    });

    let app = Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/schema", get(get_all_schemas))
        .route("/api/schema/:type", get(get_schema_by_type))
        .route("/api/roles", get(get_roles))
        .route("/api/stats", get(get_system_stats))
        .route("/api/system/restart", post(restart_server))
        .route("/api/system/status", get(get_status))
        .route("/api/apps", get(get_apps))
        .route("/api/apps/config-toggle", post(toggle_app_config))
        .route("/api/users", get(list_users))
        .route("/api/users/:username", get(get_user))
        .route("/api/users/:username/level", put(update_user_level))
        .route("/api/topics", get(list_topics))
        .route("/api/topics/:topic/messages", get(list_messages))
        .route("/api/topics/:topic/stats", get(get_topic_stats))
        .route("/api/topics/:topic/messages/:id", delete(delete_message))
        .route(
            "/api/topics/:topic/messages/:id/pin",
            put(toggle_pin_message),
        )
        .route(
            "/api/topics/:topic/messages/:id/title",
            put(update_message_title),
        )
        .route("/api/audit/logs", get(get_audit_logs))
        .route("/api/activity/feed", get(get_activity_feed))
        .route("/api/npcs", get(list_npcs))
        .route("/api/world/overview", get(get_world_overview))
        .route("/api/tinymush/status", get(get_tinymush_status))
        .route(
            "/api/tinymush/collections/:collection",
            get(get_collection).post(create_collection_item),
        )
        .route(
            "/api/tinymush/collections/:collection/:id",
            get(get_collection_item)
                .put(update_collection_item)
                .delete(delete_collection_item),
        )
        .route("/api/fortune/stats", get(get_fortune_stats))
        .route(
            "/api/apps/fortune/responses",
            get(get_fortune_responses).put(update_fortune_responses),
        )
        .route(
            "/api/apps/eightball/responses",
            get(get_eightball_responses).put(update_eightball_responses),
        )
        .nest_service("/", ServeDir::new("static"))
        .with_state(app_state.clone());

    let tls_config =
        TlsConfig::from_dashboard_config(dashboard_config, &config.storage.data_dir).await?;

    for bind_addr in &dashboard_config.bind_addresses {
        let addr: SocketAddr = bind_addr
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {}", bind_addr, e))?;

        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

        let app_clone = app.clone();
        let tls_clone = tls_config.clone();

        tokio::spawn(async move {
            info!(
                "Admin dashboard listening on {} (TLS: {})",
                addr,
                if tls_clone.is_some() {
                    "enabled"
                } else {
                    "disabled"
                }
            );

            if let Some(_tls_cfg) = tls_clone {
                info!("TLS support not yet fully implemented, using HTTP for now");
                if let Err(e) = axum::serve(listener, app_clone).await {
                    error!("Server error on {}: {}", addr, e);
                }
            } else if let Err(e) = axum::serve(listener, app_clone).await {
                error!("Server error on {}: {}", addr, e);
            }
        });
    }

    let auth_mgr = app_state.auth_manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            auth_mgr.cleanup_expired_sessions().await;
        }
    });

    Ok(())
}

fn resolve_tinymush_store(
    shared: Option<TinyMushStore>,
    apps_config: &AppsConfig,
    tinymush_db_path: &PathBuf,
) -> (Option<Arc<TinyMushStore>>, Option<String>) {
    if let Some(store) = shared {
        info!("Using TinyMUSH store shared by the running BBS");
        return (Some(Arc::new(store)), None);
    }

    if !apps_config.tinymush.enabled {
        return (
            None,
            Some("TinyMUSH is disabled in configuration".to_string()),
        );
    }

    if !tinymush_db_path.as_path().exists() {
        return (
            None,
            Some(format!(
                "TinyMUSH database not initialized at {}",
                tinymush_db_path.display()
            )),
        );
    }

    match TinyMushStore::open(tinymush_db_path.as_path()) {
        Ok(store) => (Some(Arc::new(store)), None),
        Err(err) => {
            error!(
                target: "webui::tinymush",
                "Failed to open TinyMUSH store at {}: {}",
                tinymush_db_path.display(),
                err
            );
            let friendly = describe_tinymush_open_error(tinymush_db_path.as_path(), &err);
            (None, Some(friendly))
        }
    }
}

fn describe_tinymush_open_error(path: &Path, err: &TinyMushError) -> String {
    match err {
        TinyMushError::Sled(sled_err) => {
            let message = sled_err.to_string();
            if message.contains("could not acquire lock") {
                return format!(
                    "TinyMUSH database at {} is currently locked by another process. Shut down the other process or remove the stale lock file before restarting the admin dashboard.",
                    path.display()
                );
            }
            format!(
                "Unable to open TinyMUSH database at {}: {}",
                path.display(),
                message
            )
        }
        TinyMushError::Io(io_err) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
            format!(
                "TinyMUSH database at {} is currently locked by another process. Shut down the other process or remove the stale lock before retrying.",
                path.display()
            )
        }
        _ => format!(
            "Unable to open TinyMUSH database at {}: {}",
            path.display(),
            err
        ),
    }
}
