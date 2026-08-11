use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use axum::{
    body::to_bytes,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    Json,
};
use meshbbs::config::{AdminDashboardConfig, AppsConfig};
use meshbbs::tmush::storage::{TinyMushStore, TinyMushStoreBuilder};
use meshbbs::tmush::types::{
    CraftingRecipe, NpcRecord, RecipeMaterial, NPC_SCHEMA_VERSION, RECIPE_SCHEMA_VERSION,
};
use meshbbs::webui::api::tinymush::{
    create_collection_item, delete_collection_item, update_collection_item, TinyMushUpsertRequest,
};
use meshbbs::webui::api::AppState;
use meshbbs::webui::audit::AuditLogger;
use meshbbs::webui::auth::AuthManager;
use meshbbs::webui::schema::SchemaRegistry;
use rand::rngs::OsRng;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

struct TestContext {
    #[allow(dead_code)]
    temp_dir: TempDir,
    state: Arc<AppState>,
    store: TinyMushStore,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("temp dir");
        let data_dir = temp_dir.path().to_path_buf();
        let store_path = data_dir.join("tinymush_db");

        let store = TinyMushStoreBuilder::new(&store_path)
            .without_world_seed()
            .with_admin_username("admin")
            .open()
            .expect("store");

        let mut admin_config = AdminDashboardConfig::default();
        admin_config.enabled = true;
        admin_config.audit_log_directory = Some(data_dir.to_string_lossy().to_string());

        let audit_logger =
            AuditLogger::new(&admin_config, data_dir.to_str().unwrap()).expect("audit logger");
        let auth_manager = AuthManager::new(admin_config.clone());

        let mut apps_config = AppsConfig::default();
        apps_config.tinymush.enabled = true;
        apps_config.tinymush.db_path = Some(store_path.to_string_lossy().to_string());

        let password_hash = Argon2::default()
            .hash_password("password".as_bytes(), &SaltString::generate(&mut OsRng))
            .expect("hash password")
            .to_string();

        let state = AppState {
            auth_manager: Arc::new(auth_manager),
            audit_logger,
            sysop_password_hash: password_hash,
            sysop_username: "admin".to_string(),
            storage: None,
            config: admin_config,
            schema_registry: Arc::new(SchemaRegistry::new()),
            data_dir: data_dir.clone(),
            config_path: None,
            games: Arc::new(RwLock::new(apps_config)),
            tinymush_store: Some(Arc::new(store.clone())),
            tinymush_store_error: None,
            tinymush_db_path: store_path,
        };

        Self {
            temp_dir,
            state: Arc::new(state),
            store,
        }
    }
}

async fn parse_json_body(response: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let payload = serde_json::from_slice(&bytes).expect("json payload");
    (status, payload)
}

fn seed_recipe(store: &TinyMushStore) {
    let mut recipe = CraftingRecipe::new("starter", "Starter", "starter_item", "admin");
    recipe.description = "Starter recipe".to_string();
    recipe.materials.push(RecipeMaterial::new("wood", 1));
    store.put_recipe(recipe).expect("seed recipe");
}

#[tokio::test]
async fn creating_recipe_from_map_materials_persists_record() {
    let ctx = TestContext::new();

    let payload = json!({
        "id": "signal_booster",
        "name": "Signal Booster",
        "description": "A powerful booster",
        "result_item_id": "signal_booster",
        "result_quantity": 2,
        "materials": {
            "copper_wire": 2,
            "antenna_rod": {"quantity": 1, "consumed": true}
        },
        "requires_station": "crafting_bench"
    });

    let (status, response) = parse_json_body(
        create_collection_item(
            State(ctx.state.clone()),
            AxumPath("recipes".to_string()),
            Json(TinyMushUpsertRequest { item: payload }),
        )
        .await,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(response["collection"].as_str(), Some("recipes"));
    let item = response
        .get("item")
        .and_then(serde_json::Value::as_object)
        .expect("item object");
    assert_eq!(
        item.get("id").and_then(serde_json::Value::as_str),
        Some("signal_booster")
    );
    assert_eq!(
        item.get("created_by").and_then(serde_json::Value::as_str),
        Some("admin")
    );

    let stored = ctx
        .store
        .get_recipe("signal_booster")
        .expect("stored recipe");
    assert_eq!(stored.name, "Signal Booster");
    assert_eq!(stored.result_quantity, 2);
    assert_eq!(stored.created_by, "admin");
    assert_eq!(stored.schema_version, RECIPE_SCHEMA_VERSION);
    assert_eq!(stored.materials.len(), 2);
    assert!(stored
        .materials
        .iter()
        .any(|m| m.item_id == "copper_wire" && m.quantity == 2));
    assert!(stored
        .materials
        .iter()
        .any(|m| m.item_id == "antenna_rod" && m.quantity == 1));
    assert_eq!(stored.requires_station.as_deref(), Some("crafting_bench"));
}

#[tokio::test]
async fn updating_recipe_rejects_duplicate_materials() {
    let ctx = TestContext::new();
    seed_recipe(&ctx.store);

    let payload = json!({
        "id": "starter",
        "name": "Starter",
        "materials": [
            {"item_id": "wood", "quantity": 1},
            {"item_id": "wood", "quantity": 2}
        ]
    });

    let (status, error) = parse_json_body(
        update_collection_item(
            State(ctx.state.clone()),
            AxumPath(("recipes".to_string(), "starter".to_string())),
            Json(TinyMushUpsertRequest { item: payload }),
        )
        .await,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = error
        .get("error")
        .and_then(serde_json::Value::as_str)
        .expect("error message");
    assert!(message.contains("Duplicate material 'wood'"));

    let stored = ctx.store.get_recipe("starter").expect("stored recipe");
    assert_eq!(stored.materials.len(), 1);
    assert_eq!(stored.materials[0].item_id, "wood");
    assert_eq!(stored.materials[0].quantity, 1);
}

#[tokio::test]
async fn deleting_recipe_removes_entry() {
    let ctx = TestContext::new();
    seed_recipe(&ctx.store);
    assert!(ctx.store.recipe_exists("starter").expect("exists"));

    let response = delete_collection_item(
        State(ctx.state.clone()),
        AxumPath(("recipes".to_string(), "starter".to_string())),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(!ctx.store.recipe_exists("starter").expect("exists"));
}

// ============================================================================
// NPC Collection Tests
// ============================================================================

fn seed_npc(store: &TinyMushStore) {
    let npc = NpcRecord::new(
        "guard",
        "Guard",
        "Town Guard",
        "Protects the town",
        "town_square",
    );
    store.put_npc(npc).expect("seed npc");
}

#[tokio::test]
async fn creating_npc_persists_record() {
    let ctx = TestContext::new();

    let payload = json!({
        "id": "merchant",
        "name": "Merchant Bob",
        "title": "Trader",
        "description": "A friendly merchant",
        "room_id": "market",
        "dialog": {
            "greeting": "Welcome to my shop!"
        }
    });

    let (status, response) = parse_json_body(
        create_collection_item(
            State(ctx.state.clone()),
            AxumPath("npcs".to_string()),
            Json(TinyMushUpsertRequest { item: payload }),
        )
        .await,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(response["collection"].as_str(), Some("npcs"));

    let item = response
        .get("item")
        .and_then(serde_json::Value::as_object)
        .expect("item object");
    assert_eq!(
        item.get("id").and_then(serde_json::Value::as_str),
        Some("merchant")
    );
    assert_eq!(
        item.get("name").and_then(serde_json::Value::as_str),
        Some("Merchant Bob")
    );

    let stored = ctx.store.get_npc("merchant").expect("stored npc");
    assert_eq!(stored.name, "Merchant Bob");
    assert_eq!(stored.title, "Trader");
    assert_eq!(stored.room_id, "market");
    assert_eq!(stored.schema_version, NPC_SCHEMA_VERSION);
    assert_eq!(
        stored.dialog.get("greeting"),
        Some(&"Welcome to my shop!".to_string())
    );
}

#[tokio::test]
async fn updating_npc_preserves_id() {
    let ctx = TestContext::new();
    seed_npc(&ctx.store);

    let payload = json!({
        "id": "guard",
        "name": "Elite Guard",
        "title": "Elite Town Guard",
        "description": "A highly trained guard",
        "room_id": "town_square"
    });

    let (status, _response) = parse_json_body(
        update_collection_item(
            State(ctx.state.clone()),
            AxumPath(("npcs".to_string(), "guard".to_string())),
            Json(TinyMushUpsertRequest { item: payload }),
        )
        .await,
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let stored = ctx.store.get_npc("guard").expect("stored npc");
    assert_eq!(stored.id, "guard");
    assert_eq!(stored.name, "Elite Guard");
    assert_eq!(stored.title, "Elite Town Guard");
}

#[tokio::test]
async fn creating_npc_without_required_fields_fails() {
    let ctx = TestContext::new();

    let payload = json!({
        "id": "incomplete_npc",
        "name": "Bob"
        // Missing: title, description, room_id
    });

    let (status, error) = parse_json_body(
        create_collection_item(
            State(ctx.state.clone()),
            AxumPath("npcs".to_string()),
            Json(TinyMushUpsertRequest { item: payload }),
        )
        .await,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = error
        .get("error")
        .and_then(serde_json::Value::as_str)
        .expect("error message");
    assert!(message.contains("title") || message.contains("required"));
}

#[tokio::test]
async fn deleting_npc_removes_entry() {
    let ctx = TestContext::new();
    seed_npc(&ctx.store);

    let npc_ids = ctx.store.list_npc_ids().expect("list npcs");
    assert!(npc_ids.contains(&"guard".to_string()));

    let response = delete_collection_item(
        State(ctx.state.clone()),
        AxumPath(("npcs".to_string(), "guard".to_string())),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let npc_ids_after = ctx.store.list_npc_ids().expect("list npcs");
    assert!(!npc_ids_after.contains(&"guard".to_string()));
}
