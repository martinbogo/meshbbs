//! # NPC API Endpoints
//!
//! Provides read-only access to the TinyMUSH NPC roster seeded from `data/seeds/npcs.json`.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::error;

use super::auth::AppState;
use super::ErrorResponse;

/// List NPCs response
#[derive(Debug, Serialize)]
pub struct ListNpcsResponse {
    pub npcs: Vec<NpcSummary>,
    pub total: usize,
    pub source_path: String,
}

/// NPC summary (for list view)
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NpcSummary {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub location: String,
    pub dialogue_topics: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NpcSeed {
    id: String,
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "description")]
    _description: Option<String>,
    location: String,
    #[serde(default)]
    dialogues: std::collections::HashMap<String, String>,
    #[serde(default)]
    flags: Vec<String>,
}

/// List all NPCs by reading the seed file from disk.
pub async fn list_npcs(State(state): State<Arc<AppState>>) -> Response {
    match load_npc_summaries(&state.data_dir).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            error!(target: "webui::npcs", "Failed to load NPC seeds: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to load NPC roster".to_string(),
                }),
            )
                .into_response()
        }
    }
}

async fn load_npc_summaries(data_dir: &Path) -> anyhow::Result<ListNpcsResponse> {
    let path = data_dir.join("seeds").join("npcs.json");
    let raw = fs::read_to_string(&path).await?;
    let seeds: Vec<NpcSeed> = serde_json::from_str(&raw)?;
    let npcs = transform_npcs(seeds);

    Ok(ListNpcsResponse {
        total: npcs.len(),
        npcs,
        source_path: path.to_string_lossy().to_string(),
    })
}

fn transform_npcs(seeds: Vec<NpcSeed>) -> Vec<NpcSummary> {
    let mut summaries: Vec<NpcSummary> = seeds
        .into_iter()
        .map(|seed| {
            let dialogue_topics: BTreeSet<String> = seed.dialogues.keys().cloned().collect();
            let mut flags = seed.flags;
            flags.sort();

            NpcSummary {
                id: seed.id,
                name: seed.name,
                title: seed.title,
                location: seed.location,
                dialogue_topics: dialogue_topics.into_iter().collect(),
                flags,
            }
        })
        .collect();

    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    summaries
}

#[cfg(test)]
mod tests {
    use super::{transform_npcs, ListNpcsResponse, NpcSeed};

    #[tokio::test]
    async fn npc_transformation_sorts_and_extracts_metadata() {
        let seeds = vec![
            NpcSeed {
                id: "gate_guard".into(),
                name: "Gate Guard".into(),
                title: Some("North Gate Guard".into()),
                _description: None,
                location: "north_gate".into(),
                dialogues: std::collections::HashMap::from([
                    ("greeting".to_string(), "hi".to_string()),
                    ("warning".to_string(), "beware".to_string()),
                ]),
                flags: vec!["Guard".to_string(), "TutorialNpc".to_string()],
            },
            NpcSeed {
                id: "mayor".into(),
                name: "Mayor".into(),
                title: None,
                _description: None,
                location: "city_hall".into(),
                dialogues: std::collections::HashMap::from([(
                    "greeting".to_string(),
                    "welcome".to_string(),
                )]),
                flags: vec![],
            },
        ];

        let summaries = transform_npcs(seeds);
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "Gate Guard");
        assert_eq!(
            summaries[0].dialogue_topics,
            vec!["greeting".to_string(), "warning".to_string()]
        );
        assert_eq!(
            summaries[0].flags,
            vec!["Guard".to_string(), "TutorialNpc".to_string()]
        );
        assert_eq!(summaries[1].name, "Mayor");
        assert!(summaries[1].title.is_none());
    }

    #[tokio::test]
    async fn list_npcs_response_retains_source_path() {
        use super::load_npc_summaries;
        use std::path::PathBuf;

        // Use the shipped seed skeleton; `./data` is gitignored and absent on a
        // fresh clone, which made this test environment-dependent.
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packaging/runtime-skel/data");
        let response: ListNpcsResponse = load_npc_summaries(&data_dir)
            .await
            .expect("Should load default NPC seeds");
        assert!(response.total > 0);
        assert!(response.source_path.contains("npcs.json"));
    }
}
