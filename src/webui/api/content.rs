use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::error;

use super::auth::AppState;
use super::ErrorResponse;

/// Aggregate overview of all seed-driven content collections.
#[derive(Debug, Serialize)]
pub struct WorldOverviewResponse {
    pub npcs: SeedSummary,
    pub rooms: SeedSummary,
    pub companions: SeedSummary,
    pub achievements: SeedSummary,
    pub quests: SeedSummary,
    pub recipes: SeedSummary,
}

/// Metadata about a single seed collection.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SeedSummary {
    pub count: usize,
    pub sample: Vec<String>,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Return counts and sample entries for all TinyMUSH seed files.
pub async fn get_world_overview(State(state): State<Arc<AppState>>) -> Response {
    match build_world_overview(&state.data_dir).await {
        Ok(overview) => (StatusCode::OK, Json(overview)).into_response(),
        Err(err) => {
            error!(target: "webui::world", "Failed to load world overview: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to load world overview".to_string(),
                }),
            )
                .into_response()
        }
    }
}

async fn build_world_overview(data_dir: &Path) -> anyhow::Result<WorldOverviewResponse> {
    let base = data_dir.join("seeds");

    Ok(WorldOverviewResponse {
        npcs: build_seed_summary(&base, "npcs.json", None, "name").await?,
        rooms: build_seed_summary(&base, "rooms.json", None, "name").await?,
        companions: build_seed_summary(&base, "companions.json", None, "name").await?,
        achievements: build_seed_summary(&base, "achievements.json", None, "name").await?,
        quests: build_seed_summary(&base, "quests.json", Some("quests"), "name").await?,
        recipes: build_seed_summary(&base, "recipes.json", Some("recipes"), "name").await?,
    })
}

async fn build_seed_summary(
    base: &Path,
    file_name: &str,
    collection_key: Option<&str>,
    name_field: &str,
) -> anyhow::Result<SeedSummary> {
    let path = base.join(file_name);
    let raw = fs::read_to_string(&path).await?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;

    let entries: Vec<serde_json::Value> = match collection_key {
        Some(key) => value
            .get(key)
            .and_then(|val| val.as_array())
            .ok_or_else(|| anyhow::anyhow!("Expected array at key '{key}' in {file_name}"))?
            .clone(),
        None => value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Expected top-level array in {file_name}"))?
            .clone(),
    };

    let mut sample = Vec::new();
    for entry in &entries {
        if let Some(name) = entry.get(name_field).and_then(|v| v.as_str()) {
            sample.push(name.to_string());
        }
        if sample.len() >= 3 {
            break;
        }
    }

    let updated_at = match fs::metadata(&path).await {
        Ok(metadata) => match metadata.modified() {
            Ok(modified) => Some(DateTime::<Utc>::from(modified).to_rfc3339()),
            Err(_) => None,
        },
        Err(_) => None,
    };

    Ok(SeedSummary {
        count: entries.len(),
        sample,
        source_path: path_to_string(&path),
        updated_at,
    })
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_seed_summary, build_world_overview, SeedSummary};
    use std::path::PathBuf;

    #[tokio::test]
    async fn world_overview_reads_all_seed_files() {
        let overview = build_world_overview(PathBuf::from("./data").as_path())
            .await
            .expect("world overview should load");

        assert!(overview.npcs.count > 0);
        assert!(overview.rooms.count > 0);
        assert!(overview.quests.count > 0);
        assert!(overview.recipes.count > 0);
    }

    #[tokio::test]
    async fn seed_summary_supports_keyed_collections() {
        use std::fs;
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let base = temp_dir.path();
        let file_path = base.join("quests.json");

        let mut file = fs::File::create(&file_path).expect("write temp quests");
        write!(
            file,
            "{{\n  \"quests\": [{{ \"name\": \"Quest One\" }}, {{ \"name\": \"Quest Two\" }}]\n}}"
        )
        .expect("write data");

        let summary: SeedSummary = build_seed_summary(base, "quests.json", Some("quests"), "name")
            .await
            .expect("summary loads");

        assert_eq!(summary.count, 2);
        assert_eq!(summary.sample, vec!["Quest One", "Quest Two"]);
    }
}
