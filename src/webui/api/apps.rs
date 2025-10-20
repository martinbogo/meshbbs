use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::error;

use crate::bbs::fortune::fortune_count;
use crate::config::{Config, GamesConfig};
use crate::webui::audit::{AuditAction, AuditEntry};

use super::auth::AppState;
use super::ErrorResponse;

/// Return the list of available apps with metadata for the admin dashboard.
pub async fn get_apps(State(state): State<Arc<AppState>>) -> Response {
    match build_apps_response(&state).await {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(err) => {
            error!(target: "webui::apps", "Failed to load apps manifest: {err:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to load apps".to_string(),
                }),
            )
                .into_response()
        }
    }
}

async fn build_apps_response(state: &AppState) -> anyhow::Result<AppsResponse> {
    let games = state.games.read().await.clone();
    let (mut apps, mut source) = load_apps(&state.data_dir, &games).await?;

    apps.iter_mut()
        .for_each(|app| apply_runtime_overrides(app, &games));
    augment_runtime_metrics(&mut apps);

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    if source.description.is_none() {
        source.description = Some(match source.kind.as_str() {
            "manifest" => "Apps loaded from data/apps.json".to_string(),
            _ => "Apps derived from configuration defaults".to_string(),
        });
    }

    Ok(AppsResponse {
        generated_at: Utc::now().to_rfc3339(),
        source,
        apps,
    })
}

#[derive(Debug, Deserialize)]
pub struct ConfigToggleRequest {
    pub app_id: String,
    pub target: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfigToggleResponse {
    pub app: AppDescriptor,
}

/// Toggle a configuration-backed app flag (e.g., games.tinymush_enabled).
pub async fn toggle_app_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigToggleRequest>,
) -> Response {
    let Some(config_path) = state.config_path.clone() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Configuration path unavailable".to_string(),
            }),
        )
            .into_response();
    };

    // Reload config from disk to ensure we operate on the latest version.
    let config_path_string = config_path.to_string_lossy().into_owned();
    let mut config = match Config::load(&config_path_string).await {
        Ok(cfg) => cfg,
        Err(err) => {
            tracing::error!(target = "webui::apps", "Config reload failed: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to reload configuration".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Apply the requested change.
    let mut handled = false;
    match req.target.as_str() {
        "games.tinyhack_enabled" => {
            config.games.tinyhack_enabled = req.enabled;
            handled = true;
        }
        "games.tinymush_enabled" => {
            config.games.tinymush_enabled = req.enabled;
            handled = true;
        }
        _ => {}
    }

    if !handled {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Unsupported configuration target".to_string(),
            }),
        )
            .into_response();
    }

    // Persist configuration.
    let serialized = match toml::to_string_pretty(&config) {
        Ok(content) => content,
        Err(err) => {
            tracing::error!(target = "webui::apps", "Failed to serialize config: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to serialize configuration".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(err) = fs::write(&config_path, serialized).await {
        tracing::error!(target = "webui::apps", "Failed to write config: {err}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to update configuration".to_string(),
            }),
        )
            .into_response();
    }

    // Refresh in-memory state.
    {
        let mut games = state.games.write().await;
        *games = config.games.clone();
    }

    // Log audit entry.
    state.audit_logger.log(AuditEntry {
        action: AuditAction::ConfigChange,
        username: state.sysop_username.clone(),
        resource: Some(format!("config/{}", req.target)),
        ip_address: "webui".to_string(),
        session_token: "webui_session".to_string(),
        status: "success".to_string(),
        reason: None,
    });

    // Build updated response and extract the requested app descriptor.
    match build_apps_response(&state).await {
        Ok(payload) => {
            if let Some(app) = payload
                .apps
                .into_iter()
                .find(|entry| entry.id == req.app_id)
            {
                return (StatusCode::OK, Json(ConfigToggleResponse { app })).into_response();
            }

            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "App not found after update".to_string(),
                }),
            )
                .into_response()
        }
        Err(err) => {
            tracing::error!(
                target = "webui::apps",
                "Failed to rebuild apps payload: {err:?}"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to refresh app data".to_string(),
                }),
            )
                .into_response()
        }
    }
}

async fn load_apps(
    data_dir: &Path,
    games: &GamesConfig,
) -> anyhow::Result<(Vec<AppDescriptor>, AppSource)> {
    let manifest_path = data_dir.join("apps.json");
    match fs::read_to_string(&manifest_path).await {
        Ok(raw) => {
            let manifest: AppManifest = serde_json::from_str(&raw)?;
            let apps = manifest
                .apps
                .into_iter()
                .map(AppManifestEntry::into_descriptor)
                .collect::<Vec<_>>();

            Ok((
                apps,
                AppSource {
                    kind: "manifest".to_string(),
                    path: Some(path_to_string(&manifest_path)),
                    description: None,
                },
            ))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok((
            fallback_apps(games),
            AppSource {
                kind: "fallback".to_string(),
                path: None,
                description: None,
            },
        )),
        Err(err) => Err(err.into()),
    }
}

fn apply_runtime_overrides(app: &mut AppDescriptor, games: &GamesConfig) {
    match app.id.as_str() {
        "tinyhack" => {
            app.enabled = games.tinyhack_enabled;
            ensure_config_key(app, "games.tinyhack_enabled");
            if !app.enabled && !app.planned {
                app.status = "offline".to_string();
                let mut note = format!(
                    "Disabled in config (set {} = true).",
                    "games.tinyhack_enabled"
                );
                if let Some(existing) = app.status_detail.as_ref() {
                    if !existing.is_empty() {
                        note.push(' ');
                        note.push_str(existing);
                    }
                }
                app.status_detail = Some(note);
            }
        }
        "tinymush" => {
            app.enabled = games.tinymush_enabled;
            ensure_config_key(app, "games.tinymush_enabled");
            if !app.enabled && !app.planned {
                app.status = "offline".to_string();
                let mut note = format!(
                    "Disabled in config (set {} = true).",
                    "games.tinymush_enabled"
                );
                if let Some(existing) = app.status_detail.as_ref() {
                    if !existing.is_empty() {
                        note.push(' ');
                        note.push_str(existing);
                    }
                }
                app.status_detail = Some(note);
            }
        }
        _ => {
            if app.planned {
                app.enabled = false;
                if app.status.is_empty() || app.status == "unknown" {
                    app.status = "planned".to_string();
                }
            }
        }
    }
}

fn ensure_config_key(app: &mut AppDescriptor, key: &str) {
    if !app.config_keys.iter().any(|existing| existing == key) {
        app.config_keys.push(key.to_string());
    }
}

fn augment_runtime_metrics(apps: &mut [AppDescriptor]) {
    if let Some(fortune_app) = apps.iter_mut().find(|entry| entry.id == "fortune") {
        let metrics = fortune_app.metrics.get_or_insert_with(AppMetrics::default);
        metrics.sessions_total = Some(fortune_count() as u64);
        if metrics.sessions_7d.is_none() {
            metrics.sessions_7d = Some(0);
        }
        metrics
            .last_activity
            .get_or_insert_with(|| "Always available (local dataset)".to_string());
    }
}

fn fallback_apps(games: &GamesConfig) -> Vec<AppDescriptor> {
    let mut entries = vec![AppDescriptor {
        id: "fortune".to_string(),
        name: "Fortune Teller".to_string(),
        category: "Utility".to_string(),
        description: "Broadcast classic Unix fortunes to the public channel.".to_string(),
        summary: Some("Broadcast Unix fortunes over the public channel.".to_string()),
        status: "online".to_string(),
        status_detail: Some("Active by default".to_string()),
        enabled: true,
        planned: false,
        tags: vec![],
        commands: vec![AppCommand {
            syntax: Some("^FORTUNE".to_string()),
            channel: Some("Public".to_string()),
            description: Some(
                "Broadcast a random fortune using the configured prefix.".to_string(),
            ),
            display: None,
        }],
        data_paths: vec![],
        metrics: None,
        config_keys: vec![],
        actions: vec![],
        notes: None,
    }];

    entries.push(AppDescriptor {
        id: "tinyhack".to_string(),
        name: "TinyHack".to_string(),
        category: "Games".to_string(),
        description: "Turn-based ASCII roguelike delivered via direct messages.".to_string(),
        summary: Some("Launch from the in-game Games menu.".to_string()),
        status: if games.tinyhack_enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via games.tinyhack_enabled".to_string()),
        enabled: games.tinyhack_enabled,
        planned: false,
        tags: vec![],
        commands: vec![AppCommand {
            syntax: None,
            channel: Some("Games menu".to_string()),
            description: Some(
                "Select TinyHack from the Games list to start a session.".to_string(),
            ),
            display: Some("Games menu → TinyHack".to_string()),
        }],
        data_paths: vec![],
        metrics: None,
        config_keys: vec!["games.tinyhack_enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle TinyHack".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("games.tinyhack_enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: None,
    });

    entries.push(AppDescriptor {
        id: "tinymush".to_string(),
        name: "TinyMUSH".to_string(),
        category: "Games".to_string(),
        description: "Persistent TinyMUSH experience with seed-driven content.".to_string(),
        summary: Some("Enable to expose TinyMUSH through the Games menu.".to_string()),
        status: if games.tinymush_enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via games.tinymush_enabled".to_string()),
        enabled: games.tinymush_enabled,
        planned: false,
        tags: vec![],
        commands: vec![AppCommand {
            syntax: None,
            channel: Some("Games menu".to_string()),
            description: Some("Select TinyMUSH from the Games list when enabled.".to_string()),
            display: Some("Games menu → TinyMUSH".to_string()),
        }],
        data_paths: vec![],
        metrics: None,
        config_keys: vec!["games.tinymush_enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle TinyMUSH".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("games.tinymush_enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: None,
    });

    entries
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().trim().to_string()
}

#[derive(Debug, Serialize)]
pub struct AppsResponse {
    pub generated_at: String,
    pub source: AppSource,
    pub apps: Vec<AppDescriptor>,
}

#[derive(Debug, Serialize)]
pub struct AppSource {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppDescriptor {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub planned: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<AppCommand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<AppMetrics>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AppAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Default for AppDescriptor {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            category: "App".to_string(),
            description: String::new(),
            summary: None,
            status: "unknown".to_string(),
            status_detail: None,
            enabled: false,
            planned: false,
            tags: Vec::new(),
            commands: Vec::new(),
            data_paths: Vec::new(),
            metrics: None,
            config_keys: Vec::new(),
            actions: Vec::new(),
            notes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppCommand {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions_7d: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppAction {
    pub label: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub primary: bool,
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
struct AppManifest {
    apps: Vec<AppManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct AppManifestEntry {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    status_detail: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    planned: Option<bool>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    commands: Vec<AppCommand>,
    #[serde(default)]
    data_paths: Vec<String>,
    #[serde(default)]
    metrics: Option<AppMetrics>,
    #[serde(default)]
    config_keys: Vec<String>,
    #[serde(default)]
    actions: Vec<AppAction>,
    #[serde(default)]
    notes: Option<String>,
}

impl AppManifestEntry {
    fn into_descriptor(self) -> AppDescriptor {
        AppDescriptor {
            id: self.id,
            name: self.name.unwrap_or_else(|| "Unnamed App".to_string()),
            category: self.category.unwrap_or_else(|| "App".to_string()),
            description: self
                .description
                .unwrap_or_else(|| "No description available.".to_string()),
            summary: self.summary,
            status: self.status.unwrap_or_else(|| "unknown".to_string()),
            status_detail: self.status_detail,
            enabled: self.enabled.unwrap_or(!self.planned.unwrap_or(false)),
            planned: self.planned.unwrap_or(false),
            tags: self.tags,
            commands: self.commands,
            data_paths: self.data_paths,
            metrics: self.metrics,
            config_keys: self.config_keys,
            actions: self.actions,
            notes: self.notes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_app(id: &str) -> AppDescriptor {
        AppDescriptor {
            id: id.to_string(),
            name: "Stub".to_string(),
            category: "App".to_string(),
            description: "Stub".to_string(),
            summary: None,
            status: "online".to_string(),
            status_detail: None,
            enabled: true,
            planned: false,
            tags: Vec::new(),
            commands: Vec::new(),
            data_paths: Vec::new(),
            metrics: None,
            config_keys: Vec::new(),
            actions: Vec::new(),
            notes: None,
        }
    }

    #[tokio::test]
    async fn load_apps_uses_manifest_when_present() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest_path = temp_dir.path().join("apps.json");

        let manifest = r#"{"apps":[{"id":"fortune","name":"Fortune"}]}"#;
        fs::write(&manifest_path, manifest)
            .await
            .expect("write manifest");

        let games = GamesConfig::default();
        let (apps, source) = load_apps(temp_dir.path(), &games).await.expect("load apps");

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "fortune");
        assert_eq!(source.kind, "manifest");
        assert!(source.path.is_some());
    }

    #[test]
    fn apply_runtime_overrides_respects_games_config() {
        let mut app = stub_app("tinyhack");
        let mut games = GamesConfig::default();
        games.tinyhack_enabled = false;

        apply_runtime_overrides(&mut app, &games);

        assert!(!app.enabled);
        assert_eq!(app.status, "offline");
        assert!(app
            .status_detail
            .as_ref()
            .expect("detail")
            .contains("games.tinyhack_enabled"));
        assert!(app
            .config_keys
            .iter()
            .any(|entry| entry == "games.tinyhack_enabled"));
    }

    #[test]
    fn augment_runtime_metrics_populates_fortune() {
        let mut apps = vec![stub_app("fortune")];
        augment_runtime_metrics(&mut apps);

        let fortune = apps
            .iter()
            .find(|entry| entry.id == "fortune")
            .expect("fortune present");
        assert!(fortune.metrics.is_some());
        let metrics = fortune.metrics.as_ref().unwrap();
        assert!(metrics.sessions_total.unwrap_or(0) > 0);
        assert!(metrics.sessions_7d.is_some());
        assert!(metrics.last_activity.is_some());
    }
}
