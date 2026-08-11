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
use crate::config::{AppsConfig, Config};
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
    let apps_config = state.games.read().await.clone();

    // Generate apps directly from config - no external JSON needed
    let mut apps = build_apps_from_config(&apps_config);

    augment_runtime_metrics(&mut apps);
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let source = AppSource {
        kind: "config".to_string(),
        path: state
            .config_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        description: Some("Apps discovered from [apps.*] configuration structure".to_string()),
    };

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
        "apps.fortune.enabled" => {
            config.apps.fortune.enabled = req.enabled;
            handled = true;
        }
        "apps.eightball.enabled" => {
            config.apps.eightball.enabled = req.enabled;
            handled = true;
        }
        "apps.slotmachine.enabled" => {
            config.apps.slotmachine.enabled = req.enabled;
            handled = true;
        }
        "apps.weather.enabled" => {
            config.apps.weather.enabled = req.enabled;
            handled = true;
        }
        "apps.tinyhack.enabled" => {
            config.apps.tinyhack.enabled = req.enabled;
            handled = true;
        }
        "apps.tinymush.enabled" => {
            config.apps.tinymush.enabled = req.enabled;
            handled = true;
        }
        "apps.ident_beacon.enabled" => {
            config.apps.ident_beacon.enabled = req.enabled;
            handled = true;
        }
        "apps.welcome.enabled" => {
            config.apps.welcome.enabled = req.enabled;
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
        *games = config.apps.clone();
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

// Removed: load_apps(), apply_runtime_overrides(), ensure_config_key()
// Apps are now built directly from config - single source of truth

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

/// Build the complete list of apps by introspecting the AppsConfig structure.
/// This is the single source of truth - no external JSON files needed.
fn build_apps_from_config(apps_config: &AppsConfig) -> Vec<AppDescriptor> {
    let mut entries = vec![
        // Fortune Teller
        AppDescriptor {
            id: "fortune".to_string(),
            name: "Fortune Teller".to_string(),
            category: "Utility".to_string(),
            description: "Broadcast classic Unix fortunes to the public channel.".to_string(),
            summary: Some("Broadcast Unix fortunes over the public channel.".to_string()),
            status: if apps_config.fortune.enabled {
                "online".to_string()
            } else {
                "offline".to_string()
            },
            status_detail: Some("Toggle via apps.fortune.enabled".to_string()),
            enabled: apps_config.fortune.enabled,
            planned: false,
            tags: vec!["wisdom".to_string(), "entertainment".to_string()],
            commands: vec![AppCommand {
                syntax: Some("^FORTUNE".to_string()),
                channel: Some("Public".to_string()),
                description: Some(
                    "Broadcast a random fortune using the configured prefix.".to_string(),
                ),
                display: Some("FORTUNE".to_string()),
            }],
            data_paths: vec!["data/fortunes.json".to_string()],
            metrics: None,
            config_keys: vec!["apps.fortune.enabled".to_string()],
            actions: vec![AppAction {
                label: "Toggle Fortune".to_string(),
                kind: Some("config-toggle".to_string()),
                target: Some("apps.fortune.enabled".to_string()),
                endpoint: None,
                method: None,
                primary: true,
                disabled: false,
            }],
            notes: None,
        },
    ];

    entries.push(AppDescriptor {
        id: "tinyhack".to_string(),
        name: "TinyHack".to_string(),
        category: "Games".to_string(),
        description: "Turn-based ASCII roguelike delivered via direct messages.".to_string(),
        summary: Some("Launch from the in-game Games menu.".to_string()),
        status: if apps_config.tinyhack.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.tinyhack.enabled".to_string()),
        enabled: apps_config.tinyhack.enabled,
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
        config_keys: vec!["apps.tinyhack.enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle TinyHack".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.tinyhack.enabled".to_string()),
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
        status: if apps_config.tinymush.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.tinymush.enabled".to_string()),
        enabled: apps_config.tinymush.enabled,
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
        config_keys: vec!["apps.tinymush.enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle TinyMUSH".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.tinymush.enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: None,
    });

    // 8-Ball
    entries.push(AppDescriptor {
        id: "eightball".to_string(),
        name: "Magic 8-Ball".to_string(),
        category: "Games".to_string(),
        description: "Ask a yes/no question and receive mystical guidance from the Magic 8-Ball."
            .to_string(),
        summary: Some("Responds to questions with classic 8-ball answers.".to_string()),
        status: if apps_config.eightball.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.eightball.enabled".to_string()),
        enabled: apps_config.eightball.enabled,
        planned: false,
        tags: vec!["fun".to_string(), "divination".to_string()],
        commands: vec![AppCommand {
            syntax: Some("^8BALL <question>".to_string()),
            channel: Some("Public".to_string()),
            description: Some("Ask a yes/no question and receive an 8-ball response.".to_string()),
            display: Some("8BALL <your question>".to_string()),
        }],
        data_paths: vec!["data/8ball_responses.json".to_string()],
        metrics: None,
        config_keys: vec!["apps.eightball.enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle 8-Ball".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.eightball.enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: None,
    });

    // Slot Machine
    entries.push(AppDescriptor {
        id: "slotmachine".to_string(),
        name: "Slot Machine".to_string(),
        category: "Games".to_string(),
        description:
            "Try your luck with a virtual slot machine. Spin the reels for a chance to win!"
                .to_string(),
        summary: Some("Spin the slots and test your fortune.".to_string()),
        status: if apps_config.slotmachine.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.slotmachine.enabled".to_string()),
        enabled: apps_config.slotmachine.enabled,
        planned: false,
        tags: vec!["casino".to_string(), "gambling".to_string()],
        commands: vec![AppCommand {
            syntax: Some("^SLOTS".to_string()),
            channel: Some("Public".to_string()),
            description: Some("Pull the lever and spin the slot machine reels.".to_string()),
            display: Some("SLOTS".to_string()),
        }],
        data_paths: vec![],
        metrics: None,
        config_keys: vec!["apps.slotmachine.enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle Slots".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.slotmachine.enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: None,
    });

    // Weather
    entries.push(AppDescriptor {
        id: "weather".to_string(),
        name: "Weather Reports".to_string(),
        category: "Utility".to_string(),
        description: "Get current weather conditions and forecasts for configured locations."
            .to_string(),
        summary: Some(format!(
            "Location: {}",
            apps_config.weather.default_location
        )),
        status: if apps_config.weather.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.weather.enabled".to_string()),
        enabled: apps_config.weather.enabled,
        planned: false,
        tags: vec!["information".to_string(), "api".to_string()],
        commands: vec![AppCommand {
            syntax: Some("^WEATHER".to_string()),
            channel: Some("Public".to_string()),
            description: Some("Get current weather for the configured location.".to_string()),
            display: Some("WEATHER".to_string()),
        }],
        data_paths: vec![],
        metrics: None,
        config_keys: vec![
            "apps.weather.enabled".to_string(),
            "apps.weather.location".to_string(),
            "apps.weather.api_key".to_string(),
        ],
        actions: vec![AppAction {
            label: "Toggle Weather".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.weather.enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: Some("Requires valid OpenWeatherMap API key in configuration.".to_string()),
    });

    // Ident Beacon
    entries.push(AppDescriptor {
        id: "ident_beacon".to_string(),
        name: "Identity Beacon".to_string(),
        category: "System".to_string(),
        description:
            "Periodically broadcasts BBS identification and status information to the mesh."
                .to_string(),
        summary: Some(format!(
            "Broadcasts every {} seconds",
            apps_config.ident_beacon.frequency
        )),
        status: if apps_config.ident_beacon.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.ident_beacon.enabled".to_string()),
        enabled: apps_config.ident_beacon.enabled,
        planned: false,
        tags: vec!["broadcast".to_string(), "system".to_string()],
        commands: vec![],
        data_paths: vec![],
        metrics: None,
        config_keys: vec![
            "apps.ident_beacon.enabled".to_string(),
            "apps.ident_beacon.frequency".to_string(),
        ],
        actions: vec![AppAction {
            label: "Toggle Beacon".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.ident_beacon.enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: Some("Helps other nodes discover this BBS on the mesh network.".to_string()),
    });

    // Welcome System
    entries.push(AppDescriptor {
        id: "welcome".to_string(),
        name: "Welcome Messages".to_string(),
        category: "System".to_string(),
        description: "Automatically sends welcome messages to new users joining the BBS."
            .to_string(),
        summary: Some("Greets first-time users with helpful information.".to_string()),
        status: if apps_config.welcome.enabled {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        status_detail: Some("Toggle via apps.welcome.enabled".to_string()),
        enabled: apps_config.welcome.enabled,
        planned: false,
        tags: vec!["onboarding".to_string(), "automation".to_string()],
        commands: vec![],
        data_paths: vec![
            "data/welcome_queue.json".to_string(),
            "data/welcomed_nodes.json".to_string(),
        ],
        metrics: None,
        config_keys: vec!["apps.welcome.enabled".to_string()],
        actions: vec![AppAction {
            label: "Toggle Welcome".to_string(),
            kind: Some("config-toggle".to_string()),
            target: Some("apps.welcome.enabled".to_string()),
            endpoint: None,
            method: None,
            primary: true,
            disabled: false,
        }],
        notes: Some("Tracks welcomed nodes to avoid duplicate messages.".to_string()),
    });

    entries
}

// Removed: path_to_string() - no longer needed since we don't load from JSON files

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

// Removed: AppManifest and AppManifestEntry - no longer needed
// Apps are now built directly from AppsConfig

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

    #[test]
    fn build_apps_from_config_returns_all_eight_apps() {
        let apps_config = AppsConfig::default();
        let apps = build_apps_from_config(&apps_config);

        assert_eq!(apps.len(), 8, "Should return all 8 apps");

        let ids: Vec<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"fortune"));
        assert!(ids.contains(&"tinyhack"));
        assert!(ids.contains(&"tinymush"));
        assert!(ids.contains(&"eightball"));
        assert!(ids.contains(&"slotmachine"));
        assert!(ids.contains(&"weather"));
        assert!(ids.contains(&"ident_beacon"));
        assert!(ids.contains(&"welcome"));
    }

    #[test]
    fn build_apps_from_config_respects_enabled_flags() {
        let mut apps_config = AppsConfig::default();
        apps_config.tinyhack.enabled = false;
        apps_config.weather.enabled = true;

        let apps = build_apps_from_config(&apps_config);

        let tinyhack = apps.iter().find(|a| a.id == "tinyhack").expect("tinyhack");
        assert!(!tinyhack.enabled, "TinyHack should be disabled");
        assert_eq!(tinyhack.status, "offline");

        let weather = apps.iter().find(|a| a.id == "weather").expect("weather");
        assert!(weather.enabled, "Weather should be enabled");
        assert_eq!(weather.status, "online");
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
        // sessions_total reflects fortune_count(), which may be 0 if data not loaded in test context
        assert!(metrics.sessions_total.is_some());
        assert!(metrics.sessions_7d.is_some());
        assert!(metrics.last_activity.is_some());
    }
}
