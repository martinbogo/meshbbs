use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task;
use tracing::error;

use super::auth::AppState;
use super::ErrorResponse;
use crate::tmush::errors::TinyMushError;
use crate::tmush::storage::TinyMushStore;
use crate::tmush::types::{
    AchievementCategory, AchievementRecord, AchievementTrigger, CompanionBehavior, CompanionRecord,
    CompanionType, CraftingRecipe, DialogNode, Direction, NpcFlag, NpcRecord, QuestObjective,
    QuestRecord, QuestRewards, RecipeMaterial, RoomFlag, RoomOwner, RoomRecord, RoomVisibility,
    ACHIEVEMENT_SCHEMA_VERSION, NPC_SCHEMA_VERSION, QUEST_SCHEMA_VERSION, RECIPE_SCHEMA_VERSION,
    ROOM_SCHEMA_VERSION,
};

#[derive(Debug, Serialize)]
pub struct TinyMushStatusResponse {
    pub enabled: bool,
    pub initialized: bool,
    pub db_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<TinyMushStatusCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TinyMushStatusCounts {
    pub npcs: usize,
    pub rooms: usize,
    pub companions: usize,
    pub achievements: usize,
    pub quests: usize,
    pub recipes: usize,
}

#[derive(Debug, Serialize)]
pub struct TinyMushCollectionResponse {
    pub collection: String,
    pub count: usize,
    pub items: Vec<Value>,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TinyMushItemResponse {
    pub collection: String,
    pub item: Value,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TinyMushUpsertRequest {
    pub item: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TinyMushMutationKind {
    Create,
    Update,
}

async fn mutate_collection_item(
    state: Arc<AppState>,
    raw_collection: String,
    path_id: Option<String>,
    payload: Value,
    kind: TinyMushMutationKind,
) -> Result<TinyMushItemResponse, TinymushApiError> {
    let collection = TinyMushCollection::from(raw_collection.as_str())
        .ok_or_else(|| TinymushApiError::UnknownCollection(raw_collection.clone()))?;

    ensure_enabled(&state).await?;
    let (store_arc, db_path) = require_store(&state)?;
    let username = state.sysop_username.clone();

    let db_path_clone = db_path.clone();
    let mut_payload = payload;
    let path_id_clone = path_id.clone();

    task::spawn_blocking(move || {
        let store = store_arc.as_ref().clone();
        match kind {
            TinyMushMutationKind::Create => {
                create_item(store, collection, mut_payload, username, db_path_clone)
            }
            TinyMushMutationKind::Update => update_item(
                store,
                collection,
                path_id_clone,
                mut_payload,
                username,
                db_path_clone,
            ),
        }
    })
    .await?
}

async fn delete_collection_entry(
    state: Arc<AppState>,
    raw_collection: String,
    id: String,
) -> Result<(), TinymushApiError> {
    let collection = TinyMushCollection::from(raw_collection.as_str())
        .ok_or_else(|| TinymushApiError::UnknownCollection(raw_collection.clone()))?;

    ensure_enabled(&state).await?;
    let (store_arc, db_path) = require_store(&state)?;

    task::spawn_blocking(move || {
        let store = store_arc.as_ref().clone();
        delete_item(store, collection, id, db_path)
    })
    .await??;

    Ok(())
}

fn extract_id_from_value(value: &Value) -> Result<String, TinymushApiError> {
    let Value::Object(map) = value else {
        return Err(TinymushApiError::Validation(
            "TinyMUSH payload must be a JSON object".to_string(),
        ));
    };

    match map.get("id") {
        Some(Value::String(id)) => normalize_identifier("id", id),
        Some(_) => Err(TinymushApiError::Validation(
            "Field 'id' must be a string".to_string(),
        )),
        None => Err(TinymushApiError::Validation(
            "Field 'id' is required".to_string(),
        )),
    }
}

fn normalize_identifier(field: &str, value: &str) -> Result<String, TinymushApiError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(TinymushApiError::Validation(format!(
            "Field '{field}' cannot be empty"
        )));
    }
    Ok(normalized.to_string())
}

fn normalize_non_empty(field: &str, value: &str) -> Result<String, TinymushApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(TinymushApiError::Validation(format!(
            "Field '{field}' cannot be blank"
        )));
    }
    Ok(trimmed.to_string())
}

fn canonicalize_token(value: &str) -> String {
    let mut token = String::with_capacity(value.len());
    let mut prev_was_lower = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_whitespace() || ch == '-' {
            if !token.ends_with('_') {
                token.push('_');
            }
            prev_was_lower = false;
            continue;
        }

        if ch == '_' {
            if !token.ends_with('_') {
                token.push('_');
            }
            prev_was_lower = false;
            continue;
        }

        if ch.is_ascii_uppercase() {
            if prev_was_lower && !token.ends_with('_') {
                token.push('_');
            }
            token.push(ch.to_ascii_lowercase());
            prev_was_lower = false;
        } else {
            token.push(ch.to_ascii_lowercase());
            prev_was_lower = true;
        }
    }

    token.trim_matches('_').to_string()
}

fn require_field<T>(value: Option<T>, field: &str) -> Result<T, TinymushApiError> {
    value.ok_or_else(|| {
        TinymushApiError::Validation(format!("Field '{field}' is required for TinyMUSH editing"))
    })
}

fn create_item(
    store: TinyMushStore,
    collection: TinyMushCollection,
    payload: Value,
    username: String,
    db_path: PathBuf,
) -> Result<TinyMushItemResponse, TinymushApiError> {
    let id = extract_id_from_value(&payload)?;

    match collection {
        TinyMushCollection::Npcs => {
            let record = apply_npc_payload(payload, None)?;
            store.put_npc(record)?;
        }
        TinyMushCollection::Rooms => {
            let record = apply_room_payload(payload, None)?;
            store.put_room(record)?;
        }
        TinyMushCollection::Companions => {
            let record = apply_companion_payload(payload, None)?;
            store.put_companion(record)?;
        }
        TinyMushCollection::Achievements => {
            let record = apply_achievement_payload(payload, None)?;
            store.put_achievement(record)?;
        }
        TinyMushCollection::Quests => {
            let record = apply_quest_payload(payload, None)?;
            store.put_quest(record)?;
        }
        TinyMushCollection::Recipes => {
            let record = apply_recipe_payload(payload, None, &username)?;
            store.put_recipe(record)?;
        }
    }

    build_item_response(store, collection, id, db_path)
}

fn update_item(
    store: TinyMushStore,
    collection: TinyMushCollection,
    path_id: Option<String>,
    payload: Value,
    username: String,
    db_path: PathBuf,
) -> Result<TinyMushItemResponse, TinymushApiError> {
    let payload_id = extract_id_from_value(&payload)?;
    let target_id = match path_id {
        Some(ref id) if id != &payload_id => {
            return Err(TinymushApiError::Validation(format!(
                "Payload id '{}' does not match path id '{}'",
                payload_id, id
            )))
        }
        Some(id) => id,
        None => payload_id.clone(),
    };

    match collection {
        TinyMushCollection::Npcs => {
            let existing = store.get_npc(&target_id)?;
            let record = apply_npc_payload(payload, Some(existing))?;
            store.put_npc(record)?;
        }
        TinyMushCollection::Rooms => {
            let existing = store.get_room(&target_id)?;
            let record = apply_room_payload(payload, Some(existing))?;
            store.put_room(record)?;
        }
        TinyMushCollection::Companions => {
            let existing = store.get_companion(&target_id)?;
            let record = apply_companion_payload(payload, Some(existing))?;
            store.put_companion(record)?;
        }
        TinyMushCollection::Achievements => {
            let existing = store.get_achievement(&target_id)?;
            let record = apply_achievement_payload(payload, Some(existing))?;
            store.put_achievement(record)?;
        }
        TinyMushCollection::Quests => {
            let existing = store.get_quest(&target_id)?;
            let record = apply_quest_payload(payload, Some(existing))?;
            store.put_quest(record)?;
        }
        TinyMushCollection::Recipes => {
            let existing = store.get_recipe(&target_id)?;
            let record = apply_recipe_payload(payload, Some(existing), &username)?;
            store.put_recipe(record)?;
        }
    }

    build_item_response(store, collection, target_id, db_path)
}

fn delete_item(
    store: TinyMushStore,
    collection: TinyMushCollection,
    id: String,
    _db_path: PathBuf,
) -> Result<(), TinymushApiError> {
    match collection {
        TinyMushCollection::Npcs => store.delete_npc(&id)?,
        TinyMushCollection::Rooms => store.delete_room(&id)?,
        TinyMushCollection::Companions => store.delete_companion(&id)?,
        TinyMushCollection::Achievements => store.delete_achievement(&id)?,
        TinyMushCollection::Quests => store.delete_quest(&id)?,
        TinyMushCollection::Recipes => store.delete_recipe(&id)?,
    }

    Ok(())
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct EditableNpc {
    id: Option<String>,
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    room_id: Option<String>,
    dialog: Option<HashMap<String, String>>,
    dialog_tree: Option<HashMap<String, DialogNode>>,
    flags: Option<Vec<Value>>,
    created_at: Option<DateTime<Utc>>,
    schema_version: Option<u8>,
}

fn apply_npc_payload(
    value: Value,
    existing: Option<NpcRecord>,
) -> Result<NpcRecord, TinymushApiError> {
    let editable: EditableNpc = serde_json::from_value(value.clone())
        .map_err(|err| TinymushApiError::Validation(format!("Invalid NPC payload: {err}")))?;

    let EditableNpc {
        id,
        name,
        title,
        description,
        room_id,
        dialog,
        dialog_tree,
        flags,
        created_at,
        schema_version,
    } = editable;

    let raw_id = require_field(id, "id")?;
    let id = normalize_identifier("id", &raw_id)?;

    let mut record = if let Some(existing) = existing {
        if existing.id != id {
            return Err(TinymushApiError::Validation(format!(
                "Cannot change NPC id from '{}' to '{}'",
                existing.id, id
            )));
        }
        existing
    } else {
        let name_required = require_field(name.clone(), "name")?;
        let title_required = require_field(title.clone(), "title")?;
        let description_required = require_field(description.clone(), "description")?;
        let room_required = require_field(room_id.clone(), "room_id")?;

        let name_value = normalize_non_empty("name", &name_required)?;
        let title_value = normalize_non_empty("title", &title_required)?;
        let desc_value = normalize_non_empty("description", &description_required)?;
        let room_value = normalize_identifier("room_id", &room_required)?;
        NpcRecord::new(&id, &name_value, &title_value, &desc_value, &room_value)
    };

    record.id = id;

    if let Some(name) = name {
        record.name = normalize_non_empty("name", &name)?;
    }
    if let Some(title) = title {
        record.title = normalize_non_empty("title", &title)?;
    }
    if let Some(description) = description {
        record.description = normalize_non_empty("description", &description)?;
    }
    if let Some(room_id) = room_id {
        record.room_id = normalize_identifier("room_id", &room_id)?;
    }
    if let Some(dialog) = dialog {
        record.dialog = dialog;
    }
    if let Some(dialog_tree) = dialog_tree {
        record.dialog_tree = dialog_tree;
    }
    if let Some(flag_values) = flags {
        record.flags = parse_npc_flags(flag_values)?;
    }
    if let Some(created_at) = created_at {
        record.created_at = created_at;
    }
    if let Some(schema_version) = schema_version {
        record.schema_version = schema_version;
    }

    if record.flags.len() > 1 {
        let mut seen = HashSet::new();
        record.flags.retain(|flag| seen.insert(flag.clone()));
    }

    record.schema_version = NPC_SCHEMA_VERSION;

    Ok(record)
}

fn parse_npc_flags(values: Vec<Value>) -> Result<Vec<NpcFlag>, TinymushApiError> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        if let Value::String(text) = &value {
            if let Some(flag) = parse_npc_flag_str(text) {
                result.push(flag);
                continue;
            }
        }

        match serde_json::from_value::<NpcFlag>(value.clone()) {
            Ok(flag) => result.push(flag),
            Err(_) => {
                if let Value::String(text) = value {
                    return Err(TinymushApiError::Validation(format!(
                        "Unknown NPC flag '{}'",
                        text
                    )));
                }
                return Err(TinymushApiError::Validation(
                    "NPC flags must be strings or valid flag objects".to_string(),
                ));
            }
        }
    }

    Ok(result)
}

fn parse_npc_flag_str(value: &str) -> Option<NpcFlag> {
    match canonicalize_token(value).as_str() {
        "tutorial_npc" => Some(NpcFlag::TutorialNpc),
        "quest_giver" => Some(NpcFlag::QuestGiver),
        "vendor" => Some(NpcFlag::Vendor),
        "guard" => Some(NpcFlag::Guard),
        "immortal" => Some(NpcFlag::Immortal),
        _ => None,
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct EditableRoom {
    id: Option<String>,
    name: Option<String>,
    short_desc: Option<String>,
    long_desc: Option<String>,
    owner: Option<Value>,
    visibility: Option<Value>,
    exits: Option<HashMap<String, String>>,
    items: Option<Vec<String>>,
    flags: Option<Vec<Value>>,
    max_capacity: Option<u32>,
    housing_filter_tags: Option<Vec<String>>,
    locked: Option<bool>,
    created_at: Option<DateTime<Utc>>,
    schema_version: Option<u8>,
}

fn apply_room_payload(
    value: Value,
    existing: Option<RoomRecord>,
) -> Result<RoomRecord, TinymushApiError> {
    let editable: EditableRoom = serde_json::from_value(value.clone())
        .map_err(|err| TinymushApiError::Validation(format!("Invalid room payload: {err}")))?;

    let EditableRoom {
        id,
        name,
        short_desc,
        long_desc,
        owner,
        visibility,
        exits,
        items,
        flags,
        max_capacity,
        housing_filter_tags,
        locked,
        created_at,
        schema_version,
    } = editable;

    let raw_id = require_field(id, "id")?;
    let id = normalize_identifier("id", &raw_id)?;

    let mut record = if let Some(existing) = existing {
        if existing.id != id {
            return Err(TinymushApiError::Validation(format!(
                "Cannot change room id from '{}' to '{}'",
                existing.id, id
            )));
        }
        existing
    } else {
        let name_required = require_field(name.clone(), "name")?;
        let short_required = require_field(short_desc.clone(), "short_desc")?;
        let long_required = require_field(long_desc.clone(), "long_desc")?;

        let name_value = normalize_non_empty("name", &name_required)?;
        let short_value = normalize_non_empty("short_desc", &short_required)?;
        let long_value = normalize_non_empty("long_desc", &long_required)?;

        RoomRecord::world(&id, &name_value, &short_value, &long_value)
    };

    record.id = id.clone();

    if let Some(name) = name {
        record.name = normalize_non_empty("name", &name)?;
    }
    if let Some(short_desc) = short_desc {
        record.short_desc = normalize_non_empty("short_desc", &short_desc)?;
    }
    if let Some(long_desc) = long_desc {
        record.long_desc = normalize_non_empty("long_desc", &long_desc)?;
    }
    if let Some(owner) = owner {
        record.owner = parse_room_owner(owner)?;
    }
    if let Some(visibility) = visibility {
        record.visibility = parse_room_visibility(visibility)?;
    }
    if let Some(exits) = exits {
        record.exits = parse_room_exits(exits)?;
    }
    if let Some(items) = items {
        record.items = items
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    if let Some(flag_values) = flags {
        record.flags = parse_room_flags(flag_values)?;
    }
    if let Some(capacity) = max_capacity {
        record.max_capacity = capacity.min(u16::MAX as u32) as u16;
    }
    if let Some(tags) = housing_filter_tags {
        record.housing_filter_tags = tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
    }
    if let Some(locked) = locked {
        record.locked = locked;
    }
    if let Some(created_at) = created_at {
        record.created_at = created_at;
    }
    if let Some(schema_version) = schema_version {
        record.schema_version = schema_version;
    }

    if record.flags.len() > 1 {
        let mut seen = HashSet::new();
        record.flags.retain(|flag| seen.insert(flag.clone()));
    }

    record.schema_version = ROOM_SCHEMA_VERSION;

    Ok(record)
}

fn parse_room_owner(value: Value) -> Result<RoomOwner, TinymushApiError> {
    match value {
        Value::String(text) => parse_room_owner_str(&text),
        other => serde_json::from_value::<RoomOwner>(other)
            .map_err(|err| TinymushApiError::Validation(format!("Invalid room owner: {err}"))),
    }
}

fn parse_room_owner_str(text: &str) -> Result<RoomOwner, TinymushApiError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(TinymushApiError::Validation(
            "Room owner cannot be blank".to_string(),
        ));
    }

    if canonicalize_token(trimmed) == "world" {
        return Ok(RoomOwner::World);
    }

    let lower = trimmed.to_ascii_lowercase();
    let username = if lower.starts_with("player:") {
        trimmed["player:".len()..].trim()
    } else if lower.starts_with("player/") {
        trimmed["player/".len()..].trim()
    } else {
        trimmed
    };

    if username.is_empty() {
        return Err(TinymushApiError::Validation(
            "Player owner username cannot be blank".to_string(),
        ));
    }

    Ok(RoomOwner::Player {
        username: username.to_string(),
    })
}

fn parse_room_visibility(value: Value) -> Result<RoomVisibility, TinymushApiError> {
    if let Value::String(text) = &value {
        if let Some(visibility) = parse_room_visibility_str(text) {
            return Ok(visibility);
        }
    }

    serde_json::from_value::<RoomVisibility>(value)
        .map_err(|err| TinymushApiError::Validation(format!("Invalid room visibility: {err}")))
}

fn parse_room_visibility_str(text: &str) -> Option<RoomVisibility> {
    match canonicalize_token(text).as_str() {
        "public" => Some(RoomVisibility::Public),
        "private" => Some(RoomVisibility::Private),
        "hidden" => Some(RoomVisibility::Hidden),
        _ => None,
    }
}

fn parse_room_exits(
    exits: HashMap<String, String>,
) -> Result<HashMap<Direction, String>, TinymushApiError> {
    let mut result = HashMap::with_capacity(exits.len());
    for (direction, target) in exits {
        let dir = parse_direction(&direction).ok_or_else(|| {
            TinymushApiError::Validation(format!("Unknown direction '{}'", direction))
        })?;
        let target = normalize_identifier("exit target", &target)?;
        result.insert(dir, target);
    }
    Ok(result)
}

fn parse_direction(text: &str) -> Option<Direction> {
    match canonicalize_token(text).as_str() {
        "north" | "n" => Some(Direction::North),
        "south" | "s" => Some(Direction::South),
        "east" | "e" => Some(Direction::East),
        "west" | "w" => Some(Direction::West),
        "up" | "u" => Some(Direction::Up),
        "down" | "d" => Some(Direction::Down),
        "northeast" | "ne" => Some(Direction::Northeast),
        "northwest" | "nw" => Some(Direction::Northwest),
        "southeast" | "se" => Some(Direction::Southeast),
        "southwest" | "sw" => Some(Direction::Southwest),
        _ => None,
    }
}

fn parse_room_flags(values: Vec<Value>) -> Result<Vec<RoomFlag>, TinymushApiError> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        if let Value::String(text) = &value {
            if let Some(flag) = parse_room_flag_str(text) {
                result.push(flag);
                continue;
            }
        }

        match serde_json::from_value::<RoomFlag>(value.clone()) {
            Ok(flag) => result.push(flag),
            Err(_) => {
                if let Value::String(text) = value {
                    return Err(TinymushApiError::Validation(format!(
                        "Unknown room flag '{}'",
                        text
                    )));
                }
                return Err(TinymushApiError::Validation(
                    "Room flags must be strings or valid flag objects".to_string(),
                ));
            }
        }
    }

    Ok(result)
}

fn parse_room_flag_str(text: &str) -> Option<RoomFlag> {
    match canonicalize_token(text).as_str() {
        "safe" => Some(RoomFlag::Safe),
        "dark" => Some(RoomFlag::Dark),
        "indoor" => Some(RoomFlag::Indoor),
        "shop" => Some(RoomFlag::Shop),
        "quest_location" => Some(RoomFlag::QuestLocation),
        "pvp_enabled" => Some(RoomFlag::PvpEnabled),
        "player_created" => Some(RoomFlag::PlayerCreated),
        "private" => Some(RoomFlag::Private),
        "moderated" => Some(RoomFlag::Moderated),
        "instanced" => Some(RoomFlag::Instanced),
        "crowded" => Some(RoomFlag::Crowded),
        "housing_office" => Some(RoomFlag::HousingOffice),
        "no_teleport_out" => Some(RoomFlag::NoTeleportOut),
        _ => None,
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct EditableCompanion {
    id: Option<String>,
    name: Option<String>,
    companion_type: Option<Value>,
    description: Option<String>,
    owner: Option<Option<String>>,
    room_id: Option<String>,
    loyalty: Option<u32>,
    happiness: Option<u32>,
    last_fed: Option<Option<DateTime<Utc>>>,
    behaviors: Option<Vec<Value>>,
    inventory: Option<Vec<String>>,
    is_mounted: Option<bool>,
    created_at: Option<DateTime<Utc>>,
    schema_version: Option<u8>,
}

fn apply_companion_payload(
    value: Value,
    existing: Option<CompanionRecord>,
) -> Result<CompanionRecord, TinymushApiError> {
    let editable: EditableCompanion = serde_json::from_value(value.clone())
        .map_err(|err| TinymushApiError::Validation(format!("Invalid companion payload: {err}")))?;

    let EditableCompanion {
        id,
        name,
        companion_type,
        description,
        owner,
        room_id,
        loyalty,
        happiness,
        last_fed,
        behaviors,
        inventory,
        is_mounted,
        created_at,
        schema_version,
    } = editable;

    let raw_id = require_field(id, "id")?;
    let id = normalize_identifier("id", &raw_id)?;

    let mut record = if let Some(existing) = existing {
        if existing.id != id {
            return Err(TinymushApiError::Validation(format!(
                "Cannot change companion id from '{}' to '{}'",
                existing.id, id
            )));
        }
        existing
    } else {
        let name_required = require_field(name.clone(), "name")?;
        let room_required = require_field(room_id.clone(), "room_id")?;
        let type_required = require_field(companion_type.clone(), "companion_type")?;

        let name_value = normalize_non_empty("name", &name_required)?;
        let type_enum = parse_companion_type_value(type_required)?;
        let room_value = normalize_identifier("room_id", &room_required)?;
        CompanionRecord::new(&id, &name_value, type_enum, &room_value)
    };

    record.id = id.clone();

    if let Some(name) = name {
        record.name = normalize_non_empty("name", &name)?;
    }
    if let Some(room_id) = room_id {
        record.room_id = normalize_identifier("room_id", &room_id)?;
    }
    if let Some(description) = description {
        record.description = description.trim().to_string();
    }
    if let Some(companion_type) = companion_type {
        record.companion_type = parse_companion_type_value(companion_type)?;
    }
    if let Some(owner) = owner {
        record.owner = owner.and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    }
    if let Some(loyalty) = loyalty {
        record.loyalty = loyalty.min(100);
    }
    if let Some(happiness) = happiness {
        record.happiness = happiness.min(100);
    }
    if let Some(last_fed) = last_fed {
        record.last_fed = last_fed;
    }
    if let Some(behaviors) = behaviors {
        record.behaviors = parse_companion_behaviors(behaviors)?;
    }
    if let Some(inventory) = inventory {
        record.inventory = inventory
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    if let Some(is_mounted) = is_mounted {
        record.is_mounted = is_mounted;
    }
    if let Some(created_at) = created_at {
        record.created_at = created_at;
    }
    if let Some(schema_version) = schema_version {
        record.schema_version = schema_version;
    }

    record.schema_version = 1;

    Ok(record)
}

fn parse_companion_type_value(value: Value) -> Result<CompanionType, TinymushApiError> {
    if let Value::String(text) = &value {
        if let Some(kind) = parse_companion_type_str(text) {
            return Ok(kind);
        }
    }

    serde_json::from_value::<CompanionType>(value)
        .map_err(|err| TinymushApiError::Validation(format!("Invalid companion type: {err}")))
}

fn parse_companion_type_str(text: &str) -> Option<CompanionType> {
    match canonicalize_token(text).as_str() {
        "horse" => Some(CompanionType::Horse),
        "dog" => Some(CompanionType::Dog),
        "cat" => Some(CompanionType::Cat),
        "familiar" => Some(CompanionType::Familiar),
        "mercenary" => Some(CompanionType::Mercenary),
        "construct" => Some(CompanionType::Construct),
        _ => None,
    }
}

fn parse_companion_behaviors(
    behaviors: Vec<Value>,
) -> Result<Vec<CompanionBehavior>, TinymushApiError> {
    let mut result = Vec::with_capacity(behaviors.len());
    for value in behaviors {
        result.push(
            serde_json::from_value::<CompanionBehavior>(value).map_err(|err| {
                TinymushApiError::Validation(format!("Invalid companion behavior: {err}"))
            })?,
        );
    }
    Ok(result)
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct EditableAchievement {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    category: Option<Value>,
    trigger: Option<Value>,
    title: Option<Option<String>>,
    hidden: Option<bool>,
    created_at: Option<DateTime<Utc>>,
    schema_version: Option<u8>,
}

fn apply_achievement_payload(
    value: Value,
    existing: Option<AchievementRecord>,
) -> Result<AchievementRecord, TinymushApiError> {
    let editable: EditableAchievement = serde_json::from_value(value.clone()).map_err(|err| {
        TinymushApiError::Validation(format!("Invalid achievement payload: {err}"))
    })?;

    let EditableAchievement {
        id,
        name,
        description,
        category,
        trigger,
        title,
        hidden,
        created_at,
        schema_version,
    } = editable;

    let raw_id = require_field(id, "id")?;
    let id = normalize_identifier("id", &raw_id)?;

    let mut record = if let Some(existing) = existing {
        if existing.id != id {
            return Err(TinymushApiError::Validation(format!(
                "Cannot change achievement id from '{}' to '{}'",
                existing.id, id
            )));
        }
        existing
    } else {
        let name_required = require_field(name.clone(), "name")?;
        let description_required = require_field(description.clone(), "description")?;
        let category_required = require_field(category.clone(), "category")?;
        let trigger_required = require_field(trigger.clone(), "trigger")?;

        let category_enum = parse_achievement_category_value(category_required)?;
        let trigger_enum = parse_achievement_trigger(trigger_required)?;
        let name_value = normalize_non_empty("name", &name_required)?;
        let desc_value = normalize_non_empty("description", &description_required)?;

        AchievementRecord::new(&id, &name_value, &desc_value, category_enum, trigger_enum)
    };

    record.id = id.clone();

    if let Some(name) = name {
        record.name = normalize_non_empty("name", &name)?;
    }
    if let Some(description) = description {
        record.description = normalize_non_empty("description", &description)?;
    }
    if let Some(category) = category {
        record.category = parse_achievement_category_value(category)?;
    }
    if let Some(trigger) = trigger {
        record.trigger = parse_achievement_trigger(trigger)?;
    }
    if let Some(title) = title {
        record.title = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
    }
    if let Some(hidden) = hidden {
        record.hidden = hidden;
    }
    if let Some(created_at) = created_at {
        record.created_at = created_at;
    }
    if let Some(schema_version) = schema_version {
        record.schema_version = schema_version;
    }

    record.schema_version = ACHIEVEMENT_SCHEMA_VERSION;

    Ok(record)
}

fn parse_achievement_category_value(value: Value) -> Result<AchievementCategory, TinymushApiError> {
    if let Value::String(text) = &value {
        if let Some(category) = parse_achievement_category_str(text) {
            return Ok(category);
        }
    }

    serde_json::from_value::<AchievementCategory>(value)
        .map_err(|err| TinymushApiError::Validation(format!("Invalid achievement category: {err}")))
}

fn parse_achievement_category_str(text: &str) -> Option<AchievementCategory> {
    match canonicalize_token(text).as_str() {
        "combat" => Some(AchievementCategory::Combat),
        "exploration" => Some(AchievementCategory::Exploration),
        "social" => Some(AchievementCategory::Social),
        "economic" => Some(AchievementCategory::Economic),
        "quest" => Some(AchievementCategory::Quest),
        "special" => Some(AchievementCategory::Special),
        "crafting" => Some(AchievementCategory::Crafting),
        _ => None,
    }
}

fn parse_achievement_trigger(value: Value) -> Result<AchievementTrigger, TinymushApiError> {
    serde_json::from_value::<AchievementTrigger>(value)
        .map_err(|err| TinymushApiError::Validation(format!("Invalid achievement trigger: {err}")))
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct EditableQuest {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    quest_giver_npc: Option<String>,
    difficulty: Option<u8>,
    objectives: Option<Value>,
    rewards: Option<Value>,
    prerequisites: Option<Vec<String>>,
    created_at: Option<DateTime<Utc>>,
    schema_version: Option<u8>,
}

fn apply_quest_payload(
    value: Value,
    existing: Option<QuestRecord>,
) -> Result<QuestRecord, TinymushApiError> {
    let editable: EditableQuest = serde_json::from_value(value.clone())
        .map_err(|err| TinymushApiError::Validation(format!("Invalid quest payload: {err}")))?;

    let EditableQuest {
        id,
        name,
        description,
        quest_giver_npc,
        difficulty,
        objectives,
        rewards,
        prerequisites,
        created_at,
        schema_version,
    } = editable;

    let raw_id = require_field(id, "id")?;
    let id = normalize_identifier("id", &raw_id)?;

    let mut record = if let Some(existing) = existing {
        if existing.id != id {
            return Err(TinymushApiError::Validation(format!(
                "Cannot change quest id from '{}' to '{}'",
                existing.id, id
            )));
        }
        existing
    } else {
        let name_required = require_field(name.clone(), "name")?;
        let description_required = require_field(description.clone(), "description")?;
        let giver_required = require_field(quest_giver_npc.clone(), "quest_giver_npc")?;

        let name_value = normalize_non_empty("name", &name_required)?;
        let desc_value = normalize_non_empty("description", &description_required)?;
        let giver_value = normalize_identifier("quest_giver_npc", &giver_required)?;
        let difficulty_value = difficulty.unwrap_or(1).max(1).min(5);

        QuestRecord::new(
            &id,
            &name_value,
            &desc_value,
            &giver_value,
            difficulty_value,
        )
    };

    record.id = id.clone();

    if let Some(name) = name {
        record.name = normalize_non_empty("name", &name)?;
    }
    if let Some(description) = description {
        record.description = normalize_non_empty("description", &description)?;
    }
    if let Some(quest_giver) = quest_giver_npc {
        record.quest_giver_npc = normalize_identifier("quest_giver_npc", &quest_giver)?;
    }
    if let Some(difficulty) = difficulty {
        record.difficulty = difficulty.max(1).min(5);
    }
    if let Some(objectives) = objectives {
        record.objectives = parse_quest_objectives(objectives)?;
    }
    if let Some(rewards) = rewards {
        record.rewards = parse_quest_rewards(rewards)?;
    }
    if let Some(prerequisites) = prerequisites {
        let mut normalized = Vec::with_capacity(prerequisites.len());
        for entry in prerequisites {
            let value = normalize_identifier("prerequisite", &entry)?;
            normalized.push(value);
        }
        record.prerequisites = normalized;
    }
    if let Some(created_at) = created_at {
        record.created_at = created_at;
    }
    if let Some(schema_version) = schema_version {
        record.schema_version = schema_version;
    }

    record.schema_version = QUEST_SCHEMA_VERSION;

    Ok(record)
}

fn parse_quest_objectives(value: Value) -> Result<Vec<QuestObjective>, TinymushApiError> {
    serde_json::from_value::<Vec<QuestObjective>>(value)
        .map_err(|err| TinymushApiError::Validation(format!("Invalid quest objectives: {err}")))
}

fn parse_quest_rewards(value: Value) -> Result<QuestRewards, TinymushApiError> {
    serde_json::from_value::<QuestRewards>(value)
        .map_err(|err| TinymushApiError::Validation(format!("Invalid quest rewards: {err}")))
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct EditableRecipe {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    result_item_id: Option<String>,
    result_quantity: Option<u32>,
    materials: Option<Value>,
    requires_station: Option<Option<String>>,
    skill_required: Option<Option<String>>,
    skill_level: Option<u8>,
    crafting_time_seconds: Option<u32>,
    created_at: Option<DateTime<Utc>>,
    created_by: Option<String>,
    schema_version: Option<u8>,
}

fn apply_recipe_payload(
    value: Value,
    existing: Option<CraftingRecipe>,
    username: &str,
) -> Result<CraftingRecipe, TinymushApiError> {
    let editable: EditableRecipe = serde_json::from_value(value.clone())
        .map_err(|err| TinymushApiError::Validation(format!("Invalid recipe payload: {err}")))?;

    let EditableRecipe {
        id,
        name,
        description,
        result_item_id,
        result_quantity,
        materials,
        requires_station,
        skill_required,
        skill_level,
        crafting_time_seconds,
        created_at,
        created_by,
        schema_version,
    } = editable;

    let raw_id = require_field(id, "id")?;
    let id = normalize_identifier("id", &raw_id)?;

    let is_new = existing.is_none();
    let materials_value = if is_new {
        Some(require_field(materials.clone(), "materials")?)
    } else {
        materials
    };

    let parsed_materials = match materials_value {
        Some(materials_raw) => {
            let materials = parse_recipe_materials(materials_raw)?;
            if materials.is_empty() {
                return Err(TinymushApiError::Validation(
                    "Recipes must include at least one material".to_string(),
                ));
            }
            Some(materials)
        }
        None => None,
    };

    let mut record = match existing {
        Some(mut existing_record) => {
            if existing_record.id != id {
                return Err(TinymushApiError::Validation(format!(
                    "Cannot change recipe id from '{}' to '{}'",
                    existing_record.id, id
                )));
            }

            if let Some(submitted_creator) = created_by.as_ref() {
                let trimmed = submitted_creator.trim();
                if !trimmed.is_empty() && trimmed != existing_record.created_by {
                    return Err(TinymushApiError::Validation(
                        "Recipe 'created_by' cannot be modified".to_string(),
                    ));
                }
            }

            existing_record
        }
        None => {
            if let Some(submitted_creator) = created_by.as_ref() {
                let trimmed = submitted_creator.trim();
                if !trimmed.is_empty() && trimmed != username.trim() {
                    return Err(TinymushApiError::Validation(
                        "Cannot override recipe creator".to_string(),
                    ));
                }
            }

            let name_required = require_field(name.clone(), "name")?;
            let description_required = require_field(description.clone(), "description")?;
            let result_required = require_field(result_item_id.clone(), "result_item_id")?;

            let creator = username.trim();
            if creator.is_empty() {
                return Err(TinymushApiError::Validation(
                    "Cannot determine recipe creator".to_string(),
                ));
            }

            let name_value = normalize_non_empty("name", &name_required)?;
            let description_value = normalize_non_empty("description", &description_required)?;
            let result_value = normalize_identifier("result_item_id", &result_required)?;

            let mut recipe = CraftingRecipe::new(&id, &name_value, &result_value, creator);
            recipe.description = description_value;
            recipe
        }
    };

    record.id = id.clone();

    if let Some(name) = name {
        record.name = normalize_non_empty("name", &name)?;
    }
    if let Some(description) = description {
        record.description = normalize_non_empty("description", &description)?;
    }
    if let Some(result_item_id) = result_item_id {
        record.result_item_id = normalize_identifier("result_item_id", &result_item_id)?;
    }
    if let Some(quantity) = result_quantity {
        if quantity == 0 {
            return Err(TinymushApiError::Validation(
                "Field 'result_quantity' must be at least 1".to_string(),
            ));
        }
        record.result_quantity = quantity;
    }

    if let Some(materials) = parsed_materials {
        record.materials = materials;
    }

    if record.materials.is_empty() {
        return Err(TinymushApiError::Validation(
            "Recipes must include at least one material".to_string(),
        ));
    }

    if let Some(requires_station) = requires_station {
        record.requires_station = match requires_station {
            Some(station) => Some(normalize_identifier("requires_station", &station)?),
            None => None,
        };
    }

    if let Some(skill_required) = skill_required {
        record.skill_required = match skill_required {
            Some(skill) => Some(normalize_identifier("skill_required", &skill)?),
            None => None,
        };
    }

    if let Some(skill_level) = skill_level {
        record.skill_level = skill_level;
    }

    if let Some(crafting_time_seconds) = crafting_time_seconds {
        record.crafting_time_seconds = crafting_time_seconds;
    }

    if let Some(created_at) = created_at {
        record.created_at = created_at;
    }

    if let Some(schema_version) = schema_version {
        record.schema_version = schema_version;
    }

    let mut seen_items = HashSet::new();
    for material in &mut record.materials {
        let normalized_id = normalize_identifier("materials.item_id", material.item_id.as_str())?;
        material.item_id = normalized_id;

        if material.quantity == 0 {
            return Err(TinymushApiError::Validation(format!(
                "Material '{}' must have quantity at least 1",
                material.item_id
            )));
        }

        if !seen_items.insert(material.item_id.clone()) {
            return Err(TinymushApiError::Validation(format!(
                "Duplicate material '{}' specified",
                material.item_id
            )));
        }
    }

    record.schema_version = RECIPE_SCHEMA_VERSION;

    Ok(record)
}

fn parse_recipe_materials(value: Value) -> Result<Vec<RecipeMaterial>, TinymushApiError> {
    match value {
        Value::Array(entries) => {
            let mut materials = Vec::with_capacity(entries.len());
            for entry in entries {
                let material: RecipeMaterial = serde_json::from_value(entry).map_err(|err| {
                    TinymushApiError::Validation(format!("Invalid recipe material entry: {err}"))
                })?;
                materials.push(material);
            }
            Ok(materials)
        }
        Value::Object(map) => {
            let mut materials = Vec::with_capacity(map.len());
            for (raw_id, data) in map {
                match data {
                    Value::Object(mut obj) => {
                        obj.entry("item_id".to_string())
                            .or_insert(Value::String(raw_id.clone()));
                        let material: RecipeMaterial = serde_json::from_value(Value::Object(obj))
                            .map_err(|err| {
                            TinymushApiError::Validation(format!(
                                "Invalid recipe material entry for '{raw_id}': {err}"
                            ))
                        })?;
                        materials.push(material);
                    }
                    other => {
                        let quantity = parse_recipe_material_quantity(other)?;
                        let material = RecipeMaterial::new(&raw_id, quantity);
                        materials.push(material);
                    }
                }
            }
            Ok(materials)
        }
        _ => Err(TinymushApiError::Validation(
            "Field 'materials' must be an array or object".to_string(),
        )),
    }
}

fn parse_recipe_material_quantity(value: Value) -> Result<u32, TinymushApiError> {
    match value {
        Value::Number(num) => {
            let Some(raw) = num.as_u64() else {
                return Err(TinymushApiError::Validation(
                    "Recipe material quantity must be a positive integer".to_string(),
                ));
            };

            if raw == 0 || raw > u32::MAX as u64 {
                return Err(TinymushApiError::Validation(
                    "Recipe material quantity must be between 1 and 4294967295".to_string(),
                ));
            }

            Ok(raw as u32)
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(TinymushApiError::Validation(
                    "Recipe material quantity cannot be blank".to_string(),
                ));
            }

            let parsed = trimmed.parse::<u32>().map_err(|_| {
                TinymushApiError::Validation(format!(
                    "Invalid recipe material quantity '{trimmed}'"
                ))
            })?;

            if parsed == 0 {
                return Err(TinymushApiError::Validation(
                    "Recipe material quantity must be at least 1".to_string(),
                ));
            }

            Ok(parsed)
        }
        _ => Err(TinymushApiError::Validation(
            "Recipe material quantity must be a positive integer".to_string(),
        )),
    }
}

pub async fn get_tinymush_status(State(state): State<Arc<AppState>>) -> Response {
    let enabled = state.games.read().await.tinymush_enabled;
    let db_path = state.tinymush_db_path.to_string_lossy().into_owned();
    let updated_at = last_modified(&state.tinymush_db_path).ok();

    if !enabled {
        return (
            StatusCode::OK,
            Json(TinyMushStatusResponse {
                enabled,
                initialized: false,
                db_path,
                error: Some("TinyMUSH is disabled in configuration".to_string()),
                counts: None,
                updated_at,
            }),
        )
            .into_response();
    }

    let Some(store_arc) = state.tinymush_store.clone() else {
        return (
            StatusCode::OK,
            Json(TinyMushStatusResponse {
                enabled,
                initialized: false,
                db_path,
                error: state
                    .tinymush_store_error
                    .clone()
                    .or_else(|| Some("TinyMUSH database has not been initialized".to_string())),
                counts: None,
                updated_at,
            }),
        )
            .into_response();
    };

    let counts = task::spawn_blocking(move || collect_status_counts(store_arc.as_ref().clone()))
        .await
        .map_err(TinymushApiError::from)
        .and_then(|res| res.map_err(TinymushApiError::from));

    match counts {
        Ok(counts) => (
            StatusCode::OK,
            Json(TinyMushStatusResponse {
                enabled,
                initialized: true,
                db_path,
                error: None,
                counts: Some(counts),
                updated_at,
            }),
        )
            .into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn get_collection(
    State(state): State<Arc<AppState>>,
    AxumPath(collection): AxumPath<String>,
) -> Response {
    match load_collection(&state, collection).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn get_collection_item(
    State(state): State<Arc<AppState>>,
    AxumPath((collection, id)): AxumPath<(String, String)>,
) -> Response {
    match load_collection_item(&state, collection, id).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn create_collection_item(
    State(state): State<Arc<AppState>>,
    AxumPath(collection): AxumPath<String>,
    Json(payload): Json<TinyMushUpsertRequest>,
) -> Response {
    match mutate_collection_item(
        state,
        collection,
        None,
        payload.item,
        TinyMushMutationKind::Create,
    )
    .await
    {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn update_collection_item(
    State(state): State<Arc<AppState>>,
    AxumPath((collection, id)): AxumPath<(String, String)>,
    Json(payload): Json<TinyMushUpsertRequest>,
) -> Response {
    match mutate_collection_item(
        state,
        collection,
        Some(id),
        payload.item,
        TinyMushMutationKind::Update,
    )
    .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => err.into_response(),
    }
}

pub async fn delete_collection_item(
    State(state): State<Arc<AppState>>,
    AxumPath((collection, id)): AxumPath<(String, String)>,
) -> Response {
    match delete_collection_entry(state, collection, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => err.into_response(),
    }
}

fn method_not_allowed(message: &str) -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
        .into_response()
}

async fn load_collection(
    state: &AppState,
    raw_collection: String,
) -> Result<TinyMushCollectionResponse, TinymushApiError> {
    let collection = TinyMushCollection::from(raw_collection.as_str())
        .ok_or_else(|| TinymushApiError::UnknownCollection(raw_collection.clone()))?;

    ensure_enabled(state).await?;
    let (store_arc, db_path) = require_store(state)?;

    let result = task::spawn_blocking(move || {
        let store = store_arc.as_ref().clone();
        build_collection_response(store, collection, db_path)
    })
    .await?;

    result
}

async fn load_collection_item(
    state: &AppState,
    raw_collection: String,
    id: String,
) -> Result<TinyMushItemResponse, TinymushApiError> {
    let collection = TinyMushCollection::from(raw_collection.as_str())
        .ok_or_else(|| TinymushApiError::UnknownCollection(raw_collection.clone()))?;

    ensure_enabled(state).await?;
    let (store_arc, db_path) = require_store(state)?;

    let result = task::spawn_blocking(move || {
        let store = store_arc.as_ref().clone();
        build_item_response(store, collection, id, db_path)
    })
    .await?;

    result
}

fn build_collection_response(
    store: TinyMushStore,
    collection: TinyMushCollection,
    db_path: PathBuf,
) -> Result<TinyMushCollectionResponse, TinymushApiError> {
    let (items, total) = load_all(store, collection)?;

    Ok(TinyMushCollectionResponse {
        collection: collection.as_str().to_string(),
        count: total,
        items,
        source_path: db_path.to_string_lossy().into_owned(),
        updated_at: last_modified(&db_path).ok(),
    })
}

fn build_item_response(
    store: TinyMushStore,
    collection: TinyMushCollection,
    id: String,
    db_path: PathBuf,
) -> Result<TinyMushItemResponse, TinymushApiError> {
    let item = match collection {
        TinyMushCollection::Npcs => to_value(store.get_npc(&id)?)?,
        TinyMushCollection::Rooms => to_value(store.get_room(&id)?)?,
        TinyMushCollection::Companions => to_value(store.get_companion(&id)?)?,
        TinyMushCollection::Achievements => to_value(store.get_achievement(&id)?)?,
        TinyMushCollection::Quests => to_value(store.get_quest(&id)?)?,
        TinyMushCollection::Recipes => to_value(store.get_recipe(&id)?)?,
    };

    Ok(TinyMushItemResponse {
        collection: collection.as_str().to_string(),
        item,
        source_path: db_path.to_string_lossy().into_owned(),
        updated_at: last_modified(&db_path).ok(),
    })
}

fn load_all(
    store: TinyMushStore,
    collection: TinyMushCollection,
) -> Result<(Vec<Value>, usize), TinymushApiError> {
    match collection {
        TinyMushCollection::Npcs => {
            let ids = store.list_npc_ids()?;
            let total = ids.len();
            let mut items = Vec::with_capacity(total);
            for id in ids {
                match store.get_npc(&id) {
                    Ok(npc) => items.push(to_value(npc)?),
                    Err(err) => error!(target: "webui::tinymush", "Failed to load NPC {id}: {err}"),
                }
            }
            Ok((items, total))
        }
        TinyMushCollection::Rooms => {
            let ids = store.list_room_ids()?;
            let total = ids.len();
            let mut items = Vec::with_capacity(total);
            for id in ids {
                match store.get_room(&id) {
                    Ok(room) => items.push(to_value(room)?),
                    Err(err) => {
                        error!(target: "webui::tinymush", "Failed to load room {id}: {err}")
                    }
                }
            }
            Ok((items, total))
        }
        TinyMushCollection::Companions => {
            let ids = store.list_companion_ids()?;
            let total = ids.len();
            let mut items = Vec::with_capacity(total);
            for id in ids {
                match store.get_companion(&id) {
                    Ok(companion) => items.push(to_value(companion)?),
                    Err(err) => {
                        error!(target: "webui::tinymush", "Failed to load companion {id}: {err}")
                    }
                }
            }
            Ok((items, total))
        }
        TinyMushCollection::Achievements => {
            let ids = store.list_achievement_ids()?;
            let total = ids.len();
            let mut items = Vec::with_capacity(total);
            for id in ids {
                match store.get_achievement(&id) {
                    Ok(achievement) => items.push(to_value(achievement)?),
                    Err(err) => {
                        error!(target: "webui::tinymush", "Failed to load achievement {id}: {err}")
                    }
                }
            }
            Ok((items, total))
        }
        TinyMushCollection::Quests => {
            let ids = store.list_quest_ids()?;
            let total = ids.len();
            let mut items = Vec::with_capacity(total);
            for id in ids {
                match store.get_quest(&id) {
                    Ok(quest) => items.push(to_value(quest)?),
                    Err(err) => {
                        error!(target: "webui::tinymush", "Failed to load quest {id}: {err}")
                    }
                }
            }
            Ok((items, total))
        }
        TinyMushCollection::Recipes => {
            let recipes = store.list_recipes(None)?;
            let total = recipes.len();
            let mut items = Vec::with_capacity(total);
            for recipe in recipes {
                items.push(to_value(recipe)?);
            }
            Ok((items, total))
        }
    }
}

fn collect_status_counts(store: TinyMushStore) -> Result<TinyMushStatusCounts, TinyMushError> {
    let npcs = store.list_npc_ids()?.len();
    let rooms = store.list_room_ids()?.len();
    let companions = store.list_companion_ids()?.len();
    let achievements = store.list_achievement_ids()?.len();
    let quests = store.list_quest_ids()?.len();
    let recipes = store.list_recipes(None)?.len();

    Ok(TinyMushStatusCounts {
        npcs,
        rooms,
        companions,
        achievements,
        quests,
        recipes,
    })
}

async fn ensure_enabled(state: &AppState) -> Result<(), TinymushApiError> {
    if state.games.read().await.tinymush_enabled {
        Ok(())
    } else {
        Err(TinymushApiError::Disabled)
    }
}

fn require_store(state: &AppState) -> Result<(Arc<TinyMushStore>, PathBuf), TinymushApiError> {
    match state.tinymush_store.clone() {
        Some(store) => Ok((store, state.tinymush_db_path.clone())),
        None => Err(TinymushApiError::NotReady(
            state
                .tinymush_store_error
                .clone()
                .unwrap_or_else(|| "TinyMUSH database is not initialized".to_string()),
        )),
    }
}

#[derive(Debug, Clone, Copy)]
enum TinyMushCollection {
    Npcs,
    Rooms,
    Companions,
    Achievements,
    Quests,
    Recipes,
}

impl TinyMushCollection {
    fn from(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "npcs" => Some(Self::Npcs),
            "rooms" => Some(Self::Rooms),
            "companions" => Some(Self::Companions),
            "achievements" => Some(Self::Achievements),
            "quests" => Some(Self::Quests),
            "recipes" => Some(Self::Recipes),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Npcs => "npcs",
            Self::Rooms => "rooms",
            Self::Companions => "companions",
            Self::Achievements => "achievements",
            Self::Quests => "quests",
            Self::Recipes => "recipes",
        }
    }
}

#[derive(Debug)]
enum TinymushApiError {
    Disabled,
    NotReady(String),
    UnknownCollection(String),
    Store(TinyMushError),
    Serialization(serde_json::Error),
    Join(tokio::task::JoinError),
    Validation(String),
}

impl From<TinyMushError> for TinymushApiError {
    fn from(value: TinyMushError) -> Self {
        Self::Store(value)
    }
}

impl From<serde_json::Error> for TinymushApiError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}

impl From<tokio::task::JoinError> for TinymushApiError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value)
    }
}

impl TinymushApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            TinymushApiError::Disabled => (
                StatusCode::SERVICE_UNAVAILABLE,
                "TinyMUSH is disabled in configuration".to_string(),
            ),
            TinymushApiError::NotReady(reason) => (StatusCode::SERVICE_UNAVAILABLE, reason.clone()),
            TinymushApiError::UnknownCollection(name) => (
                StatusCode::NOT_FOUND,
                format!("Unknown TinyMUSH collection '{name}'"),
            ),
            TinymushApiError::Store(err) => {
                error!(target: "webui::tinymush", "TinyMUSH store error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "TinyMUSH storage error".to_string(),
                )
            }
            TinymushApiError::Serialization(err) => {
                error!(target: "webui::tinymush", "Serialization error: {err}");
                (
                    StatusCode::BAD_REQUEST,
                    "Failed to serialize TinyMUSH data".to_string(),
                )
            }
            TinymushApiError::Join(err) => {
                error!(target: "webui::tinymush", "Task join error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "TinyMUSH task failed".to_string(),
                )
            }
            TinymushApiError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

fn to_value<T: Serialize>(value: T) -> Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}

fn last_modified(path: &PathBuf) -> Result<String, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified()?;
    let datetime: DateTime<Utc> = DateTime::<Utc>::from(modified);
    Ok(datetime.to_rfc3339())
}
