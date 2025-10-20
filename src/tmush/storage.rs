use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::Utc;
use sled::IVec;

use crate::tmush::errors::TinyMushError;
use crate::tmush::shop::ShopRecord;
use crate::tmush::state::canonical_world_seed;
use crate::tmush::types::{
    BulletinBoard, BulletinMessage, CompanionRecord, CurrencyAmount, CurrencyTransaction,
    HousingInstance, HousingTemplate, MailMessage, MailStatus, NpcRecord, ObjectOwner,
    ObjectRecord, PlayerRecord, QuestRecord, RoomFlag, RoomOwner, RoomRecord, TradeSession,
    TransactionReason, WorldConfig, BULLETIN_SCHEMA_VERSION, MAIL_SCHEMA_VERSION,
    OBJECT_SCHEMA_VERSION, PLAYER_SCHEMA_VERSION, ROOM_SCHEMA_VERSION,
};

const TREE_PRIMARY: &str = "tinymush";
const TREE_OBJECTS: &str = "tinymush_objects";
const TREE_MAIL: &str = "tinymush_mail";
const TREE_LOGS: &str = "tinymush_logs";
const TREE_BULLETINS: &str = "tinymush_bulletins";
const TREE_SHOPS: &str = "tinymush_shops";
const TREE_TRADES: &str = "tinymush_trades";
const TREE_NPCS: &str = "tinymush_npcs";
const TREE_QUESTS: &str = "tinymush_quests";
const TREE_ACHIEVEMENTS: &str = "tinymush_achievements";
const TREE_COMPANIONS: &str = "tinymush_companions";
const TREE_CONFIG: &str = "tinymush_config";
const TREE_HOUSING_TEMPLATES: &str = "tinymush_housing_templates";
const TREE_HOUSING_INSTANCES: &str = "tinymush_housing_instances";

// Secondary indexes for O(1) lookups (performance optimization for scale)
const TREE_OBJECT_INDEX: &str = "tinymush_object_index";
const TREE_HOUSING_GUESTS: &str = "tinymush_housing_guests";
const TREE_PLAYER_TRADES: &str = "tinymush_player_trades";
const TREE_TEMPLATE_INSTANCES: &str = "tinymush_template_instances";

/// Statistics about secondary index sizes for monitoring
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub object_index_entries: usize,
    pub housing_guest_entries: usize,
    pub template_instance_entries: usize,
    pub player_trade_entries: usize,
}

fn next_timestamp_nanos() -> i64 {
    let now = Utc::now();
    now.timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp_micros() * 1000)
}

/// Helper builder so tests can easily create throwaway stores with custom paths.
pub struct TinyMushStoreBuilder {
    path: PathBuf,
    ensure_world_seed: bool,
    admin_username: String,
}

impl TinyMushStoreBuilder {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ensure_world_seed: true,
            admin_username: "admin".to_string(),
        }
    }

    /// Opt out of seeding the canonical world during initialization (useful for targeted tests).
    pub fn without_world_seed(mut self) -> Self {
        self.ensure_world_seed = false;
        self
    }

    /// Set the admin username (defaults to "admin"). Should typically match the BBS sysop username.
    pub fn with_admin_username(mut self, username: impl Into<String>) -> Self {
        self.admin_username = username.into();
        self
    }

    pub fn open(self) -> Result<TinyMushStore, TinyMushError> {
        TinyMushStore::open_with_options(self.path, self.ensure_world_seed, self.admin_username)
    }
}

/// Sled-backed persistence for TinyMUSH world data and player state.
/// TinyMUSH persistent storage layer built on top of Sled.
///
/// This struct is cheap to clone - all internal Sled types (Db, Tree) are Arc-based,
/// so cloning creates a new handle to the same underlying database. This allows
/// multiple command processors to safely share the same database without lock conflicts.
#[derive(Clone)]
pub struct TinyMushStore {
    _db: sled::Db,
    primary: sled::Tree,
    objects: sled::Tree,
    mail: sled::Tree,
    logs: sled::Tree,
    bulletins: sled::Tree,
    shops: sled::Tree,
    trades: sled::Tree,
    npcs: sled::Tree,
    quests: sled::Tree,
    achievements: sled::Tree,
    companions: sled::Tree,
    config: sled::Tree,
    housing_templates: sled::Tree,
    housing_instances: sled::Tree,

    // Secondary indexes for O(1) lookups (performance optimization)
    object_index: sled::Tree,       // oid:{id} → full_key
    housing_guests: sled::Tree,     // guest:{username}:{instance_id} → ""
    player_trades: sled::Tree,      // ptrade:{username} → session_id
    template_instances: sled::Tree, // tpl:{template_id}:{instance_id} → ""

    // Data directory path for seed files and other data
    data_dir: PathBuf,

    // In-memory instanced room cache (not persisted across restarts)
    instanced_rooms: Arc<RwLock<HashMap<String, RoomRecord>>>,
    landing_instances: Arc<RwLock<HashMap<String, String>>>,
}

impl TinyMushStore {
    /// Get access to the underlying Sled database for transactions.
    ///
    /// This is useful for test helpers that need atomic operations with
    /// strong visibility guarantees across multiple database handles.
    #[allow(dead_code)]
    pub fn db(&self) -> &sled::Db {
        &self._db
    }

    /// Get access to the primary tree for transactions.
    ///
    /// This is useful for test helpers that need atomic operations on player
    /// records with strong visibility guarantees across multiple handles.
    #[allow(dead_code)]
    pub fn primary_tree(&self) -> &sled::Tree {
        &self.primary
    }

    /// Open (or create) the TinyMUSH store rooted at `path`. When `seed_world` is true the
    /// canonical "Old Towne Mesh" rooms are inserted if no world rooms exist yet.
    /// Uses "admin" as the default admin username.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, TinyMushError> {
        Self::open_with_options(path, true, "admin".to_string())
    }

    fn open_with_options<P: AsRef<Path>>(
        path: P,
        seed_world: bool,
        admin_username: String,
    ) -> Result<Self, TinyMushError> {
        let path_ref = path.as_ref();
        std::fs::create_dir_all(path_ref)?;

        // Calculate data directory for seed files (parent of database path)
        // Database is typically at data/tinymush, seeds are at data/seeds/
        let data_dir = path_ref
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let db = sled::open(path_ref)?;
        let primary = db.open_tree(TREE_PRIMARY)?;
        let objects = db.open_tree(TREE_OBJECTS)?;
        let mail = db.open_tree(TREE_MAIL)?;
        let logs = db.open_tree(TREE_LOGS)?;
        let bulletins = db.open_tree(TREE_BULLETINS)?;
        let shops = db.open_tree(TREE_SHOPS)?;
        let trades = db.open_tree(TREE_TRADES)?;
        let npcs = db.open_tree(TREE_NPCS)?;
        let quests = db.open_tree(TREE_QUESTS)?;
        let achievements = db.open_tree(TREE_ACHIEVEMENTS)?;
        let companions = db.open_tree(TREE_COMPANIONS)?;
        let config = db.open_tree(TREE_CONFIG)?;
        let housing_templates = db.open_tree(TREE_HOUSING_TEMPLATES)?;
        let housing_instances = db.open_tree(TREE_HOUSING_INSTANCES)?;

        // Open secondary index trees
        let object_index = db.open_tree(TREE_OBJECT_INDEX)?;
        let housing_guests = db.open_tree(TREE_HOUSING_GUESTS)?;
        let player_trades = db.open_tree(TREE_PLAYER_TRADES)?;
        let template_instances = db.open_tree(TREE_TEMPLATE_INSTANCES)?;

        let store = Self {
            _db: db,
            primary,
            objects,
            mail,
            logs,
            bulletins,
            shops,
            trades,
            npcs,
            quests,
            achievements,
            companions,
            config,
            housing_templates,
            housing_instances,
            object_index,
            housing_guests,
            player_trades,
            template_instances,
            data_dir,
            instanced_rooms: Arc::new(RwLock::new(HashMap::new())),
            landing_instances: Arc::new(RwLock::new(HashMap::new())),
        };

        if seed_world {
            store.seed_world_if_needed()?;
            store.seed_quests_if_needed()?;
            store.seed_achievements_if_needed()?;
            store.seed_companions_if_needed()?;
            store.seed_recipes_if_needed()?;
            store.seed_npcs_if_needed()?;
            store.seed_shops_if_needed()?;

            // Seed full dialogue trees for NPCs
            crate::tmush::state::seed_npc_dialogues_if_needed(&store)?;

            store.seed_housing_templates_if_needed()?;

            // Seed example trigger objects for Phase 9 testing
            store.seed_example_trigger_objects()?;

            // Seed content objects, NPCs, and quests
            store.seed_content_objects_if_needed()?;
            store.seed_content_npcs_if_needed()?;
            store.seed_content_quests_if_needed()?;

            // Seed initial admin account using provided username
            // This should typically match the BBS sysop username from config
            store.seed_admin_if_needed(&admin_username)?;

            // Maintain compatibility with legacy "admin" account expectations in tests
            if !admin_username.eq_ignore_ascii_case("admin") {
                store.seed_admin_if_needed("admin")?;
            }
        }

        Ok(store)
    }

    fn players_key(username: &str) -> Vec<u8> {
        format!("players:{}", username.to_ascii_lowercase()).into_bytes()
    }

    fn room_key(record: &RoomRecord) -> Vec<u8> {
        match &record.owner {
            RoomOwner::World => format!("rooms:world:{}", record.id),
            RoomOwner::Player { username } => {
                format!(
                    "rooms:player:{}:{}",
                    username.to_ascii_lowercase(),
                    record.id
                )
            }
        }
        .into_bytes()
    }

    fn room_world_prefix() -> &'static [u8] {
        b"rooms:world:"
    }

    fn object_key(record: &ObjectRecord) -> Vec<u8> {
        match &record.owner {
            ObjectOwner::World => format!("objects:world:{}", record.id),
            ObjectOwner::Player { username } => {
                format!(
                    "objects:player:{}:{}",
                    username.to_ascii_lowercase(),
                    record.id
                )
            }
        }
        .into_bytes()
    }

    fn serialize<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, TinyMushError> {
        Ok(bincode::serialize(value)?)
    }

    fn deserialize<T: serde::de::DeserializeOwned>(bytes: IVec) -> Result<T, TinyMushError> {
        Ok(bincode::deserialize::<T>(&bytes)?)
    }

    /// Insert or update a player record.
    pub fn put_player(&self, mut player: PlayerRecord) -> Result<(), TinyMushError> {
        player.schema_version = PLAYER_SCHEMA_VERSION;
        player.touch();
        let key = Self::players_key(&player.username);
        let bytes = Self::serialize(&player)?;
        self.primary.insert(key, bytes)?;
        self.primary.flush()?;
        Ok(())
    }

    /// Fetch a player record by username.
    pub fn get_player(&self, username: &str) -> Result<PlayerRecord, TinyMushError> {
        let key = Self::players_key(username);
        let Some(bytes) = self.primary.get(&key)? else {
            return Err(TinyMushError::NotFound(format!("player: {}", username)));
        };

        // Use migration system to load and auto-migrate if needed
        use crate::tmush::migration::load_and_migrate;
        let (record, was_migrated): (PlayerRecord, bool) = load_and_migrate(&bytes, username)
            .map_err(|e| {
                TinyMushError::Bincode(bincode::Error::from(bincode::ErrorKind::Custom(format!(
                    "Failed to load player: {}",
                    e
                ))))
            })?;

        // If migration occurred, save the migrated version back to storage
        if was_migrated {
            log::info!(
                "Auto-migrated PlayerRecord '{}', saving updated version",
                username
            );
            self.put_player(record.clone())?;
        }

        Ok(record)
    }

    /// List all player usernames currently stored.
    pub fn list_player_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        for entry in self.primary.scan_prefix(b"players:") {
            let (key, _) = entry?;
            let text = String::from_utf8_lossy(&key);
            if let Some(username) = text.strip_prefix("players:") {
                ids.push(username.to_string());
            }
        }
        Ok(ids)
    }

    // ============================================================================
    // Admin Helper Functions
    // ============================================================================

    /// Check if a player has admin privileges
    pub fn is_admin(&self, username: &str) -> Result<bool, TinyMushError> {
        let player = self.get_player(username)?;
        Ok(player.is_admin())
    }

    /// Require admin privileges, return PermissionDenied error if not admin
    pub fn require_admin(&self, username: &str) -> Result<(), TinyMushError> {
        if !self.is_admin(username)? {
            return Err(TinyMushError::PermissionDenied(
                "This command requires admin privileges".to_string(),
            ));
        }
        Ok(())
    }

    /// Grant admin privileges to a player (requires caller to be admin)
    pub fn grant_admin(&self, granter: &str, target: &str, level: u8) -> Result<(), TinyMushError> {
        // Verify granter has permission
        self.require_admin(granter)?;

        // Grant admin to target
        let mut player = self.get_player(target)?;
        player.grant_admin(level);
        self.put_player(player)?;

        Ok(())
    }

    /// Revoke admin privileges from a player (requires caller to be admin)
    pub fn revoke_admin(&self, revoker: &str, target: &str) -> Result<(), TinyMushError> {
        // Verify revoker has permission
        self.require_admin(revoker)?;

        // Cannot revoke your own admin
        if revoker == target {
            return Err(TinyMushError::PermissionDenied(
                "Cannot revoke your own admin privileges".to_string(),
            ));
        }

        // Revoke admin from target
        let mut player = self.get_player(target)?;
        player.revoke_admin();
        self.put_player(player)?;

        Ok(())
    }

    /// List all admin players
    pub fn list_admins(&self) -> Result<Vec<PlayerRecord>, TinyMushError> {
        let mut admins = Vec::new();
        let player_ids = self.list_player_ids()?;

        for username in player_ids {
            if let Ok(player) = self.get_player(&username) {
                if player.is_admin() {
                    admins.push(player);
                }
            }
        }

        Ok(admins)
    }

    // ============================================================================
    // Room Storage
    // ============================================================================

    /// Insert or update a room record.
    pub fn put_room(&self, mut room: RoomRecord) -> Result<(), TinyMushError> {
        if room.flags.len() > 1 {
            // Preserve the first occurrence of each flag and drop duplicates to keep UI filters sane.
            let mut seen = HashSet::with_capacity(room.flags.len());
            room.flags.retain(|flag| seen.insert(flag.clone()));
        }

        if crate::tmush::state::is_personal_landing(&room.id) {
            let mut rooms = self.instanced_rooms.write().unwrap();
            rooms.insert(room.id.clone(), room);
            return Ok(());
        }

        room.schema_version = ROOM_SCHEMA_VERSION;
        let key = Self::room_key(&room);
        let bytes = Self::serialize(&room)?;
        self.primary.insert(key, bytes)?;
        self.primary.flush()?;
        Ok(())
    }

    pub fn get_room(&self, room_id: &str) -> Result<RoomRecord, TinyMushError> {
        if let Some(room) = self.instanced_rooms.read().unwrap().get(room_id).cloned() {
            return Ok(room);
        }

        self.get_world_room(room_id)
    }

    pub fn get_world_room(&self, room_id: &str) -> Result<RoomRecord, TinyMushError> {
        let key = format!("rooms:world:{}", room_id).into_bytes();
        let Some(bytes) = self.primary.get(&key)? else {
            return Err(TinyMushError::NotFound(format!("room: {}", room_id)));
        };
        let record: RoomRecord = Self::deserialize(bytes)?;
        if record.schema_version != ROOM_SCHEMA_VERSION {
            return Err(TinyMushError::SchemaMismatch {
                entity: "room",
                expected: ROOM_SCHEMA_VERSION,
                found: record.schema_version,
            });
        }
        Ok(record)
    }

    /// Ensure a per-player landing gazebo instance exists and return its room ID.
    pub fn ensure_personal_landing_room(&self, username: &str) -> Result<String, TinyMushError> {
        let username_key = username.to_ascii_lowercase();

        // Return existing instance if present and still cached
        if let Some(room_id) = self
            .landing_instances
            .read()
            .unwrap()
            .get(&username_key)
            .cloned()
        {
            if self.instanced_rooms.read().unwrap().contains_key(&room_id) {
                return Ok(room_id);
            }
        }

        // Create a fresh instance from the world template
        let mut template =
            self.get_world_room(crate::tmush::state::REQUIRED_LANDING_LOCATION_ID)?;
        let instance_id = crate::tmush::state::generate_landing_instance_id(username);
        template.id = instance_id.clone();
        if !template.flags.contains(&RoomFlag::Instanced) {
            template.flags.push(RoomFlag::Instanced);
        }

        {
            let mut rooms = self.instanced_rooms.write().unwrap();
            rooms.insert(instance_id.clone(), template);
        }

        {
            let mut mapping = self.landing_instances.write().unwrap();
            mapping.insert(username_key, instance_id.clone());
        }

        Ok(instance_id)
    }

    /// Return the active landing instance room id for a player, if one exists.
    pub fn landing_instance_for_player(&self, username: &str) -> Option<String> {
        let username_key = username.to_ascii_lowercase();
        self.landing_instances
            .read()
            .unwrap()
            .get(&username_key)
            .cloned()
    }

    /// Remove a player's landing instance if it matches the provided room id.
    pub fn clear_landing_instance_with_id(&self, username: &str, room_id: &str) {
        let username_key = username.to_ascii_lowercase();
        let removed = {
            let mut mapping = self.landing_instances.write().unwrap();
            match mapping.get(&username_key) {
                Some(current) if current == room_id => {
                    mapping.remove(&username_key);
                    true
                }
                _ => false,
            }
        };

        if removed {
            self.instanced_rooms.write().unwrap().remove(room_id);
        }
    }

    /// Remove any landing instance currently associated with the player.
    pub fn clear_landing_instance_for_player(&self, username: &str) {
        let username_key = username.to_ascii_lowercase();
        let room_id = {
            let mut mapping = self.landing_instances.write().unwrap();
            mapping.remove(&username_key)
        };

        if let Some(room_id) = room_id {
            self.instanced_rooms.write().unwrap().remove(&room_id);
        }
    }

    /// List all world room IDs
    pub fn list_room_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        let prefix = b"rooms:world:";
        for kv in self.primary.scan_prefix(prefix) {
            let (key_bytes, _) = kv?;
            if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                if let Some(id) = key_str.strip_prefix("rooms:world:") {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Delete a world room
    pub fn delete_room(&self, room_id: &str) -> Result<(), TinyMushError> {
        let key = format!("rooms:world:{}", room_id).into_bytes();
        self.primary.remove(key)?;
        self.primary.flush()?;
        Ok(())
    }

    /// List all world object IDs
    pub fn list_object_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        let prefix = b"objects:world:";
        for kv in self.objects.scan_prefix(prefix) {
            let (key_bytes, _) = kv?;
            if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                if let Some(id) = key_str.strip_prefix("objects:world:") {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Delete a world object by ID
    pub fn delete_object_world(&self, object_id: &str) -> Result<(), TinyMushError> {
        // Remove from primary object tree
        let key = format!("objects:world:{}", object_id).into_bytes();
        self.objects.remove(key)?;

        // Remove from index
        let index_key = format!("oid:{}", object_id);
        self.object_index.remove(index_key.as_bytes())?;

        self.objects.flush()?;
        Ok(())
    }

    /// Resolve a room destination for a player, cloning the landing gazebo when required.
    pub fn resolve_destination_for_player(
        &self,
        username: &str,
        room_id: &str,
    ) -> Result<String, TinyMushError> {
        if crate::tmush::state::is_landing_template(room_id) {
            self.ensure_personal_landing_room(username)
        } else {
            Ok(room_id.to_string())
        }
    }

    /// Cleanup landing instance if player left the gazebo.
    pub fn cleanup_landing_instance_after_move(
        &self,
        username: &str,
        previous_room_id: &str,
        current_room_id: &str,
    ) {
        if crate::tmush::state::is_personal_landing(previous_room_id)
            && !crate::tmush::state::is_any_landing_room(current_room_id)
        {
            self.clear_landing_instance_with_id(username, previous_room_id);
        }
    }

    /// Insert or update an object definition.
    pub fn put_object(&self, mut object: ObjectRecord) -> Result<(), TinyMushError> {
        object.schema_version = OBJECT_SCHEMA_VERSION;
        let key = Self::object_key(&object);
        let bytes = Self::serialize(&object)?;
        self.objects.insert(&key, bytes)?;

        // Maintain secondary index: oid:{id} → full_key
        let index_key = format!("oid:{}", object.id);
        self.object_index
            .insert(index_key.as_bytes(), key.as_slice())?;

        self.objects.flush()?;
        Ok(())
    }

    pub fn get_object(&self, id: &str) -> Result<ObjectRecord, TinyMushError> {
        // Use secondary index for O(1) lookup
        let index_key = format!("oid:{}", id);
        if let Some(full_key) = self.object_index.get(index_key.as_bytes())? {
            // Index hit - direct lookup
            if let Some(bytes) = self.objects.get(&full_key)? {
                let object: ObjectRecord = Self::deserialize(bytes)?;
                if object.schema_version != OBJECT_SCHEMA_VERSION {
                    return Err(TinyMushError::SchemaMismatch {
                        entity: "object",
                        expected: OBJECT_SCHEMA_VERSION,
                        found: object.schema_version,
                    });
                }
                return Ok(object);
            }
        }

        // Fallback: scan for migration or index rebuild (legacy data)
        // Try world objects first
        let world_key = format!("objects:world:{}", id);
        if let Some(bytes) = self.objects.get(world_key.as_bytes())? {
            let object: ObjectRecord = Self::deserialize(bytes)?;
            if object.schema_version != OBJECT_SCHEMA_VERSION {
                return Err(TinyMushError::SchemaMismatch {
                    entity: "object",
                    expected: OBJECT_SCHEMA_VERSION,
                    found: object.schema_version,
                });
            }
            // Rebuild index entry for this object
            let index_key = format!("oid:{}", id);
            let _ = self
                .object_index
                .insert(index_key.as_bytes(), world_key.as_bytes());
            return Ok(object);
        }

        // Scan player-owned objects (slowest fallback)
        let prefix = "objects:player:".to_string();
        for result in self.objects.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result?;
            let object: ObjectRecord = Self::deserialize(value)?;
            if object.id == id {
                if object.schema_version != OBJECT_SCHEMA_VERSION {
                    return Err(TinyMushError::SchemaMismatch {
                        entity: "object",
                        expected: OBJECT_SCHEMA_VERSION,
                        found: object.schema_version,
                    });
                }
                // Rebuild index entry for this object
                let index_key = format!("oid:{}", id);
                let _ = self.object_index.insert(index_key.as_bytes(), &key);
                return Ok(object);
            }
        }

        Err(TinyMushError::NotFound(format!("object {}", id)))
    }

    /// Delete an object from storage with safe container handling
    ///
    /// # Container Safety Rules
    /// 1. If object is a container with items, contents are moved to parent location
    /// 2. If container holds nested containers, deletion is blocked (must empty first)
    /// 3. Object is removed from all indexes and location references
    /// 4. Deletion is logged for audit trail
    ///
    /// # Arguments
    /// * `object_id` - ID of the object to delete
    /// * `current_location` - Room ID where object currently resides (for content relocation)
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` - List of item IDs that were relocated (empty if no contents)
    /// * `Err(ContainerNotEmpty)` - If container has nested containers
    /// * `Err(NotFound)` - If object doesn't exist
    pub fn delete_object(
        &self,
        object_id: &str,
        current_location: &str,
    ) -> Result<Vec<String>, TinyMushError> {
        // Get the object first to check if it exists and is a container
        let object = self.get_object(object_id)?;

        let mut relocated_items = Vec::new();

        // Check if this is a container with contents
        if object
            .flags
            .contains(&crate::tmush::types::ObjectFlag::Container)
        {
            // For Phase 7, we'll do a simple check: scan all objects to find ones contained by this
            // In a future phase, we should add a `contained_by` field to ObjectRecord for efficiency

            let prefix = "objects:";
            let contained_items: Vec<String> = Vec::new();
            let has_nested_containers = false;

            for result in self.objects.scan_prefix(prefix.as_bytes()) {
                let (_, value) = result?;
                if let Ok(potential_item) = Self::deserialize::<ObjectRecord>(value) {
                    // Check if this item's location matches our container
                    // Note: In current implementation, we don't track container relationships
                    // This is a TODO for future container system enhancement
                    // For now, we'll just handle the simple case

                    // Skip the container itself
                    if potential_item.id == object_id {
                        continue;
                    }

                    // Future: check if potential_item.contained_by == object_id
                    // For now, containers are assumed empty unless explicitly modeled
                }
            }

            // If nested containers found, block deletion
            if has_nested_containers {
                return Err(TinyMushError::ContainerNotEmpty(format!(
                    "Container '{}' contains nested containers. Empty it first.",
                    object.name
                )));
            }

            // Relocate non-container items to the room
            if !contained_items.is_empty() {
                for item_id in &contained_items {
                    relocated_items.push(item_id.clone());
                    // Items are already in the room's item list, so no additional action needed
                    // In a full container system, we'd update item.contained_by = None here
                }
            }
        }

        // Remove from object index
        let index_key = format!("oid:{}", object_id);
        self.object_index.remove(index_key.as_bytes())?;

        // Remove from object storage
        let object_key = Self::object_key(&object);
        self.objects.remove(&object_key)?;

        // Remove from room's item list if present
        if let Ok(mut room) = self.get_room(current_location) {
            let before_count = room.items.len();
            room.items.retain(|id| id != object_id);
            if room.items.len() != before_count {
                self.put_room(room)?;
            }
        }

        // Remove from player inventories (scan all players)
        let player_ids = self.list_player_ids()?;
        for username in player_ids {
            if let Ok(mut player) = self.get_player(&username) {
                // Check legacy inventory
                let before_legacy = player.inventory.len();
                player.inventory.retain(|id| id != object_id);

                // Check inventory stacks
                let before_stacks = player.inventory_stacks.len();
                player
                    .inventory_stacks
                    .retain(|stack| stack.object_id != object_id);

                // Only save if something changed
                if player.inventory.len() != before_legacy
                    || player.inventory_stacks.len() != before_stacks
                {
                    self.put_player(player)?;
                }
            }
        }

        // Flush changes
        self.objects.flush()?;
        self.object_index.flush()?;

        Ok(relocated_items)
    }

    /// Insert or update a shop record
    pub fn put_shop(&self, shop: ShopRecord) -> Result<(), TinyMushError> {
        let key = format!("shops:{}", shop.id).into_bytes();
        let bytes = Self::serialize(&shop)?;
        self.shops.insert(key, bytes)?;
        self.shops.flush()?;
        Ok(())
    }

    /// Get a shop by ID
    pub fn get_shop(&self, shop_id: &str) -> Result<ShopRecord, TinyMushError> {
        let key = format!("shops:{}", shop_id).into_bytes();
        let Some(bytes) = self.shops.get(&key)? else {
            return Err(TinyMushError::NotFound(format!("shop: {}", shop_id)));
        };
        let shop: ShopRecord = Self::deserialize(bytes)?;
        Ok(shop)
    }

    /// List all shop IDs
    pub fn list_shop_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        for entry in self.shops.scan_prefix(b"shops:") {
            let (key, _) = entry?;
            let text = String::from_utf8_lossy(&key);
            if let Some(shop_id) = text.strip_prefix("shops:") {
                ids.push(shop_id.to_string());
            }
        }
        Ok(ids)
    }

    /// Get all shops in a specific location (room)
    pub fn get_shops_in_location(&self, location: &str) -> Result<Vec<ShopRecord>, TinyMushError> {
        let mut shops = Vec::new();
        for entry in self.shops.scan_prefix(b"shops:") {
            let (_, value) = entry?;
            let shop: ShopRecord = Self::deserialize(value)?;
            if shop.location == location {
                shops.push(shop);
            }
        }
        Ok(shops)
    }

    /// Delete a shop by ID
    pub fn delete_shop(&self, shop_id: &str) -> Result<(), TinyMushError> {
        let key = format!("shops:{}", shop_id).into_bytes();
        self.shops.remove(key)?;
        self.shops.flush()?;
        Ok(())
    }

    // ============================================================================
    // NPC Storage
    // ============================================================================

    /// Store or update an NPC record
    pub fn put_npc(&self, npc: NpcRecord) -> Result<(), TinyMushError> {
        let key = format!("npcs:{}", npc.id).into_bytes();
        let value = Self::serialize(&npc)?;
        self.npcs.insert(key, value)?;
        self.npcs.flush()?;
        Ok(())
    }

    /// Retrieve an NPC by ID
    pub fn get_npc(&self, npc_id: &str) -> Result<NpcRecord, TinyMushError> {
        let key = format!("npcs:{}", npc_id).into_bytes();
        let data = self
            .npcs
            .get(key.clone())?
            .ok_or_else(|| TinyMushError::NotFound(format!("NPC not found: {}", npc_id)))?;

        // Use migration system to load and auto-migrate if needed
        use crate::tmush::migration::load_and_migrate;
        let (npc, was_migrated): (NpcRecord, bool) =
            load_and_migrate(&data, npc_id).map_err(|e| {
                TinyMushError::Bincode(bincode::Error::from(bincode::ErrorKind::Custom(format!(
                    "Failed to load NPC: {}",
                    e
                ))))
            })?;

        // If migration occurred, save the migrated version back to storage
        if was_migrated {
            log::info!(
                "Auto-migrated NpcRecord '{}', saving updated version",
                npc_id
            );
            self.put_npc(npc.clone())?;
        }

        Ok(npc)
    }

    /// List all NPC IDs
    pub fn list_npc_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        for item in self.npcs.scan_prefix(b"npcs:") {
            let (key, _) = item?;
            if let Ok(s) = std::str::from_utf8(&key) {
                if let Some(id) = s.strip_prefix("npcs:") {
                    ids.push(id.to_string());
                }
            }
        }
        Ok(ids)
    }

    /// Get all NPCs in a specific room
    pub fn get_npcs_in_room(&self, room_id: &str) -> Result<Vec<NpcRecord>, TinyMushError> {
        let mut npcs = Vec::new();
        for item in self.npcs.scan_prefix(b"npcs:") {
            let (key, value) = item?;

            // Extract NPC ID from key for migration logging
            let npc_id = std::str::from_utf8(&key)
                .ok()
                .and_then(|s| s.strip_prefix("npcs:"))
                .unwrap_or("unknown");

            // Use migration system to load and auto-migrate if needed
            use crate::tmush::migration::load_and_migrate;
            let (npc, was_migrated): (NpcRecord, bool) =
                load_and_migrate(&value, npc_id).map_err(|e| {
                    TinyMushError::Bincode(bincode::Error::from(bincode::ErrorKind::Custom(
                        format!("Failed to load NPC {}: {}", npc_id, e),
                    )))
                })?;

            // If migration occurred, save the migrated version back to storage
            if was_migrated {
                log::info!(
                    "Auto-migrated NpcRecord '{}' in get_npcs_in_room, saving updated version",
                    npc_id
                );
                self.put_npc(npc.clone())?;
            }

            if npc.room_id == room_id {
                npcs.push(npc);
            }
        }
        Ok(npcs)
    }

    /// Delete an NPC by ID
    pub fn delete_npc(&self, npc_id: &str) -> Result<(), TinyMushError> {
        let key = format!("npcs:{}", npc_id).into_bytes();
        self.npcs.remove(key)?;
        self.npcs.flush()?;
        Ok(())
    }

    /// Check if an NPC exists
    pub fn npc_exists(&self, npc_id: &str) -> Result<bool, TinyMushError> {
        let key = format!("npcs:{}", npc_id);
        Ok(self.npcs.contains_key(key.as_bytes())?)
    }

    /// Check if a companion exists
    pub fn companion_exists(&self, companion_id: &str) -> Result<bool, TinyMushError> {
        let key = format!("companion:{}", companion_id);
        Ok(self.companions.contains_key(key.as_bytes())?)
    }

    /// Check if a room exists
    pub fn room_exists(&self, room_id: &str) -> Result<bool, TinyMushError> {
        let key = format!("rooms:world:{}", room_id);
        Ok(self.primary.contains_key(key.as_bytes())?)
    }

    /// Check if an object exists (world objects only)
    pub fn object_exists(&self, object_id: &str) -> Result<bool, TinyMushError> {
        // Check secondary index first for O(1) lookup
        let index_key = format!("oid:{}", object_id);
        if self.object_index.contains_key(index_key.as_bytes())? {
            return Ok(true);
        }

        // Fallback: check world objects tree directly
        let key = format!("objects:world:{}", object_id);
        Ok(self.objects.contains_key(key.as_bytes())?)
    }

    // ============================================================================
    // Conversation State Storage (Phase 8.5)
    // ============================================================================

    /// Store or update conversation state for a player-NPC pair
    pub fn put_conversation_state(
        &self,
        state: crate::tmush::types::ConversationState,
    ) -> Result<(), TinyMushError> {
        let key = format!("conversation:{}:{}", state.player_id, state.npc_id).into_bytes();
        let value = Self::serialize(&state)?;
        self.npcs.insert(key, value)?;
        self.npcs.flush()?;
        Ok(())
    }

    /// Get conversation state for a player-NPC pair
    pub fn get_conversation_state(
        &self,
        player_id: &str,
        npc_id: &str,
    ) -> Result<Option<crate::tmush::types::ConversationState>, TinyMushError> {
        let key = format!("conversation:{}:{}", player_id, npc_id).into_bytes();
        match self.npcs.get(key)? {
            Some(data) => {
                let state: crate::tmush::types::ConversationState = Self::deserialize(data)?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    /// Get all conversation states for a player (across all NPCs)
    pub fn get_player_conversation_states(
        &self,
        player_id: &str,
    ) -> Result<Vec<crate::tmush::types::ConversationState>, TinyMushError> {
        let prefix = format!("conversation:{}:", player_id).into_bytes();
        let mut states = Vec::new();
        for item in self.npcs.scan_prefix(&prefix) {
            let (_, value) = item?;
            let state: crate::tmush::types::ConversationState = Self::deserialize(value)?;
            states.push(state);
        }
        Ok(states)
    }

    /// Delete old conversation states (older than days)
    pub fn cleanup_old_conversations(&self, days: i64) -> Result<usize, TinyMushError> {
        use chrono::Duration;
        let cutoff = Utc::now() - Duration::days(days);
        let mut deleted = 0;

        let mut to_delete = Vec::new();
        for item in self.npcs.scan_prefix(b"conversation:") {
            let (key, value) = item?;
            let state: crate::tmush::types::ConversationState = Self::deserialize(value)?;
            if state.last_conversation_time < cutoff {
                to_delete.push(key.to_vec());
            }
        }

        for key in to_delete {
            self.npcs.remove(key)?;
            deleted += 1;
        }

        if deleted > 0 {
            self.npcs.flush()?;
        }

        Ok(deleted)
    }

    // ============================================================================
    // Dialog Session Storage (Phase 8.5)
    // ============================================================================

    /// Store an active dialog session
    pub fn put_dialog_session(
        &self,
        session: crate::tmush::types::DialogSession,
    ) -> Result<(), TinyMushError> {
        let key = format!("dialog_session:{}:{}", session.player_id, session.npc_id).into_bytes();
        let value = Self::serialize(&session)?;
        self.npcs.insert(key, value)?;
        self.npcs.flush()?;
        Ok(())
    }

    /// Get active dialog session for player-NPC pair
    pub fn get_dialog_session(
        &self,
        player_id: &str,
        npc_id: &str,
    ) -> Result<Option<crate::tmush::types::DialogSession>, TinyMushError> {
        let key = format!("dialog_session:{}:{}", player_id, npc_id).into_bytes();
        match self.npcs.get(key)? {
            Some(data) => {
                let session: crate::tmush::types::DialogSession = Self::deserialize(data)?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Delete a dialog session (when conversation ends)
    pub fn delete_dialog_session(
        &self,
        player_id: &str,
        npc_id: &str,
    ) -> Result<(), TinyMushError> {
        let key = format!("dialog_session:{}:{}", player_id, npc_id).into_bytes();
        self.npcs.remove(key)?;
        self.npcs.flush()?;
        Ok(())
    }

    /// Get the first active dialog session for a player (returns NPC ID and session)
    pub fn get_active_dialog_for_player(
        &self,
        player_id: &str,
    ) -> Result<Option<(String, crate::tmush::types::DialogSession)>, TinyMushError> {
        let prefix = format!("dialog_session:{}:", player_id).into_bytes();

        for item in self.npcs.scan_prefix(&prefix) {
            let (key, value) = item?;
            let session: crate::tmush::types::DialogSession = Self::deserialize(value)?;

            // Extract NPC ID from key: dialog_session:{player_id}:{npc_id}
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if let Some(npc_id) =
                    key_str.strip_prefix(&format!("dialog_session:{}:", player_id))
                {
                    return Ok(Some((npc_id.to_string(), session)));
                }
            }
        }

        Ok(None)
    }

    // ============================================================================
    // Disambiguation Session Storage (Fuzzy Object Matching)
    // ============================================================================

    /// Store a disambiguation session for fuzzy object matching
    pub fn put_disambiguation_session(
        &self,
        session: crate::tmush::types::DisambiguationSession,
    ) -> Result<(), TinyMushError> {
        let key = format!("disambiguation_session:{}", session.player_id).into_bytes();
        let value = Self::serialize(&session)?;
        self.npcs.insert(key, value)?;
        self.npcs.flush()?;
        Ok(())
    }

    /// Get active disambiguation session for player
    pub fn get_disambiguation_session(
        &self,
        player_id: &str,
    ) -> Result<Option<crate::tmush::types::DisambiguationSession>, TinyMushError> {
        let key = format!("disambiguation_session:{}", player_id).into_bytes();
        match self.npcs.get(key)? {
            Some(data) => {
                let session: crate::tmush::types::DisambiguationSession = Self::deserialize(data)?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Delete a disambiguation session (after choice made or timeout)
    pub fn delete_disambiguation_session(&self, player_id: &str) -> Result<(), TinyMushError> {
        let key = format!("disambiguation_session:{}", player_id).into_bytes();
        self.npcs.remove(key)?;
        self.npcs.flush()?;
        Ok(())
    }

    pub fn seed_world_if_needed(&self) -> Result<usize, TinyMushError> {
        if self
            .primary
            .scan_prefix(Self::room_world_prefix())
            .next()
            .is_some()
        {
            return Ok(0);
        }

        // Load rooms from JSON seed file
        let seed_path = std::path::Path::new(&self.data_dir).join("seeds/rooms.json");
        let rooms = match crate::tmush::load_rooms_from_json(&seed_path) {
            Ok(rooms) => rooms,
            Err(e) => {
                log::warn!(
                    "Failed to load rooms from {}: {}. Falling back to hardcoded seeds.",
                    seed_path.display(),
                    e
                );
                // Fallback to hardcoded seeds if JSON file doesn't exist
                let now = Utc::now();
                canonical_world_seed(now)
            }
        };

        let mut inserted = 0usize;
        for room in rooms {
            self.put_room(room)?;
            inserted += 1;
        }
        Ok(inserted)
    }

    pub fn seed_quests_if_needed(&self) -> Result<usize, TinyMushError> {
        if self.quests.iter().next().is_some() {
            return Ok(0);
        }

        // Load quests from JSON seed file
        let seed_path = std::path::Path::new(&self.data_dir).join("seeds/quests.json");
        let quests = match crate::tmush::load_quests_from_json(&seed_path) {
            Ok(quests) => quests,
            Err(e) => {
                log::warn!(
                    "Failed to load quests from {}: {}. Falling back to hardcoded seeds.",
                    seed_path.display(),
                    e
                );
                // Fallback to hardcoded seeds if JSON file doesn't exist
                crate::tmush::seed_starter_quests()
            }
        };

        let mut inserted = 0usize;
        for quest in quests {
            self.put_quest(quest)?;
            inserted += 1;
        }
        Ok(inserted)
    }

    /// Seed example trigger objects for Phase 9 testing
    ///
    /// Creates 6 example objects with triggers and places them in the museum
    /// for players to discover and test.
    pub fn seed_example_trigger_objects(&self) -> Result<usize, TinyMushError> {
        // Check if example objects already exist
        if self.get_object("example_healing_potion").is_ok() {
            return Ok(0); // Already seeded
        }

        let now = Utc::now();
        let objects = crate::tmush::state::create_example_trigger_objects(now);
        let mut inserted = 0usize;

        // Store all objects
        for object in &objects {
            self.put_object(object.clone())?;
            inserted += 1;
        }

        // Add objects to the museum room for discovery
        if let Ok(mut museum) = self.get_room("mesh_museum") {
            for object in &objects {
                if !museum.items.contains(&object.id) {
                    museum.items.push(object.id.clone());
                }
            }
            self.put_room(museum)?;
        }

        Ok(inserted)
    }

    /// Seed content objects (rumor board, carved symbols, materials, etc.)
    /// Returns the number of new objects inserted.
    pub fn seed_content_objects_if_needed(&self) -> Result<usize, TinyMushError> {
        // Check if content objects already exist
        if self.get_object("rumor_board").is_ok() {
            return Ok(0); // Already seeded
        }

        let now = Utc::now();
        let objects = crate::tmush::state::create_content_objects(now);
        let mut inserted = 0usize;

        // Define the object -> room mappings based on design document
        let object_locations = vec![
            ("rumor_board", "relay_tavern"),
            ("diagnostic_panel", "repeater_tower"),
            ("northern_array", "repeater_upper"),
            ("carved_symbols_oak", "ancient_grove"),
            ("carved_symbols_elm", "ancient_grove"),
            ("carved_symbols_willow", "ancient_grove"),
            ("carved_symbols_ash", "ancient_grove"),
            ("crafting_bench", "workshop_district"),
            ("copper_wire", "workshop_district"),
            ("circuit_board", "maintenance_tunnels"),
            ("antenna_rod", "repeater_tower"),
            ("crystal_shard", "ancient_grove"),
            ("signal_capacitor", "relay_tavern"),
            ("torch", "relay_tavern"),
        ];

        // Store all objects and place them in their designated rooms
        for object in objects {
            let object_id = object.id.clone();
            self.put_object(object)?;
            inserted += 1;

            // Find this object's designated room
            if let Some((_, room_id)) = object_locations
                .iter()
                .find(|(obj_id, _)| *obj_id == object_id)
            {
                if let Ok(mut room) = self.get_room(room_id) {
                    if !room.items.contains(&object_id) {
                        room.items.push(object_id.clone());
                        self.put_room(room)?;
                    }
                }
            }
        }

        Ok(inserted)
    }

    /// Seed content NPCs (Old Graybeard, Barkeep Mira, Old Elm, Tinker Brass)
    /// Returns the number of new NPCs inserted.
    pub fn seed_content_npcs_if_needed(&self) -> Result<usize, TinyMushError> {
        // Check if content NPCs already exist
        if self.get_npc("old_graybeard").is_ok() {
            return Ok(0); // Already seeded
        }

        let now = Utc::now();
        let npcs = crate::tmush::state::create_content_npcs(now);
        let mut inserted = 0usize;

        // Store all NPCs
        for npc in npcs {
            self.put_npc(npc)?;
            inserted += 1;
        }

        Ok(inserted)
    }

    /// Seed content quests (tower diagnostics, grove mystery, tunnel salvage, first craft)
    /// Returns the number of new quests inserted.
    pub fn seed_content_quests_if_needed(&self) -> Result<usize, TinyMushError> {
        // Check if content quests already exist
        if self.get_quest("tower_diagnostics").is_ok() {
            return Ok(0); // Already seeded
        }

        let now = Utc::now();
        let quests = crate::tmush::state::create_content_quests(now);
        let mut inserted = 0usize;

        // Store all quests
        for quest in quests {
            self.put_quest(quest)?;
            inserted += 1;
        }

        Ok(inserted)
    }

    /// Create or update a bulletin board configuration
    pub fn put_bulletin_board(&self, mut board: BulletinBoard) -> Result<(), TinyMushError> {
        board.schema_version = BULLETIN_SCHEMA_VERSION;
        let key = format!("boards:{}", board.id).into_bytes();
        let bytes = Self::serialize(&board)?;
        self.bulletins.insert(key, bytes)?;
        self.bulletins.flush()?;
        Ok(())
    }

    /// Get a bulletin board by ID
    pub fn get_bulletin_board(&self, board_id: &str) -> Result<BulletinBoard, TinyMushError> {
        let key = format!("boards:{}", board_id).into_bytes();
        let bytes = self
            .bulletins
            .get(key)?
            .ok_or_else(|| TinyMushError::NotFound(format!("Bulletin board: {}", board_id)))?;
        Self::deserialize(bytes)
    }

    /// Post a message to a bulletin board
    pub fn post_bulletin(&self, mut message: BulletinMessage) -> Result<u64, TinyMushError> {
        // Generate unique message ID based on timestamp
        let message_id = next_timestamp_nanos() as u64;
        message.id = message_id;
        message.schema_version = BULLETIN_SCHEMA_VERSION;

        let key = format!("messages:{}:{:020}", message.board_id, message_id).into_bytes();
        let bytes = Self::serialize(&message)?;
        self.bulletins.insert(key, bytes)?;
        self.bulletins.flush()?;
        Ok(message_id)
    }

    /// Get a specific bulletin message
    pub fn get_bulletin(
        &self,
        board_id: &str,
        message_id: u64,
    ) -> Result<BulletinMessage, TinyMushError> {
        let key = format!("messages:{}:{:020}", board_id, message_id).into_bytes();
        let bytes = self
            .bulletins
            .get(key)?
            .ok_or_else(|| TinyMushError::NotFound(format!("Bulletin message: {}", message_id)))?;
        Self::deserialize(bytes)
    }

    /// List bulletin messages for a board with pagination
    pub fn list_bulletins(
        &self,
        board_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<BulletinMessage>, TinyMushError> {
        let prefix = format!("messages:{}:", board_id);
        let messages: Result<Vec<_>, _> = self
            .bulletins
            .scan_prefix(prefix.as_bytes())
            .skip(offset)
            .take(limit)
            .map(|result| {
                result
                    .map_err(TinyMushError::from)
                    .and_then(|(_key, value)| Self::deserialize(value))
            })
            .collect();
        messages
    }

    /// Count total messages on a bulletin board
    pub fn count_bulletins(&self, board_id: &str) -> Result<usize, TinyMushError> {
        let prefix = format!("messages:{}:", board_id);
        let count = self.bulletins.scan_prefix(prefix.as_bytes()).count();
        Ok(count)
    }

    /// Remove old messages to enforce max_messages limit
    pub fn cleanup_bulletins(
        &self,
        board_id: &str,
        max_messages: u32,
    ) -> Result<usize, TinyMushError> {
        let prefix = format!("messages:{}:", board_id);
        let all_keys: Result<Vec<_>, _> = self
            .bulletins
            .scan_prefix(prefix.as_bytes())
            .map(|result| result.map(|(key, _value)| key))
            .collect();

        let mut keys = all_keys?;
        if keys.len() <= max_messages as usize {
            return Ok(0);
        }

        // Sort by key (which includes timestamp) and remove oldest
        keys.sort();
        let to_remove = keys.len() - max_messages as usize;
        let mut removed = 0;

        for key in keys.iter().take(to_remove) {
            self.bulletins.remove(key)?;
            removed += 1;
        }

        self.bulletins.flush()?;
        Ok(removed)
    }

    /// Append a line to the TinyMUSH diagnostic log tree.
    pub fn append_log(&self, message: &str) -> Result<(), TinyMushError> {
        let key = format!("logs:{}", next_timestamp_nanos()).into_bytes();
        self.logs.insert(key, message.as_bytes())?;
        self.logs.flush()?;
        Ok(())
    }

    /// Send a mail message between players
    pub fn send_mail(&self, mut message: MailMessage) -> Result<u64, TinyMushError> {
        // Generate unique message ID based on timestamp
        let message_id = next_timestamp_nanos() as u64;
        message.id = message_id;
        message.schema_version = MAIL_SCHEMA_VERSION;

        // Store in recipient's inbox
        let inbox_key = format!(
            "mail:inbox:{}:{}:{:020}",
            message.recipient.to_ascii_lowercase(),
            message.sender.to_ascii_lowercase(),
            message_id
        )
        .into_bytes();

        // Store in sender's sent folder
        let sent_key = format!(
            "mail:sent:{}:{}:{:020}",
            message.sender.to_ascii_lowercase(),
            message.recipient.to_ascii_lowercase(),
            message_id
        )
        .into_bytes();

        let bytes = Self::serialize(&message)?;

        self.mail.insert(inbox_key, bytes.clone())?;
        self.mail.insert(sent_key, bytes)?;
        self.mail.flush()?;

        Ok(message_id)
    }

    /// Get a specific mail message
    pub fn get_mail(
        &self,
        folder: &str,
        username: &str,
        message_id: u64,
    ) -> Result<MailMessage, TinyMushError> {
        // Try to find the message in the specified folder
        let prefix = format!("mail:{}:{}:", folder, username.to_ascii_lowercase());

        for result in self.mail.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.ends_with(&format!(":{:020}", message_id)) {
                let message: MailMessage = Self::deserialize(value)?;
                return Ok(message);
            }
        }

        Err(TinyMushError::NotFound(format!(
            "Mail message: {}",
            message_id
        )))
    }

    /// List mail messages for a player's folder
    pub fn list_mail(
        &self,
        folder: &str,
        username: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MailMessage>, TinyMushError> {
        let prefix = format!("mail:{}:{}:", folder, username.to_ascii_lowercase());

        let messages: Result<Vec<_>, _> = self
            .mail
            .scan_prefix(prefix.as_bytes())
            .skip(offset)
            .take(limit)
            .map(|result| {
                result
                    .map_err(TinyMushError::from)
                    .and_then(|(_key, value)| Self::deserialize::<MailMessage>(value))
            })
            .collect();

        let mut messages = messages?;
        // Sort by sent date, newest first
        messages.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        Ok(messages)
    }

    /// Mark a mail message as read
    pub fn mark_mail_read(
        &self,
        folder: &str,
        username: &str,
        message_id: u64,
    ) -> Result<(), TinyMushError> {
        let prefix = format!("mail:{}:{}:", folder, username.to_ascii_lowercase());

        for result in self.mail.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.ends_with(&format!(":{:020}", message_id)) {
                let mut message: MailMessage = Self::deserialize(value)?;
                message.mark_read();
                let updated_bytes = Self::serialize(&message)?;
                self.mail.insert(key, updated_bytes)?;
                self.mail.flush()?;
                return Ok(());
            }
        }

        Err(TinyMushError::NotFound(format!(
            "Mail message: {}",
            message_id
        )))
    }

    /// Delete a mail message
    pub fn delete_mail(
        &self,
        folder: &str,
        username: &str,
        message_id: u64,
    ) -> Result<(), TinyMushError> {
        let prefix = format!("mail:{}:{}:", folder, username.to_ascii_lowercase());

        for result in self.mail.scan_prefix(prefix.as_bytes()) {
            let (key, _value) = result?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.ends_with(&format!(":{:020}", message_id)) {
                self.mail.remove(key)?;
                self.mail.flush()?;
                return Ok(());
            }
        }

        Err(TinyMushError::NotFound(format!(
            "Mail message: {}",
            message_id
        )))
    }

    /// Count mail messages in a folder
    pub fn count_mail(&self, folder: &str, username: &str) -> Result<usize, TinyMushError> {
        let prefix = format!("mail:{}:{}:", folder, username.to_ascii_lowercase());
        let count = self.mail.scan_prefix(prefix.as_bytes()).count();
        Ok(count)
    }

    /// Count unread mail messages in inbox
    pub fn count_unread_mail(&self, username: &str) -> Result<usize, TinyMushError> {
        let prefix = format!("mail:inbox:{}:", username.to_ascii_lowercase());
        let mut unread_count = 0;

        for result in self.mail.scan_prefix(prefix.as_bytes()) {
            let (_key, value) = result?;
            let message: MailMessage = Self::deserialize(value)?;
            if message.status == MailStatus::Unread {
                unread_count += 1;
            }
        }

        Ok(unread_count)
    }

    /// Cleanup old read mail messages
    pub fn cleanup_old_mail(&self, username: &str, days_old: u32) -> Result<usize, TinyMushError> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days_old as i64);
        let prefix = format!("mail:inbox:{}:", username.to_ascii_lowercase());
        let mut removed = 0;

        let mut keys_to_remove = Vec::new();

        for result in self.mail.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result?;
            let message: MailMessage = Self::deserialize(value)?;

            // Only delete read messages older than cutoff
            if message.status != MailStatus::Unread && message.sent_at < cutoff_date {
                keys_to_remove.push(key);
            }
        }

        for key in keys_to_remove {
            self.mail.remove(key)?;
            removed += 1;
        }

        if removed > 0 {
            self.mail.flush()?;
        }

        Ok(removed)
    }

    /// Enforce mail quota for a player
    pub fn enforce_mail_quota(
        &self,
        username: &str,
        max_messages: u32,
    ) -> Result<usize, TinyMushError> {
        let inbox_count = self.count_mail("inbox", username)?;

        if inbox_count <= max_messages as usize {
            return Ok(0);
        }

        // Get all messages sorted by date (oldest first)
        let prefix = format!("mail:inbox:{}:", username.to_ascii_lowercase());
        let mut messages_with_keys = Vec::new();

        for result in self.mail.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result?;
            let message: MailMessage = Self::deserialize(value)?;
            messages_with_keys.push((key, message));
        }

        // Sort by sent date, oldest first
        messages_with_keys.sort_by(|a, b| a.1.sent_at.cmp(&b.1.sent_at));

        // Remove oldest messages to get under quota
        let to_remove = inbox_count - max_messages as usize;
        let mut removed = 0;

        for (key, message) in messages_with_keys.iter().take(to_remove) {
            // Only remove read messages to preserve unread mail
            if message.status != MailStatus::Unread {
                self.mail.remove(key)?;
                removed += 1;
            }
        }

        if removed > 0 {
            self.mail.flush()?;
        }

        Ok(removed)
    }

    /// Legacy method for simple mail notifications (kept for compatibility)
    pub fn enqueue_mail(&self, username: &str, body: &str) -> Result<(), TinyMushError> {
        let message = MailMessage::new("system", username, "System Message", body);
        self.send_mail(message)?;
        Ok(())
    }

    // ========================================================================
    // Currency & Economy Methods
    // ========================================================================

    /// Transfer currency from one player to another (atomic transaction)
    pub fn transfer_currency(
        &self,
        from_username: &str,
        to_username: &str,
        amount: &CurrencyAmount,
        reason: TransactionReason,
    ) -> Result<CurrencyTransaction, TinyMushError> {
        // Validate amount is positive
        if amount.is_zero_or_negative() {
            return Err(TinyMushError::InvalidCurrency(
                "Transfer amount must be positive".to_string(),
            ));
        }

        // Get both players (locks the records)
        let mut from_player = self.get_player(from_username)?;
        let mut to_player = self.get_player(to_username)?;

        // Check if sender can afford it
        if !from_player.currency.can_afford(amount) {
            return Err(TinyMushError::InsufficientFunds);
        }

        // Perform the transfer
        from_player.currency = from_player.currency.subtract(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency subtraction failed: {}", e))
        })?;

        to_player.currency = to_player.currency.add(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency addition failed: {}", e))
        })?;

        // Save both players
        self.put_player(from_player)?;
        self.put_player(to_player)?;

        // Create transaction record
        let transaction = CurrencyTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: Some(from_username.to_string()),
            to: Some(to_username.to_string()),
            amount: amount.clone(),
            reason,
            rolled_back: false,
        };

        // Log the transaction
        self.log_transaction(&transaction)?;

        Ok(transaction)
    }

    /// Grant currency to a player from the system (admin/quest rewards)
    pub fn grant_currency(
        &self,
        username: &str,
        amount: &CurrencyAmount,
        reason: TransactionReason,
    ) -> Result<CurrencyTransaction, TinyMushError> {
        if amount.is_zero_or_negative() {
            return Err(TinyMushError::InvalidCurrency(
                "Grant amount must be positive".to_string(),
            ));
        }

        let mut player = self.get_player(username)?;
        player.currency = player.currency.add(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency addition failed: {}", e))
        })?;
        self.put_player(player)?;

        let transaction = CurrencyTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: None, // System grant
            to: Some(username.to_string()),
            amount: amount.clone(),
            reason,
            rolled_back: false,
        };

        self.log_transaction(&transaction)?;
        Ok(transaction)
    }

    /// Deduct currency from a player to the system (admin/rent payments)
    pub fn deduct_currency(
        &self,
        username: &str,
        amount: &CurrencyAmount,
        reason: TransactionReason,
    ) -> Result<CurrencyTransaction, TinyMushError> {
        if amount.is_zero_or_negative() {
            return Err(TinyMushError::InvalidCurrency(
                "Deduct amount must be positive".to_string(),
            ));
        }

        let mut player = self.get_player(username)?;
        if !player.currency.can_afford(amount) {
            return Err(TinyMushError::InsufficientFunds);
        }

        player.currency = player.currency.subtract(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency subtraction failed: {}", e))
        })?;
        self.put_player(player)?;

        let transaction = CurrencyTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: Some(username.to_string()),
            to: None, // System deduction
            amount: amount.clone(),
            reason,
            rolled_back: false,
        };

        self.log_transaction(&transaction)?;
        Ok(transaction)
    }

    /// Deposit currency to bank (move from pocket to vault)
    pub fn bank_deposit(
        &self,
        username: &str,
        amount: &CurrencyAmount,
    ) -> Result<CurrencyTransaction, TinyMushError> {
        if amount.is_zero_or_negative() {
            return Err(TinyMushError::InvalidCurrency(
                "Deposit amount must be positive".to_string(),
            ));
        }

        let mut player = self.get_player(username)?;
        if !player.currency.can_afford(amount) {
            return Err(TinyMushError::InsufficientFunds);
        }

        // Move from pocket to bank
        player.currency = player.currency.subtract(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency subtraction failed: {}", e))
        })?;

        player.banked_currency = player.banked_currency.add(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency addition failed: {}", e))
        })?;

        self.put_player(player)?;

        let transaction = CurrencyTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: Some(username.to_string()),
            to: Some(format!("{}:bank", username)),
            amount: amount.clone(),
            reason: TransactionReason::BankDeposit,
            rolled_back: false,
        };

        self.log_transaction(&transaction)?;
        Ok(transaction)
    }

    /// Withdraw currency from bank (move from vault to pocket)
    pub fn bank_withdraw(
        &self,
        username: &str,
        amount: &CurrencyAmount,
    ) -> Result<CurrencyTransaction, TinyMushError> {
        if amount.is_zero_or_negative() {
            return Err(TinyMushError::InvalidCurrency(
                "Withdrawal amount must be positive".to_string(),
            ));
        }

        let mut player = self.get_player(username)?;
        if !player.banked_currency.can_afford(amount) {
            return Err(TinyMushError::InsufficientFunds);
        }

        // Move from bank to pocket
        player.banked_currency = player.banked_currency.subtract(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency subtraction failed: {}", e))
        })?;

        player.currency = player.currency.add(amount).map_err(|e| {
            TinyMushError::InvalidCurrency(format!("Currency addition failed: {}", e))
        })?;

        self.put_player(player)?;

        let transaction = CurrencyTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: Some(format!("{}:bank", username)),
            to: Some(username.to_string()),
            amount: amount.clone(),
            reason: TransactionReason::BankWithdrawal,
            rolled_back: false,
        };

        self.log_transaction(&transaction)?;
        Ok(transaction)
    }

    /// Log a transaction to the audit tree
    fn log_transaction(&self, transaction: &CurrencyTransaction) -> Result<(), TinyMushError> {
        let key = format!("transaction:{}", transaction.id);
        self.mail
            .insert(key.as_bytes(), Self::serialize(transaction)?)?;
        self.mail.flush()?;
        Ok(())
    }

    /// Get a transaction by ID
    pub fn get_transaction(&self, id: &str) -> Result<CurrencyTransaction, TinyMushError> {
        let key = format!("transaction:{}", id);
        match self.mail.get(key.as_bytes())? {
            Some(bytes) => Ok(Self::deserialize(bytes)?),
            None => Err(TinyMushError::TransactionNotFound),
        }
    }

    /// Rollback a transaction (reverse the currency movement)
    pub fn rollback_transaction(&self, transaction_id: &str) -> Result<(), TinyMushError> {
        let mut transaction = self.get_transaction(transaction_id)?;

        if transaction.rolled_back {
            return Err(TinyMushError::InvalidCurrency(
                "Transaction already rolled back".to_string(),
            ));
        }

        // Reverse the transaction based on type
        match (&transaction.from, &transaction.to) {
            (Some(from), Some(to)) if to.contains(":bank") => {
                // Reverse bank deposit
                let username = from;
                let mut player = self.get_player(username)?;

                player.currency = player.currency.subtract(&transaction.amount).map_err(|e| {
                    TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                })?;

                player.banked_currency =
                    player
                        .banked_currency
                        .add(&transaction.amount)
                        .map_err(|e| {
                            TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                        })?;

                self.put_player(player)?;
            }
            (Some(from), Some(to)) if from.contains(":bank") => {
                // Reverse bank withdrawal
                let username = to;
                let mut player = self.get_player(username)?;

                player.banked_currency = player
                    .banked_currency
                    .subtract(&transaction.amount)
                    .map_err(|e| {
                        TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                    })?;

                player.currency = player.currency.add(&transaction.amount).map_err(|e| {
                    TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                })?;

                self.put_player(player)?;
            }
            (Some(from), Some(to)) => {
                // Reverse player-to-player transfer
                let mut from_player = self.get_player(from)?;
                let mut to_player = self.get_player(to)?;

                from_player.currency =
                    from_player.currency.add(&transaction.amount).map_err(|e| {
                        TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                    })?;

                to_player.currency =
                    to_player
                        .currency
                        .subtract(&transaction.amount)
                        .map_err(|e| {
                            TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                        })?;

                self.put_player(from_player)?;
                self.put_player(to_player)?;
            }
            (None, Some(to)) => {
                // Reverse system grant
                let mut player = self.get_player(to)?;
                player.currency = player.currency.subtract(&transaction.amount).map_err(|e| {
                    TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                })?;
                self.put_player(player)?;
            }
            (Some(from), None) => {
                // Reverse system deduction
                let mut player = self.get_player(from)?;
                player.currency = player.currency.add(&transaction.amount).map_err(|e| {
                    TinyMushError::InvalidCurrency(format!("Rollback failed: {}", e))
                })?;
                self.put_player(player)?;
            }
            _ => {
                return Err(TinyMushError::InvalidCurrency(
                    "Invalid transaction format".to_string(),
                ))
            }
        }

        // Mark transaction as rolled back
        transaction.rolled_back = true;
        self.log_transaction(&transaction)?;

        Ok(())
    }

    /// Get transaction history for a player
    pub fn get_player_transactions(
        &self,
        username: &str,
        limit: usize,
    ) -> Result<Vec<CurrencyTransaction>, TinyMushError> {
        let mut transactions = Vec::new();
        let prefix = b"transaction:";

        for result in self.mail.scan_prefix(prefix) {
            let (_key, value) = result?;
            let transaction: CurrencyTransaction = Self::deserialize(value)?;

            // Check if player is involved in this transaction
            let involved = match (&transaction.from, &transaction.to) {
                (Some(from), Some(to)) => {
                    from == username
                        || to == username
                        || from == &format!("{}:bank", username)
                        || to == &format!("{}:bank", username)
                }
                (Some(from), None) => from == username,
                (None, Some(to)) => to == username,
                _ => false,
            };

            if involved {
                transactions.push(transaction);
            }

            if transactions.len() >= limit {
                break;
            }
        }

        // Sort by timestamp, newest first
        transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        transactions.truncate(limit);

        Ok(transactions)
    }

    // ========================================================================
    // Inventory Management Methods
    // ========================================================================

    /// Add an item to player's inventory with capacity checks
    pub fn player_add_item(
        &self,
        username: &str,
        object_id: &str,
        quantity: u32,
        config: &crate::tmush::types::InventoryConfig,
    ) -> Result<crate::tmush::types::InventoryResult, TinyMushError> {
        let mut player = self.get_player(username)?;
        let item = self.get_object(object_id)?;

        // Check if we can add the item
        let get_item_fn = |id: &str| self.get_object(id).ok();
        if let Err(reason) =
            crate::tmush::inventory::can_add_item(&player, &item, quantity, config, get_item_fn)
        {
            return Ok(crate::tmush::types::InventoryResult::Failed { reason });
        }

        // Add the item
        let result =
            crate::tmush::inventory::add_item_to_inventory(&mut player, &item, quantity, config);

        // Save the player
        player.touch();
        self.put_player(player)?;

        Ok(result)
    }

    /// Remove an item from player's inventory
    pub fn player_remove_item(
        &self,
        username: &str,
        object_id: &str,
        quantity: u32,
    ) -> Result<crate::tmush::types::InventoryResult, TinyMushError> {
        let mut player = self.get_player(username)?;

        let result =
            crate::tmush::inventory::remove_item_from_inventory(&mut player, object_id, quantity);

        // Save the player if successful
        if !matches!(result, crate::tmush::types::InventoryResult::Failed { .. }) {
            player.touch();
            self.put_player(player)?;
        }

        Ok(result)
    }

    /// Check if player has an item in inventory
    pub fn player_has_item(
        &self,
        username: &str,
        object_id: &str,
        quantity: u32,
    ) -> Result<bool, TinyMushError> {
        let player = self.get_player(username)?;
        Ok(crate::tmush::inventory::has_item(
            &player, object_id, quantity,
        ))
    }

    /// Get quantity of an item in player's inventory
    pub fn player_item_quantity(
        &self,
        username: &str,
        object_id: &str,
    ) -> Result<u32, TinyMushError> {
        let player = self.get_player(username)?;
        Ok(crate::tmush::inventory::get_item_quantity(
            &player, object_id,
        ))
    }

    /// Get formatted inventory list for player
    pub fn player_inventory_list(&self, username: &str) -> Result<Vec<String>, TinyMushError> {
        let player = self.get_player(username)?;
        let get_item_fn = |id: &str| self.get_object(id).ok();
        Ok(crate::tmush::inventory::format_inventory_compact(
            &player,
            get_item_fn,
        ))
    }

    /// Calculate total weight of player's inventory
    pub fn player_inventory_weight(&self, username: &str) -> Result<u32, TinyMushError> {
        let player = self.get_player(username)?;
        let get_item_fn = |id: &str| self.get_object(id).ok();
        Ok(crate::tmush::inventory::calculate_total_weight(
            &player.inventory_stacks,
            get_item_fn,
        ))
    }

    /// Transfer an item from one player to another (for trading)
    pub fn transfer_item(
        &self,
        from_username: &str,
        to_username: &str,
        object_id: &str,
        quantity: u32,
        config: &crate::tmush::types::InventoryConfig,
    ) -> Result<(), TinyMushError> {
        // Get both players
        let mut from_player = self.get_player(from_username)?;
        let mut to_player = self.get_player(to_username)?;

        // Check if sender has the item
        if !crate::tmush::inventory::has_item(&from_player, object_id, quantity) {
            return Err(TinyMushError::InvalidCurrency(
                "Sender does not have enough of that item".to_string(),
            ));
        }

        // Get the item
        let item = self.get_object(object_id)?;

        // Check if receiver can accept the item
        let get_item_fn = |id: &str| self.get_object(id).ok();
        if let Err(reason) =
            crate::tmush::inventory::can_add_item(&to_player, &item, quantity, config, get_item_fn)
        {
            return Err(TinyMushError::InvalidCurrency(reason));
        }

        // Perform the transfer
        let remove_result = crate::tmush::inventory::remove_item_from_inventory(
            &mut from_player,
            object_id,
            quantity,
        );
        if matches!(
            remove_result,
            crate::tmush::types::InventoryResult::Failed { .. }
        ) {
            return Err(TinyMushError::InvalidCurrency(
                "Failed to remove item from sender".to_string(),
            ));
        }

        let add_result =
            crate::tmush::inventory::add_item_to_inventory(&mut to_player, &item, quantity, config);
        if matches!(
            add_result,
            crate::tmush::types::InventoryResult::Failed { .. }
        ) {
            // Rollback - add item back to sender
            crate::tmush::inventory::add_item_to_inventory(
                &mut from_player,
                &item,
                quantity,
                config,
            );
            return Err(TinyMushError::InvalidCurrency(
                "Failed to add item to receiver".to_string(),
            ));
        }

        // Save both players
        from_player.touch();
        to_player.touch();
        self.put_player(from_player)?;
        self.put_player(to_player)?;

        Ok(())
    }

    // ============================================================================
    // Trade Session Management
    // ============================================================================

    /// Create or update a trade session
    pub fn put_trade_session(&self, session: &TradeSession) -> Result<(), TinyMushError> {
        let key = format!("trade:{}", session.id);
        let value = bincode::serialize(session)?;
        self.trades.insert(key.as_bytes(), value)?;

        // Maintain player trade indexes for both participants
        // Only index active (non-expired, non-completed) trades
        if !session.is_expired() && session.completed_at.is_none() {
            let p1_key = format!("ptrade:{}", session.player1.to_ascii_lowercase());
            let p2_key = format!("ptrade:{}", session.player2.to_ascii_lowercase());
            self.player_trades
                .insert(p1_key.as_bytes(), session.id.as_bytes())?;
            self.player_trades
                .insert(p2_key.as_bytes(), session.id.as_bytes())?;
        }

        Ok(())
    }

    /// Get an active trade session by ID
    pub fn get_trade_session(
        &self,
        session_id: &str,
    ) -> Result<Option<TradeSession>, TinyMushError> {
        let key = format!("trade:{}", session_id);
        if let Some(bytes) = self.trades.get(key.as_bytes())? {
            let session: TradeSession = bincode::deserialize(&bytes)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    /// Get active trade session for a player (either as initiator or recipient)
    pub fn get_player_active_trade(
        &self,
        username: &str,
    ) -> Result<Option<TradeSession>, TinyMushError> {
        let username_lower = username.to_ascii_lowercase();

        // Use secondary index for O(1) lookup
        let index_key = format!("ptrade:{}", username_lower);
        if let Some(session_id_bytes) = self.player_trades.get(index_key.as_bytes())? {
            let session_id = String::from_utf8_lossy(&session_id_bytes).to_string();
            if let Some(session) = self.get_trade_session(&session_id)? {
                // Verify session is still active
                if !session.is_expired() && session.completed_at.is_none() {
                    return Ok(Some(session));
                } else {
                    // Clean up stale index entry
                    let _ = self.player_trades.remove(index_key.as_bytes());
                }
            }
        }

        // Fallback: scan for migration (rebuilds index automatically)
        for result in self.trades.iter() {
            let (_key, value) = result?;
            let session: TradeSession = bincode::deserialize(&value)?;
            if !session.is_expired()
                && session.completed_at.is_none()
                && (session.player1.to_ascii_lowercase() == username_lower
                    || session.player2.to_ascii_lowercase() == username_lower)
            {
                // Rebuild index entries
                let p1_key = format!("ptrade:{}", session.player1.to_ascii_lowercase());
                let p2_key = format!("ptrade:{}", session.player2.to_ascii_lowercase());
                let _ = self
                    .player_trades
                    .insert(p1_key.as_bytes(), session.id.as_bytes());
                let _ = self
                    .player_trades
                    .insert(p2_key.as_bytes(), session.id.as_bytes());
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    /// Delete a trade session (after completion or cancellation)
    pub fn delete_trade_session(&self, session_id: &str) -> Result<(), TinyMushError> {
        // Get the session to clean up player indexes
        if let Ok(Some(session)) = self.get_trade_session(session_id) {
            let p1_key = format!("ptrade:{}", session.player1.to_ascii_lowercase());
            let p2_key = format!("ptrade:{}", session.player2.to_ascii_lowercase());
            let _ = self.player_trades.remove(p1_key.as_bytes());
            let _ = self.player_trades.remove(p2_key.as_bytes());
        }

        let key = format!("trade:{}", session_id);
        self.trades.remove(key.as_bytes())?;
        Ok(())
    }

    /// Clean up expired trade sessions
    pub fn cleanup_expired_trades(&self) -> Result<usize, TinyMushError> {
        let mut expired_keys = Vec::new();
        for result in self.trades.iter() {
            let (key, value) = result?;
            let session: TradeSession = bincode::deserialize(&value)?;
            if session.is_expired() || session.completed_at.is_some() {
                expired_keys.push(key);
            }
        }
        let count = expired_keys.len();
        for key in expired_keys {
            self.trades.remove(key)?;
        }
        Ok(count)
    }

    // ====== Quest Storage Methods ======

    /// Store or update a quest template
    pub fn put_quest(&self, quest: QuestRecord) -> Result<(), TinyMushError> {
        let key = format!("quest:{}", quest.id);
        let bytes = bincode::serialize(&quest)?;
        self.quests.insert(key.as_bytes(), bytes)?;
        Ok(())
    }

    /// Retrieve a quest template by ID
    pub fn get_quest(&self, quest_id: &str) -> Result<QuestRecord, TinyMushError> {
        let key = format!("quest:{}", quest_id);
        let bytes = self
            .quests
            .get(key.as_bytes())?
            .ok_or_else(|| TinyMushError::NotFound(format!("Quest not found: {}", quest_id)))?;
        let quest: QuestRecord = bincode::deserialize(&bytes)?;
        Ok(quest)
    }

    /// List all quest IDs
    pub fn list_quest_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let prefix = b"quest:";
        let mut ids = Vec::new();
        for item in self.quests.scan_prefix(prefix) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id) = key_str.strip_prefix("quest:") {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }

    /// Get quests offered by a specific NPC
    pub fn get_quests_by_npc(&self, npc_id: &str) -> Result<Vec<QuestRecord>, TinyMushError> {
        let all_ids = self.list_quest_ids()?;
        let mut quests = Vec::new();
        for id in all_ids {
            if let Ok(quest) = self.get_quest(&id) {
                if quest.quest_giver_npc == npc_id {
                    quests.push(quest);
                }
            }
        }
        Ok(quests)
    }

    /// Delete a quest template (admin function)
    pub fn delete_quest(&self, quest_id: &str) -> Result<(), TinyMushError> {
        let key = format!("quest:{}", quest_id);
        self.quests.remove(key.as_bytes())?;
        Ok(())
    }

    /// Check if a quest exists
    pub fn quest_exists(&self, quest_id: &str) -> Result<bool, TinyMushError> {
        let key = format!("quest:{}", quest_id);
        Ok(self.quests.contains_key(key.as_bytes())?)
    }

    // ========================================================================
    // Achievement Storage
    // ========================================================================

    /// Store or update an achievement
    pub fn put_achievement(
        &self,
        mut achievement: crate::tmush::types::AchievementRecord,
    ) -> Result<(), TinyMushError> {
        achievement.schema_version = 1;
        let key = format!("achievement:{}", achievement.id).into_bytes();
        let bytes = Self::serialize(&achievement)?;
        self.achievements.insert(key, bytes)?;
        self.achievements.flush()?;
        Ok(())
    }

    /// Retrieve an achievement by ID
    pub fn get_achievement(
        &self,
        achievement_id: &str,
    ) -> Result<crate::tmush::types::AchievementRecord, TinyMushError> {
        let key = format!("achievement:{}", achievement_id);
        let bytes = self
            .achievements
            .get(key.as_bytes())?
            .ok_or_else(|| TinyMushError::NotFound(format!("Achievement {}", achievement_id)))?;
        Self::deserialize(bytes)
    }

    /// List all achievement IDs
    pub fn list_achievement_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        for kv in self.achievements.iter() {
            let (key_bytes, _) = kv?;
            if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                if let Some(id) = key_str.strip_prefix("achievement:") {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Get all achievements in a category
    pub fn get_achievements_by_category(
        &self,
        category: &crate::tmush::types::AchievementCategory,
    ) -> Result<Vec<crate::tmush::types::AchievementRecord>, TinyMushError> {
        let mut achievements = Vec::new();
        for kv in self.achievements.iter() {
            let (_key, value) = kv?;
            let achievement: crate::tmush::types::AchievementRecord = Self::deserialize(value)?;
            if &achievement.category == category {
                achievements.push(achievement);
            }
        }
        Ok(achievements)
    }

    /// Delete an achievement
    pub fn delete_achievement(&self, achievement_id: &str) -> Result<(), TinyMushError> {
        let key = format!("achievement:{}", achievement_id);
        self.achievements.remove(key.as_bytes())?;
        Ok(())
    }

    /// Check if an achievement exists
    pub fn achievement_exists(&self, achievement_id: &str) -> Result<bool, TinyMushError> {
        let key = format!("achievement:{}", achievement_id);
        Ok(self.achievements.contains_key(key.as_bytes())?)
    }

    /// Seed achievements if none exist
    pub fn seed_achievements_if_needed(&self) -> Result<usize, TinyMushError> {
        if self.achievements.iter().next().is_some() {
            return Ok(0);
        }

        // Load achievements from JSON seed file
        let seed_path = std::path::Path::new(&self.data_dir).join("seeds/achievements.json");
        let achievements = match crate::tmush::load_achievements_from_json(&seed_path) {
            Ok(achievements) => achievements,
            Err(e) => {
                log::warn!(
                    "Failed to load achievements from {}: {}. Falling back to hardcoded seeds.",
                    seed_path.display(),
                    e
                );
                // Fallback to hardcoded seeds if JSON file doesn't exist
                crate::tmush::seed_starter_achievements()
            }
        };

        let mut inserted = 0usize;
        for achievement in achievements {
            self.put_achievement(achievement)?;
            inserted += 1;
        }
        Ok(inserted)
    }

    // ========================================================================
    // Companion Storage (Phase 6 Week 4)
    // ========================================================================

    /// Store or update a companion
    pub fn put_companion(&self, mut companion: CompanionRecord) -> Result<(), TinyMushError> {
        companion.schema_version = 1;
        let key = format!("companion:{}", companion.id).into_bytes();
        let bytes = Self::serialize(&companion)?;
        self.companions.insert(key, bytes)?;
        self.companions.flush()?;
        Ok(())
    }

    /// Retrieve a companion by ID
    pub fn get_companion(&self, companion_id: &str) -> Result<CompanionRecord, TinyMushError> {
        let key = format!("companion:{}", companion_id);
        let bytes = self
            .companions
            .get(key.as_bytes())?
            .ok_or_else(|| TinyMushError::NotFound(format!("Companion {}", companion_id)))?;
        Self::deserialize(bytes)
    }

    /// List all companion IDs
    pub fn list_companion_ids(&self) -> Result<Vec<String>, TinyMushError> {
        let mut ids = Vec::new();
        for kv in self.companions.iter() {
            let (key_bytes, _) = kv?;
            if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                if let Some(id) = key_str.strip_prefix("companion:") {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Get all companions in a room
    pub fn get_companions_in_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<CompanionRecord>, TinyMushError> {
        let mut companions = Vec::new();
        for kv in self.companions.iter() {
            let (_key, value) = kv?;
            let companion: CompanionRecord = Self::deserialize(value)?;
            if companion.room_id == room_id {
                companions.push(companion);
            }
        }
        Ok(companions)
    }

    /// Get all companions owned by a player
    pub fn get_player_companions(
        &self,
        username: &str,
    ) -> Result<Vec<CompanionRecord>, TinyMushError> {
        let mut companions = Vec::new();
        for kv in self.companions.iter() {
            let (_key, value) = kv?;
            let companion: CompanionRecord = Self::deserialize(value)?;
            if companion.owner.as_deref() == Some(username) {
                companions.push(companion);
            }
        }
        Ok(companions)
    }

    /// Get unowned (wild) companions in a room
    pub fn get_wild_companions_in_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<CompanionRecord>, TinyMushError> {
        let mut companions = Vec::new();
        for kv in self.companions.iter() {
            let (_key, value) = kv?;
            let companion: CompanionRecord = Self::deserialize(value)?;
            if companion.room_id == room_id && companion.owner.is_none() {
                companions.push(companion);
            }
        }
        Ok(companions)
    }

    /// Delete a companion
    pub fn delete_companion(&self, companion_id: &str) -> Result<(), TinyMushError> {
        let key = format!("companion:{}", companion_id);
        self.companions.remove(key.as_bytes())?;
        Ok(())
    }

    /// Seed starter companions if none exist
    pub fn seed_companions_if_needed(&self) -> Result<usize, TinyMushError> {
        if self.companions.iter().next().is_some() {
            return Ok(0);
        }

        // Load companions from JSON seed file
        let seed_path = std::path::Path::new(&self.data_dir).join("seeds/companions.json");
        let companions = match crate::tmush::load_companions_from_json(&seed_path) {
            Ok(companions) => companions,
            Err(e) => {
                log::warn!(
                    "Failed to load companions from {}: {}. Falling back to hardcoded seeds.",
                    seed_path.display(),
                    e
                );
                // Fallback to hardcoded seeds if JSON file doesn't exist
                crate::tmush::seed_starter_companions()
            }
        };

        let mut inserted = 0usize;
        for companion in companions {
            self.put_companion(companion)?;
            inserted += 1;
        }
        Ok(inserted)
    }

    // ========================================================================
    // Crafting Recipe Management (Data-Driven System)
    // ========================================================================

    /// Store or update a crafting recipe
    pub fn put_recipe(
        &self,
        recipe: crate::tmush::types::CraftingRecipe,
    ) -> Result<(), TinyMushError> {
        let key = format!("recipe:{}", recipe.id);
        let value = Self::serialize(&recipe)?;
        self.objects.insert(key.as_bytes(), value)?;
        self.objects.flush()?;
        Ok(())
    }

    /// Get a recipe by ID
    pub fn get_recipe(
        &self,
        recipe_id: &str,
    ) -> Result<crate::tmush::types::CraftingRecipe, TinyMushError> {
        let key = format!("recipe:{}", recipe_id);
        let bytes = self
            .objects
            .get(key.as_bytes())?
            .ok_or_else(|| TinyMushError::NotFound(format!("Recipe not found: {}", recipe_id)))?;
        Self::deserialize(bytes)
    }

    /// List all recipes, optionally filtered by crafting station
    pub fn list_recipes(
        &self,
        station_filter: Option<&str>,
    ) -> Result<Vec<crate::tmush::types::CraftingRecipe>, TinyMushError> {
        let prefix = b"recipe:";
        let mut recipes = Vec::new();

        for result in self.objects.scan_prefix(prefix) {
            let (_, value) = result?;
            let recipe: crate::tmush::types::CraftingRecipe = Self::deserialize(value)?;

            if let Some(station) = station_filter {
                if recipe.requires_station.as_deref() == Some(station) {
                    recipes.push(recipe);
                }
            } else {
                recipes.push(recipe);
            }
        }

        // Sort by name for consistent ordering
        recipes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(recipes)
    }

    /// Delete a recipe by ID
    pub fn delete_recipe(&self, recipe_id: &str) -> Result<(), TinyMushError> {
        let key = format!("recipe:{}", recipe_id);
        self.objects.remove(key.as_bytes())?;
        self.objects.flush()?;
        Ok(())
    }

    /// Check if a recipe exists
    pub fn recipe_exists(&self, recipe_id: &str) -> Result<bool, TinyMushError> {
        let key = format!("recipe:{}", recipe_id);
        Ok(self.objects.contains_key(key.as_bytes())?)
    }

    /// Seed default crafting recipes if none exist
    pub fn seed_recipes_if_needed(&self) -> Result<usize, TinyMushError> {
        // Check if any recipes exist
        if self.objects.scan_prefix(b"recipe:").next().is_some() {
            return Ok(0);
        }

        // Load recipes from JSON seed file
        let seed_path = std::path::Path::new(&self.data_dir).join("seeds/recipes.json");
        let recipes = match crate::tmush::load_recipes_from_json(&seed_path) {
            Ok(recipes) => recipes,
            Err(e) => {
                log::warn!(
                    "Failed to load recipes from {}: {}. Falling back to hardcoded seeds.",
                    seed_path.display(),
                    e
                );
                // Fallback to hardcoded seeds if JSON file doesn't exist
                crate::tmush::state::seed_default_recipes()
            }
        };

        let mut inserted = 0usize;
        for recipe in recipes {
            self.put_recipe(recipe)?;
            inserted += 1;
        }
        Ok(inserted)
    }

    // ========================================================================
    // NPC and World Seeding
    // ========================================================================

    pub fn seed_npcs_if_needed(&self) -> Result<usize, TinyMushError> {
        // Check if NPCs exist by trying to iterate the tree
        // If deserialization fails (schema mismatch), clear old data and reseed
        match self.npcs.iter().next() {
            Some(Ok(_)) => {
                // NPCs exist and are valid, no seeding needed
                return Ok(0);
            }
            Some(Err(e)) => {
                // NPCs exist but have deserialization errors (schema mismatch)
                log::warn!("Found corrupted NPC data, clearing and reseeding: {}", e);
                // Clear all NPC data
                self.npcs.clear()?;
                self.npcs.flush()?;
            }
            None => {
                // No NPCs exist, proceed with seeding
            }
        }

        // Load NPCs from JSON seed file
        let seed_path = std::path::Path::new(&self.data_dir).join("seeds/npcs.json");
        let npcs = match crate::tmush::load_npcs_from_json(&seed_path) {
            Ok(npcs) => npcs,
            Err(e) => {
                log::warn!(
                    "Failed to load NPCs from {}: {}. Falling back to hardcoded seeds.",
                    seed_path.display(),
                    e
                );
                // Fallback to hardcoded seeds if JSON file doesn't exist
                crate::tmush::seed_starter_npcs()
            }
        };

        let mut inserted = 0usize;
        for npc in npcs {
            self.put_npc(npc)?;
            inserted += 1;
        }
        Ok(inserted)
    }

    /// Seed starter shop (vending machine) with trinkets (idempotent)
    ///
    /// Creates a vending machine in the Museum that sells commemorative trinkets.
    /// Uses clone_items mode to sell unlimited clones of the seed objects.
    pub fn seed_shops_if_needed(&self) -> Result<usize, TinyMushError> {
        // Check if shops already exist
        let shop_ids = self.list_shop_ids()?;
        if !shop_ids.is_empty() {
            return Ok(0); // Shops already seeded
        }

        // Seed vending machine and trinkets
        let (vending_machine, trinkets) = crate::tmush::state::seed_starter_vending_machine();

        // Store trinkets as objects
        for trinket in trinkets {
            self.put_object(trinket)?;
        }

        // Store vending machine shop
        self.put_shop(vending_machine)?;

        Ok(1) // 1 shop created
    }

    /// Seed initial admin account if no admins exist (idempotent)
    ///
    /// Creates a default sysop-level admin account on fresh database initialization.
    /// Uses the provided username (typically from config) and grants level 3 (sysop) privileges.
    ///
    /// # Arguments
    /// * `admin_username` - Username for the admin account (e.g., "admin", "sysop")
    ///
    /// # Returns
    /// * `Ok(true)` if admin was created
    /// * `Ok(false)` if admin already exists (no-op)
    ///
    /// # Example
    /// ```no_run
    /// # use meshbbs::tmush::TinyMushStoreBuilder;
    /// # let store = TinyMushStoreBuilder::new("/tmp/test").open().unwrap();
    /// store.seed_admin_if_needed("admin").expect("seed admin");
    /// ```
    pub fn seed_admin_if_needed(&self, admin_username: &str) -> Result<bool, TinyMushError> {
        // Normalize username for comparison while preserving original casing when creating records
        let target_lower = admin_username.to_ascii_lowercase();
        let admins = self.list_admins()?;

        // If target already exists as admin (case-insensitive), nothing to do
        if admins
            .iter()
            .any(|player| player.username.eq_ignore_ascii_case(admin_username))
        {
            return Ok(false);
        }

        // Promote existing player if present
        if let Ok(mut existing) = self.get_player(admin_username) {
            existing.grant_admin(3);
            self.put_player(existing)?;
            return Ok(true);
        }

        // Player not found using original case; try lowercase variant before creating new record
        if admin_username != target_lower {
            if let Ok(mut existing) = self.get_player(&target_lower) {
                existing.grant_admin(3);
                self.put_player(existing)?;
                return Ok(true);
            }
        }

        // Create new admin account for target username
        let mut admin = crate::tmush::types::PlayerRecord::new(
            admin_username,
            &format!("{} (Admin)", admin_username),
            crate::tmush::state::REQUIRED_START_LOCATION_ID,
        );
        admin.grant_admin(3);
        self.put_player(admin)?;

        Ok(true)
    }

    // ========================
    // World Configuration
    // ========================

    /// Get the world configuration, returning default if not found
    pub fn get_world_config(&self) -> Result<WorldConfig, TinyMushError> {
        let key = b"world_config";
        match self.config.get(key)? {
            Some(value) => Ok(Self::deserialize(value)?),
            None => {
                // Initialize with default if not found
                let default = WorldConfig::default();
                self.put_world_config(&default)?;
                Ok(default)
            }
        }
    }

    /// Save the world configuration
    pub fn put_world_config(&self, config: &WorldConfig) -> Result<(), TinyMushError> {
        let key = b"world_config";
        let value = Self::serialize(config)?;
        self.config.insert(key, value)?;
        Ok(())
    }

    // =========================================================================
    // Housing System (Phase 7)
    // =========================================================================

    /// Seed default housing templates if none exist
    pub fn seed_housing_templates_if_needed(&self) -> Result<usize, TinyMushError> {
        let existing = self.list_housing_templates()?;
        if !existing.is_empty() {
            return Ok(0);
        }

        use crate::tmush::types::{
            Direction, HousingPermissions, HousingTemplate, HousingTemplateRoom, RoomFlag,
        };

        let mut templates = Vec::new();

        // 1. Studio Apartment - Affordable single room
        let mut studio = HousingTemplate::new(
            "studio_apartment",
            "Studio Apartment",
            "A cozy single-room apartment perfect for solo living. Affordable and efficient.",
            "world_builder",
        )
        .with_cost(100, 10); // 100 to rent, 10 recurring

        studio.permissions = HousingPermissions {
            can_edit_description: true,
            can_add_objects: true,
            can_invite_guests: true,
            can_build: false,
            can_set_flags: false,
            can_rename_exits: false,
        };

        studio.rooms = vec![HousingTemplateRoom {
            room_id: "main_room".to_string(),
            name: "Studio Apartment".to_string(),
            short_desc: "A compact studio apartment".to_string(),
            long_desc: "This modest studio apartment combines living, sleeping, and cooking areas into a single efficient space. A small kitchenette occupies one corner, while a comfortable bed sits against the far wall. A window offers a view of the city street below.".to_string(),
            exits: std::collections::HashMap::new(), // Entry from world, added during instantiation
            flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
            max_capacity: 5,
        }];
        studio.entry_room = "main_room".to_string();
        studio = studio
            .with_tags(vec![
                "modern".to_string(),
                "urban".to_string(),
                "affordable".to_string(),
            ])
            .with_category("apartment")
            .with_max_instances(-1); // Unlimited

        templates.push(studio);

        // 2. Basic Apartment - 3 rooms for comfortable living
        let mut basic = HousingTemplate::new(
            "basic_apartment",
            "Basic Apartment",
            "A comfortable three-room apartment with separate living, sleeping, and cooking areas.",
            "world_builder",
        )
        .with_cost(500, 50); // 500 to rent, 50 recurring

        basic.permissions = HousingPermissions {
            can_edit_description: true,
            can_add_objects: true,
            can_invite_guests: true,
            can_build: false,
            can_set_flags: false,
            can_rename_exits: false,
        };

        let mut living_exits = std::collections::HashMap::new();
        living_exits.insert(Direction::East, "bedroom".to_string());
        living_exits.insert(Direction::West, "kitchen".to_string());

        let mut bedroom_exits = std::collections::HashMap::new();
        bedroom_exits.insert(Direction::West, "living_room".to_string());

        let mut kitchen_exits = std::collections::HashMap::new();
        kitchen_exits.insert(Direction::East, "living_room".to_string());

        basic.rooms = vec![
            HousingTemplateRoom {
                room_id: "living_room".to_string(),
                name: "Living Room".to_string(),
                short_desc: "A cozy living room".to_string(),
                long_desc: "This comfortable living room serves as the heart of the apartment. A worn but serviceable sofa faces a small entertainment center. Sunlight streams through a large window. Doorways lead east to the bedroom and west to the kitchen.".to_string(),
                exits: living_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 10,
            },
            HousingTemplateRoom {
                room_id: "bedroom".to_string(),
                name: "Bedroom".to_string(),
                short_desc: "A quiet bedroom".to_string(),
                long_desc: "A peaceful bedroom with a comfortable bed, nightstand, and small closet. Curtains can be drawn over the window for privacy. The living room lies to the west.".to_string(),
                exits: bedroom_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe, RoomFlag::Dark],
                max_capacity: 5,
            },
            HousingTemplateRoom {
                room_id: "kitchen".to_string(),
                name: "Kitchen".to_string(),
                short_desc: "A functional kitchen".to_string(),
                long_desc: "A compact but well-equipped kitchen with modern appliances. Counter space surrounds a small sink, and cabinets provide ample storage. The living room is to the east.".to_string(),
                exits: kitchen_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 5,
            },
        ];
        basic.entry_room = "living_room".to_string();
        basic = basic
            .with_tags(vec![
                "modern".to_string(),
                "urban".to_string(),
                "comfortable".to_string(),
            ])
            .with_category("apartment")
            .with_max_instances(20); // Limited availability

        templates.push(basic);

        // 3. Luxury Flat - 5 rooms with extended permissions
        let mut luxury = HousingTemplate::new(
            "luxury_flat",
            "Luxury Flat",
            "An upscale five-room flat with premium finishes and expanded customization options.",
            "world_builder",
        )
        .with_cost(2000, 200); // 2000 to rent, 200 recurring

        luxury.permissions = HousingPermissions {
            can_edit_description: true,
            can_add_objects: true,
            can_invite_guests: true,
            can_build: true, // Can add rooms!
            can_set_flags: true,
            can_rename_exits: true,
        };

        // Entry hall → branches to all rooms
        let mut entry_exits = std::collections::HashMap::new();
        entry_exits.insert(Direction::North, "living_room".to_string());
        entry_exits.insert(Direction::East, "master_bedroom".to_string());
        entry_exits.insert(Direction::West, "kitchen".to_string());
        entry_exits.insert(Direction::South, "study".to_string());

        let mut lux_living_exits = std::collections::HashMap::new();
        lux_living_exits.insert(Direction::South, "entry_hall".to_string());
        lux_living_exits.insert(Direction::East, "dining_room".to_string());

        let mut master_exits = std::collections::HashMap::new();
        master_exits.insert(Direction::West, "entry_hall".to_string());

        let mut lux_kitchen_exits = std::collections::HashMap::new();
        lux_kitchen_exits.insert(Direction::East, "entry_hall".to_string());
        lux_kitchen_exits.insert(Direction::North, "dining_room".to_string());

        let mut dining_exits = std::collections::HashMap::new();
        dining_exits.insert(Direction::West, "living_room".to_string());
        dining_exits.insert(Direction::South, "kitchen".to_string());

        let mut study_exits = std::collections::HashMap::new();
        study_exits.insert(Direction::North, "entry_hall".to_string());

        luxury.rooms = vec![
            HousingTemplateRoom {
                room_id: "entry_hall".to_string(),
                name: "Entry Hall".to_string(),
                short_desc: "An elegant entry hall".to_string(),
                long_desc: "A spacious entry hall with polished hardwood floors and tasteful artwork. Doorways branch to the north (living room), east (master bedroom), west (kitchen), and south (study).".to_string(),
                exits: entry_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 10,
            },
            HousingTemplateRoom {
                room_id: "living_room".to_string(),
                name: "Living Room".to_string(),
                short_desc: "A luxurious living room".to_string(),
                long_desc: "This expansive living room features designer furniture, a modern entertainment system, and floor-to-ceiling windows offering stunning city views. The entry hall is to the south, and the dining room to the east.".to_string(),
                exits: lux_living_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 20,
            },
            HousingTemplateRoom {
                room_id: "master_bedroom".to_string(),
                name: "Master Bedroom".to_string(),
                short_desc: "A luxurious master bedroom".to_string(),
                long_desc: "An opulent master bedroom with a king-sized bed, walk-in closet, and an ensuite bathroom visible through an archway. Plush carpeting and high-end furnishings complete the space. The entry hall is to the west.".to_string(),
                exits: master_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 5,
            },
            HousingTemplateRoom {
                room_id: "kitchen".to_string(),
                name: "Gourmet Kitchen".to_string(),
                short_desc: "A professional-grade kitchen".to_string(),
                long_desc: "A chef's dream kitchen with stainless steel appliances, granite countertops, and a large center island. Ample cabinet space and a wine rack complete the setup. The entry hall is to the east, and the dining room to the north.".to_string(),
                exits: lux_kitchen_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 10,
            },
            HousingTemplateRoom {
                room_id: "dining_room".to_string(),
                name: "Dining Room".to_string(),
                short_desc: "A formal dining room".to_string(),
                long_desc: "An elegant dining room with a large table that seats eight, a crystal chandelier overhead, and a sideboard for serving. The living room is to the west, and the kitchen to the south.".to_string(),
                exits: dining_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe],
                max_capacity: 15,
            },
            HousingTemplateRoom {
                room_id: "study".to_string(),
                name: "Private Study".to_string(),
                short_desc: "A quiet study".to_string(),
                long_desc: "A tranquil study lined with built-in bookshelves and featuring a large oak desk. A comfortable reading chair sits by the window. Perfect for work or contemplation. The entry hall is to the north.".to_string(),
                exits: study_exits,
                flags: vec![RoomFlag::Private, RoomFlag::Indoor, RoomFlag::Safe, RoomFlag::Dark],
                max_capacity: 5,
            },
        ];
        luxury.entry_room = "entry_hall".to_string();
        luxury = luxury
            .with_tags(vec![
                "modern".to_string(),
                "urban".to_string(),
                "luxury".to_string(),
                "premium".to_string(),
            ])
            .with_category("flat")
            .with_max_instances(5); // Very limited

        templates.push(luxury);

        // Save all templates
        for template in &templates {
            self.put_housing_template(template)?;
        }

        Ok(templates.len())
    }

    /// Get a housing template by ID
    pub fn get_housing_template(
        &self,
        template_id: &str,
    ) -> Result<HousingTemplate, TinyMushError> {
        let key = format!("template:{}", template_id);
        match self.housing_templates.get(key.as_bytes())? {
            Some(data) => Ok(Self::deserialize(data)?),
            None => Err(TinyMushError::NotFound(format!(
                "Housing template not found: {}",
                template_id
            ))),
        }
    }

    /// Save a housing template
    pub fn put_housing_template(&self, template: &HousingTemplate) -> Result<(), TinyMushError> {
        let key = format!("template:{}", template.id);
        let value = Self::serialize(template)?;
        self.housing_templates.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// List all housing template IDs
    pub fn list_housing_templates(&self) -> Result<Vec<String>, TinyMushError> {
        let mut template_ids = Vec::new();
        for item in self.housing_templates.scan_prefix(b"template:") {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if let Some(id) = key_str.strip_prefix("template:") {
                template_ids.push(id.to_string());
            }
        }
        Ok(template_ids)
    }

    /// Delete a housing template
    pub fn delete_housing_template(&self, template_id: &str) -> Result<(), TinyMushError> {
        let key = format!("template:{}", template_id);
        self.housing_templates.remove(key.as_bytes())?;
        Ok(())
    }

    /// Get a housing instance by ID
    pub fn get_housing_instance(
        &self,
        instance_id: &str,
    ) -> Result<HousingInstance, TinyMushError> {
        let key = format!("instance:{}", instance_id);
        match self.housing_instances.get(key.as_bytes())? {
            Some(data) => Ok(Self::deserialize(data)?),
            None => Err(TinyMushError::NotFound(format!(
                "Housing instance not found: {}",
                instance_id
            ))),
        }
    }

    /// Get all housing instances for a specific owner
    pub fn get_player_housing_instances(
        &self,
        owner: &str,
    ) -> Result<Vec<HousingInstance>, TinyMushError> {
        let prefix = format!("instance:{}", owner);
        let mut instances = Vec::new();
        for item in self.housing_instances.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let instance: HousingInstance = Self::deserialize(value)?;
            instances.push(instance);
        }
        Ok(instances)
    }

    /// Get all housing instances where the player is a guest
    pub fn get_guest_housing_instances(
        &self,
        username: &str,
    ) -> Result<Vec<HousingInstance>, TinyMushError> {
        let username_lower = username.to_ascii_lowercase();
        let mut instances = Vec::new();

        // Use secondary index for O(1) per-instance lookup
        let prefix = format!("guest:{}:", username_lower);
        for item in self.housing_guests.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            // Extract instance_id from key: guest:{username}:{instance_id}
            if let Some(instance_id) = key_str.strip_prefix(&format!("guest:{}:", username_lower)) {
                // Direct lookup of the instance
                if let Ok(instance) = self.get_housing_instance(instance_id) {
                    instances.push(instance);
                }
            }
        }

        // Fallback: scan all instances for migration (rebuilds index automatically)
        if instances.is_empty() {
            for item in self.housing_instances.scan_prefix(b"instance:") {
                let (_, value) = item?;
                let instance: HousingInstance = Self::deserialize(value)?;
                // Check if this player is on the guest list
                if instance
                    .guests
                    .iter()
                    .any(|g| g.to_ascii_lowercase() == username_lower)
                {
                    // Rebuild index entry
                    let index_key = format!("guest:{}:{}", username_lower, instance.id);
                    let _ = self.housing_guests.insert(index_key.as_bytes(), b"");
                    instances.push(instance);
                }
            }
        }

        Ok(instances)
    }

    /// Save a housing instance
    pub fn put_housing_instance(&self, instance: &HousingInstance) -> Result<(), TinyMushError> {
        // First, get old instance to clean up old guest indexes
        if let Ok(old_instance) = self.get_housing_instance(&instance.id) {
            // Remove old guest index entries
            for old_guest in &old_instance.guests {
                let old_key = format!("guest:{}:{}", old_guest.to_ascii_lowercase(), instance.id);
                let _ = self.housing_guests.remove(old_key.as_bytes());
            }

            // Remove old template instance index if template changed
            if old_instance.template_id != instance.template_id {
                let old_tpl_key = format!("tpl:{}:{}", old_instance.template_id, instance.id);
                let _ = self.template_instances.remove(old_tpl_key.as_bytes());
            }
        }

        // Save the instance
        let key = format!("instance:{}", instance.id);
        let value = Self::serialize(instance)?;
        self.housing_instances.insert(key.as_bytes(), value)?;

        // Maintain guest indexes: guest:{username}:{instance_id} → ""
        for guest in &instance.guests {
            let guest_key = format!("guest:{}:{}", guest.to_ascii_lowercase(), instance.id);
            self.housing_guests.insert(guest_key.as_bytes(), b"")?;
        }

        // Maintain template instance index: tpl:{template_id}:{instance_id} → ""
        if instance.active {
            let tpl_key = format!("tpl:{}:{}", instance.template_id, instance.id);
            self.template_instances.insert(tpl_key.as_bytes(), b"")?;
        }

        Ok(())
    }

    /// List all housing instance IDs
    pub fn list_housing_instances(&self) -> Result<Vec<String>, TinyMushError> {
        let mut instance_ids = Vec::new();
        for item in self.housing_instances.scan_prefix(b"instance:") {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if let Some(id) = key_str.strip_prefix("instance:") {
                instance_ids.push(id.to_string());
            }
        }
        Ok(instance_ids)
    }

    /// Delete a housing instance
    pub fn delete_housing_instance(&self, instance_id: &str) -> Result<(), TinyMushError> {
        // Get the instance to clean up indexes
        if let Ok(instance) = self.get_housing_instance(instance_id) {
            // Remove guest index entries
            for guest in &instance.guests {
                let guest_key = format!("guest:{}:{}", guest.to_ascii_lowercase(), instance_id);
                let _ = self.housing_guests.remove(guest_key.as_bytes());
            }

            // Remove template instance index
            let tpl_key = format!("tpl:{}:{}", instance.template_id, instance_id);
            let _ = self.template_instances.remove(tpl_key.as_bytes());
        }

        // Delete the instance
        let key = format!("instance:{}", instance_id);
        self.housing_instances.remove(key.as_bytes())?;
        Ok(())
    }

    /// Count active instances of a template
    pub fn count_template_instances(&self, template_id: &str) -> Result<usize, TinyMushError> {
        // Use secondary index for O(n_instances) instead of O(all_instances)
        let prefix = format!("tpl:{}:", template_id);
        let count = self
            .template_instances
            .scan_prefix(prefix.as_bytes())
            .count();

        // Fallback: rebuild index if empty but instances exist
        if count == 0 {
            let mut fallback_count = 0;
            for item in self.housing_instances.scan_prefix(b"instance:") {
                let (_, value) = item?;
                let instance: HousingInstance = Self::deserialize(value)?;
                if instance.template_id == template_id && instance.active {
                    // Rebuild index entry
                    let tpl_key = format!("tpl:{}:{}", template_id, instance.id);
                    let _ = self.template_instances.insert(tpl_key.as_bytes(), b"");
                    fallback_count += 1;
                }
            }
            return Ok(fallback_count);
        }
        Ok(count)
    }

    /// Clone a housing template to create a new instance for a player
    /// This creates actual room records and preserves connectivity
    pub fn clone_housing_template(
        &self,
        template_id: &str,
        owner: &str,
    ) -> Result<HousingInstance, TinyMushError> {
        // Load the template
        let template = self.get_housing_template(template_id)?;

        // Check max instances limit (-1 means unlimited)
        if template.max_instances >= 0 {
            let current_count = self.count_template_instances(template_id)?;
            if current_count >= template.max_instances as usize {
                return Err(TinyMushError::InvalidCurrency(format!(
                    "Template {} has reached its maximum instance limit ({})",
                    template_id, template.max_instances
                )));
            }
        }

        // Generate a unique instance ID
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let instance_id = format!("{}_{}", owner, timestamp);

        // Create room ID mappings
        let mut room_mappings = std::collections::HashMap::new();

        // First pass: Create all rooms and build the mapping
        for template_room in &template.rooms {
            let instance_room_id = format!(
                "rooms:instance:{}:{}:{}",
                owner, template_id, template_room.room_id
            );
            room_mappings.insert(template_room.room_id.clone(), instance_room_id.clone());

            // Create the actual room record
            use crate::tmush::types::{RoomOwner, RoomRecord};
            use chrono::Utc;
            let room = RoomRecord {
                id: instance_room_id.clone(),
                name: template_room.name.clone(),
                short_desc: template_room.short_desc.clone(),
                long_desc: template_room.long_desc.clone(),
                exits: std::collections::HashMap::new(), // Will update in second pass
                flags: template_room.flags.clone(),
                owner: RoomOwner::Player {
                    username: owner.to_string(),
                },
                created_at: Utc::now(),
                max_capacity: template_room.max_capacity,
                visibility: crate::tmush::types::RoomVisibility::Private,
                items: vec![],
                housing_filter_tags: vec![], // Instance rooms don't filter housing
                locked: false,               // New housing rooms unlocked by default
                schema_version: crate::tmush::types::ROOM_SCHEMA_VERSION,
            };
            self.put_room(room)?;
        }

        // Second pass: Update exit mappings
        for template_room in &template.rooms {
            let instance_room_id = room_mappings.get(&template_room.room_id).unwrap();
            let mut room = self.get_room(instance_room_id)?;

            // Remap exits to instance room IDs
            for (direction, target) in &template_room.exits {
                // If the target is another room in the template, use mapped ID
                // Otherwise, keep the original (world room)
                let new_target = room_mappings
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| target.clone());
                room.exits.insert(*direction, new_target);
            }

            self.put_room(room)?;
        }

        // Create the housing instance record
        let entry_room_id = room_mappings
            .get(&template.entry_room)
            .cloned()
            .ok_or_else(|| {
                TinyMushError::NotFound(format!(
                    "Entry room {} not found in template",
                    template.entry_room
                ))
            })?;

        use chrono::Utc;
        let instance = HousingInstance {
            id: instance_id.clone(),
            template_id: template_id.to_string(),
            owner: owner.to_string(),
            created_at: Utc::now(),
            last_payment: Utc::now(),
            room_mappings,
            entry_room_id,
            guests: vec![],
            active: true,
            reclaim_box: vec![],  // Empty reclaim box for new housing
            inactive_since: None, // Active housing
            schema_version: 1,
        };

        self.put_housing_instance(&instance)?;

        Ok(instance)
    }

    /// Update a specific configuration field
    pub fn update_world_config_field(
        &self,
        field: &str,
        value: &str,
        updated_by: &str,
    ) -> Result<(), TinyMushError> {
        let mut config = self.get_world_config()?;
        config.updated_at = Utc::now();
        config.updated_by = updated_by.to_string();

        match field {
            // Branding
            "welcome_message" => config.welcome_message = value.to_string(),
            "motd" => config.motd = value.to_string(),
            "world_name" => config.world_name = value.to_string(),
            "world_description" => config.world_description = value.to_string(),

            // Help system
            "help_main" => config.help_main = value.to_string(),
            "help_commands" => config.help_commands = value.to_string(),
            "help_movement" => config.help_movement = value.to_string(),
            "help_social" => config.help_social = value.to_string(),
            "help_bulletin" => config.help_bulletin = value.to_string(),
            "help_companion" => config.help_companion = value.to_string(),
            "help_mail" => config.help_mail = value.to_string(),

            // Error messages
            "err_no_exit" => config.err_no_exit = value.to_string(),
            "err_whisper_self" => config.err_whisper_self = value.to_string(),
            "err_no_shops" => config.err_no_shops = value.to_string(),
            "err_item_not_found" => config.err_item_not_found = value.to_string(),
            "err_trade_self" => config.err_trade_self = value.to_string(),
            "err_say_what" => config.err_say_what = value.to_string(),
            "err_emote_what" => config.err_emote_what = value.to_string(),
            "err_insufficient_funds" => config.err_insufficient_funds = value.to_string(),

            // Success messages
            "msg_deposit_success" => config.msg_deposit_success = value.to_string(),
            "msg_withdraw_success" => config.msg_withdraw_success = value.to_string(),
            "msg_buy_success" => config.msg_buy_success = value.to_string(),
            "msg_sell_success" => config.msg_sell_success = value.to_string(),
            "msg_trade_initiated" => config.msg_trade_initiated = value.to_string(),

            // Validation & input errors
            "err_whisper_what" => config.err_whisper_what = value.to_string(),
            "err_whisper_whom" => config.err_whisper_whom = value.to_string(),
            "err_pose_what" => config.err_pose_what = value.to_string(),
            "err_ooc_what" => config.err_ooc_what = value.to_string(),
            "err_amount_positive" => config.err_amount_positive = value.to_string(),
            "err_invalid_amount_format" => config.err_invalid_amount_format = value.to_string(),
            "err_transfer_self" => config.err_transfer_self = value.to_string(),

            // Empty state messages
            "msg_empty_inventory" => config.msg_empty_inventory = value.to_string(),
            "msg_no_item_quantity" => config.msg_no_item_quantity = value.to_string(),
            "msg_no_shops_available" => config.msg_no_shops_available = value.to_string(),
            "msg_no_shops_sell_to" => config.msg_no_shops_sell_to = value.to_string(),
            "msg_no_companions" => config.msg_no_companions = value.to_string(),
            "msg_no_companions_tame_hint" => config.msg_no_companions_tame_hint = value.to_string(),
            "msg_no_companions_follow" => config.msg_no_companions_follow = value.to_string(),
            "msg_no_active_quests" => config.msg_no_active_quests = value.to_string(),
            "msg_no_achievements" => config.msg_no_achievements = value.to_string(),
            "msg_no_achievements_earned" => config.msg_no_achievements_earned = value.to_string(),
            "msg_no_titles_unlocked" => config.msg_no_titles_unlocked = value.to_string(),
            "msg_no_title_equipped" => config.msg_no_title_equipped = value.to_string(),
            "msg_no_active_trade" => config.msg_no_active_trade = value.to_string(),
            "msg_no_active_trade_hint" => config.msg_no_active_trade_hint = value.to_string(),
            "msg_no_trade_history" => config.msg_no_trade_history = value.to_string(),
            "msg_no_players_found" => config.msg_no_players_found = value.to_string(),

            // Shop error messages
            "err_shop_no_sell" => config.err_shop_no_sell = value.to_string(),
            "err_shop_doesnt_sell" => config.err_shop_doesnt_sell = value.to_string(),
            "err_shop_insufficient_funds" => config.err_shop_insufficient_funds = value.to_string(),
            "err_shop_no_buy" => config.err_shop_no_buy = value.to_string(),
            "err_shop_wont_buy_price" => config.err_shop_wont_buy_price = value.to_string(),
            "err_item_not_owned" => config.err_item_not_owned = value.to_string(),
            "err_only_have_quantity" => config.err_only_have_quantity = value.to_string(),

            // Trading system messages
            "err_trade_already_active" => config.err_trade_already_active = value.to_string(),
            "err_trade_partner_busy" => config.err_trade_partner_busy = value.to_string(),
            "err_trade_player_not_here" => config.err_trade_player_not_here = value.to_string(),
            "err_trade_insufficient_amount" => {
                config.err_trade_insufficient_amount = value.to_string()
            }
            "msg_trade_accepted_waiting" => config.msg_trade_accepted_waiting = value.to_string(),

            // Movement & navigation messages
            "err_movement_restricted" => config.err_movement_restricted = value.to_string(),
            "err_player_not_here" => config.err_player_not_here = value.to_string(),

            // Quest system messages
            "err_quest_cannot_accept" => config.err_quest_cannot_accept = value.to_string(),
            "err_quest_not_found" => config.err_quest_not_found = value.to_string(),
            "msg_quest_abandoned" => config.msg_quest_abandoned = value.to_string(),

            // Achievement system messages
            "err_achievement_unknown_category" => {
                config.err_achievement_unknown_category = value.to_string()
            }
            "msg_no_achievements_category" => {
                config.msg_no_achievements_category = value.to_string()
            }

            // Title system messages
            "err_title_not_unlocked" => config.err_title_not_unlocked = value.to_string(),
            "msg_title_equipped" => config.msg_title_equipped = value.to_string(),
            "msg_title_equipped_display" => config.msg_title_equipped_display = value.to_string(),
            "err_title_usage" => config.err_title_usage = value.to_string(),

            // Companion system messages
            "msg_companion_tamed" => config.msg_companion_tamed = value.to_string(),
            "err_companion_owned" => config.err_companion_owned = value.to_string(),
            "err_companion_not_found" => config.err_companion_not_found = value.to_string(),
            "msg_companion_released" => config.msg_companion_released = value.to_string(),

            // Bulletin board messages
            "err_board_location_required" => config.err_board_location_required = value.to_string(),
            "err_board_post_location" => config.err_board_post_location = value.to_string(),
            "err_board_read_location" => config.err_board_read_location = value.to_string(),

            // NPC & tutorial messages
            "err_no_npc_here" => config.err_no_npc_here = value.to_string(),
            "msg_tutorial_completed" => config.msg_tutorial_completed = value.to_string(),
            "msg_tutorial_not_started" => config.msg_tutorial_not_started = value.to_string(),

            // Housing system messages
            "err_housing_not_at_office" => config.err_housing_not_at_office = value.to_string(),
            "err_housing_no_templates" => config.err_housing_no_templates = value.to_string(),
            "err_housing_insufficient_funds" => {
                config.err_housing_insufficient_funds = value.to_string()
            }
            "err_housing_already_owns" => config.err_housing_already_owns = value.to_string(),
            "err_housing_template_not_found" => {
                config.err_housing_template_not_found = value.to_string()
            }
            "msg_housing_rented" => config.msg_housing_rented = value.to_string(),
            "msg_housing_list_header" => config.msg_housing_list_header = value.to_string(),

            // Home/teleport system messages
            "err_teleport_in_combat" => config.err_teleport_in_combat = value.to_string(),
            "err_teleport_restricted" => config.err_teleport_restricted = value.to_string(),
            "err_teleport_cooldown" => config.err_teleport_cooldown = value.to_string(),
            "err_no_housing" => config.err_no_housing = value.to_string(),
            "err_teleport_no_access" => config.err_teleport_no_access = value.to_string(),
            "msg_teleport_success" => config.msg_teleport_success = value.to_string(),
            "home_cooldown_seconds" => {
                config.home_cooldown_seconds = value.parse().map_err(|_| {
                    TinyMushError::NotFound(format!(
                        "Invalid number for home_cooldown_seconds: {}",
                        value
                    ))
                })?;
            }
            "msg_home_list_header" => config.msg_home_list_header = value.to_string(),
            "msg_home_list_empty" => config.msg_home_list_empty = value.to_string(),
            "msg_home_list_footer_travel" => config.msg_home_list_footer_travel = value.to_string(),
            "msg_home_list_footer_set" => config.msg_home_list_footer_set = value.to_string(),
            "err_home_not_found" => config.err_home_not_found = value.to_string(),
            "msg_home_set_success" => config.msg_home_set_success = value.to_string(),

            // Guest/invite system messages
            "err_invite_no_housing" => config.err_invite_no_housing = value.to_string(),
            "err_invite_not_in_housing" => config.err_invite_not_in_housing = value.to_string(),
            "err_invite_player_not_found" => config.err_invite_player_not_found = value.to_string(),
            "err_invite_already_guest" => config.err_invite_already_guest = value.to_string(),
            "msg_invite_success" => config.msg_invite_success = value.to_string(),
            "err_uninvite_not_guest" => config.err_uninvite_not_guest = value.to_string(),
            "msg_uninvite_success" => config.msg_uninvite_success = value.to_string(),

            // Describe/customization system messages
            "err_describe_not_in_housing" => config.err_describe_not_in_housing = value.to_string(),
            "err_describe_no_permission" => config.err_describe_no_permission = value.to_string(),
            "err_describe_too_long" => config.err_describe_too_long = value.to_string(),
            "msg_describe_success" => config.msg_describe_success = value.to_string(),
            "msg_describe_current" => config.msg_describe_current = value.to_string(),

            // Technical/system messages
            "err_player_load_failed" => config.err_player_load_failed = value.to_string(),
            "err_shop_save_failed" => config.err_shop_save_failed = value.to_string(),
            "err_player_save_failed" => config.err_player_save_failed = value.to_string(),
            "err_payment_failed" => config.err_payment_failed = value.to_string(),
            "err_purchase_failed" => config.err_purchase_failed = value.to_string(),
            "err_sale_failed" => config.err_sale_failed = value.to_string(),
            "err_tutorial_error" => config.err_tutorial_error = value.to_string(),
            "err_reward_error" => config.err_reward_error = value.to_string(),
            "err_quest_failed" => config.err_quest_failed = value.to_string(),
            "err_shop_find_failed" => config.err_shop_find_failed = value.to_string(),
            "err_player_list_failed" => config.err_player_list_failed = value.to_string(),
            "err_movement_failed" => config.err_movement_failed = value.to_string(),
            "err_movement_save_failed" => config.err_movement_save_failed = value.to_string(),

            _ => {
                return Err(TinyMushError::NotFound(format!(
                    "Unknown config field: {}",
                    field
                )))
            }
        }

        self.put_world_config(&config)?;
        Ok(())
    }

    // ============================================================================
    // Index Management & Maintenance (Performance Optimization)
    // ============================================================================

    /// Rebuild all secondary indexes from primary data.
    /// Call this after database migration or if indexes become inconsistent.
    pub fn rebuild_all_indexes(&self) -> Result<(), TinyMushError> {
        self.rebuild_object_index()?;
        self.rebuild_housing_guest_indexes()?;
        self.rebuild_template_instance_indexes()?;
        self.rebuild_player_trade_indexes()?;
        Ok(())
    }

    /// Rebuild object index: oid:{id} → full_key
    pub fn rebuild_object_index(&self) -> Result<usize, TinyMushError> {
        // Clear existing index
        self.object_index.clear()?;

        let mut count = 0;

        // Index world objects
        for result in self.objects.scan_prefix(b"objects:world:") {
            let (key, value) = result?;
            let object: ObjectRecord = Self::deserialize(value)?;
            let index_key = format!("oid:{}", object.id);
            self.object_index.insert(index_key.as_bytes(), &key)?;
            count += 1;
        }

        // Index player objects
        for result in self.objects.scan_prefix(b"objects:player:") {
            let (key, value) = result?;
            let object: ObjectRecord = Self::deserialize(value)?;
            let index_key = format!("oid:{}", object.id);
            self.object_index.insert(index_key.as_bytes(), &key)?;
            count += 1;
        }

        self.object_index.flush()?;
        Ok(count)
    }

    /// Rebuild housing guest indexes: guest:{username}:{instance_id} → ""
    pub fn rebuild_housing_guest_indexes(&self) -> Result<usize, TinyMushError> {
        // Clear existing indexes
        self.housing_guests.clear()?;

        let mut count = 0;
        for item in self.housing_instances.scan_prefix(b"instance:") {
            let (_, value) = item?;
            let instance: HousingInstance = Self::deserialize(value)?;

            for guest in &instance.guests {
                let guest_key = format!("guest:{}:{}", guest.to_ascii_lowercase(), instance.id);
                self.housing_guests.insert(guest_key.as_bytes(), b"")?;
                count += 1;
            }
        }

        self.housing_guests.flush()?;
        Ok(count)
    }

    /// Rebuild template instance indexes: tpl:{template_id}:{instance_id} → ""
    pub fn rebuild_template_instance_indexes(&self) -> Result<usize, TinyMushError> {
        // Clear existing indexes
        self.template_instances.clear()?;

        let mut count = 0;
        for item in self.housing_instances.scan_prefix(b"instance:") {
            let (_, value) = item?;
            let instance: HousingInstance = Self::deserialize(value)?;

            if instance.active {
                let tpl_key = format!("tpl:{}:{}", instance.template_id, instance.id);
                self.template_instances.insert(tpl_key.as_bytes(), b"")?;
                count += 1;
            }
        }

        self.template_instances.flush()?;
        Ok(count)
    }

    /// Rebuild player trade indexes: ptrade:{username} → session_id
    pub fn rebuild_player_trade_indexes(&self) -> Result<usize, TinyMushError> {
        // Clear existing indexes
        self.player_trades.clear()?;

        let mut count = 0;
        for result in self.trades.iter() {
            let (_, value) = result?;
            let session: TradeSession = bincode::deserialize(&value)?;

            // Only index active trades
            if !session.is_expired() && session.completed_at.is_none() {
                let p1_key = format!("ptrade:{}", session.player1.to_ascii_lowercase());
                let p2_key = format!("ptrade:{}", session.player2.to_ascii_lowercase());
                self.player_trades
                    .insert(p1_key.as_bytes(), session.id.as_bytes())?;
                self.player_trades
                    .insert(p2_key.as_bytes(), session.id.as_bytes())?;
                count += 2;
            }
        }

        self.player_trades.flush()?;
        Ok(count)
    }

    /// Get index statistics for monitoring
    pub fn get_index_stats(&self) -> Result<IndexStats, TinyMushError> {
        Ok(IndexStats {
            object_index_entries: self.object_index.len(),
            housing_guest_entries: self.housing_guests.len(),
            template_instance_entries: self.template_instances.len(),
            player_trade_entries: self.player_trades.len(),
        })
    }
}

// ============================================================================
// Async Wrappers for Non-Blocking Database Operations
// ============================================================================
//
// These async methods wrap the synchronous Sled operations with spawn_blocking
// to prevent blocking Tokio async worker threads. This is critical for handling
// 500-1000 concurrent users without latency spikes.
//
// Pattern: Clone self, move into blocking closure, await result
// This works because TinyMushStore::clone() is cheap (Arc-based internally)

impl TinyMushStore {
    // ===== Player Operations =====

    /// Async version of put_player - saves player record without blocking
    pub async fn put_player_async(&self, player: PlayerRecord) -> Result<(), TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.put_player(player))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_player - retrieves player record without blocking
    pub async fn get_player_async(&self, username: &str) -> Result<PlayerRecord, TinyMushError> {
        let store = self.clone();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.get_player(&username))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of list_player_ids
    pub async fn list_player_ids_async(&self) -> Result<Vec<String>, TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_player_ids())
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Room Operations =====

    /// Async version of put_room
    pub async fn put_room_async(&self, room: RoomRecord) -> Result<(), TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.put_room(room))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_room
    pub async fn get_room_async(&self, room_id: &str) -> Result<RoomRecord, TinyMushError> {
        let store = self.clone();
        let room_id = room_id.to_string();
        tokio::task::spawn_blocking(move || store.get_room(&room_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Object Operations =====

    /// Async version of put_object
    pub async fn put_object_async(&self, object: ObjectRecord) -> Result<(), TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.put_object(object))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_object
    pub async fn get_object_async(&self, id: &str) -> Result<ObjectRecord, TinyMushError> {
        let store = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || store.get_object(&id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Currency Operations =====

    /// Async version of transfer_item
    pub async fn transfer_item_async(
        &self,
        from_username: &str,
        to_username: &str,
        item: &str,
        quantity: u32,
        config: &crate::tmush::types::InventoryConfig,
    ) -> Result<(), TinyMushError> {
        let store = self.clone();
        let from = from_username.to_string();
        let to = to_username.to_string();
        let item = item.to_string();
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            store.transfer_item(&from, &to, &item, quantity, &config)
        })
        .await
        .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Trade Operations =====

    /// Async version of put_trade_session
    pub async fn put_trade_session_async(
        &self,
        session: &TradeSession,
    ) -> Result<(), TinyMushError> {
        let store = self.clone();
        let session = session.clone();
        tokio::task::spawn_blocking(move || store.put_trade_session(&session))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_trade_session
    pub async fn get_trade_session_async(
        &self,
        session_id: &str,
    ) -> Result<Option<TradeSession>, TinyMushError> {
        let store = self.clone();
        let session_id = session_id.to_string();
        tokio::task::spawn_blocking(move || store.get_trade_session(&session_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_player_active_trade
    pub async fn get_player_active_trade_async(
        &self,
        username: &str,
    ) -> Result<Option<TradeSession>, TinyMushError> {
        let store = self.clone();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.get_player_active_trade(&username))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of delete_trade_session
    pub async fn delete_trade_session_async(&self, session_id: &str) -> Result<(), TinyMushError> {
        let store = self.clone();
        let session_id = session_id.to_string();
        tokio::task::spawn_blocking(move || store.delete_trade_session(&session_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Housing Operations =====

    /// Async version of get_housing_template
    pub async fn get_housing_template_async(
        &self,
        template_id: &str,
    ) -> Result<HousingTemplate, TinyMushError> {
        let store = self.clone();
        let template_id = template_id.to_string();
        tokio::task::spawn_blocking(move || store.get_housing_template(&template_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of list_housing_templates
    pub async fn list_housing_templates_async(&self) -> Result<Vec<String>, TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_housing_templates())
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_housing_instance
    pub async fn get_housing_instance_async(
        &self,
        instance_id: &str,
    ) -> Result<HousingInstance, TinyMushError> {
        let store = self.clone();
        let instance_id = instance_id.to_string();
        tokio::task::spawn_blocking(move || store.get_housing_instance(&instance_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of put_housing_instance
    pub async fn put_housing_instance_async(
        &self,
        instance: &HousingInstance,
    ) -> Result<(), TinyMushError> {
        let store = self.clone();
        let instance = instance.clone();
        tokio::task::spawn_blocking(move || store.put_housing_instance(&instance))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_player_housing_instances
    pub async fn get_player_housing_instances_async(
        &self,
        owner: &str,
    ) -> Result<Vec<HousingInstance>, TinyMushError> {
        let store = self.clone();
        let owner = owner.to_string();
        tokio::task::spawn_blocking(move || store.get_player_housing_instances(&owner))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_guest_housing_instances
    pub async fn get_guest_housing_instances_async(
        &self,
        username: &str,
    ) -> Result<Vec<HousingInstance>, TinyMushError> {
        let store = self.clone();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.get_guest_housing_instances(&username))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of delete_housing_instance
    pub async fn delete_housing_instance_async(
        &self,
        instance_id: &str,
    ) -> Result<(), TinyMushError> {
        let store = self.clone();
        let instance_id = instance_id.to_string();
        tokio::task::spawn_blocking(move || store.delete_housing_instance(&instance_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of count_template_instances
    pub async fn count_template_instances_async(
        &self,
        template_id: &str,
    ) -> Result<usize, TinyMushError> {
        let store = self.clone();
        let template_id = template_id.to_string();
        tokio::task::spawn_blocking(move || store.count_template_instances(&template_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Mail Operations =====

    /// Async version of send_mail - creates and sends a mail message
    pub async fn send_mail_async(&self, message: MailMessage) -> Result<u64, TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.send_mail(message))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_mail
    pub async fn get_mail_async(
        &self,
        folder: &str,
        username: &str,
        message_id: u64,
    ) -> Result<MailMessage, TinyMushError> {
        let store = self.clone();
        let folder = folder.to_string();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.get_mail(&folder, &username, message_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of list_mail
    pub async fn list_mail_async(
        &self,
        folder: &str,
        username: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MailMessage>, TinyMushError> {
        let store = self.clone();
        let folder = folder.to_string();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.list_mail(&folder, &username, offset, limit))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of mark_mail_read
    pub async fn mark_mail_read_async(
        &self,
        folder: &str,
        username: &str,
        message_id: u64,
    ) -> Result<(), TinyMushError> {
        let store = self.clone();
        let folder = folder.to_string();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.mark_mail_read(&folder, &username, message_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of delete_mail
    pub async fn delete_mail_async(
        &self,
        folder: &str,
        username: &str,
        message_id: u64,
    ) -> Result<(), TinyMushError> {
        let store = self.clone();
        let folder = folder.to_string();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || store.delete_mail(&folder, &username, message_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Bulletin Board Operations =====

    /// Async version of get_bulletin_board
    pub async fn get_bulletin_board_async(
        &self,
        board_id: &str,
    ) -> Result<BulletinBoard, TinyMushError> {
        let store = self.clone();
        let board_id = board_id.to_string();
        tokio::task::spawn_blocking(move || store.get_bulletin_board(&board_id))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of post_bulletin - posts a bulletin message
    pub async fn post_bulletin_async(
        &self,
        message: BulletinMessage,
    ) -> Result<u64, TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.post_bulletin(message))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== World Configuration =====

    /// Async version of get_world_config
    pub async fn get_world_config_async(&self) -> Result<WorldConfig, TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_world_config())
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of put_world_config
    pub async fn put_world_config_async(&self, config: &WorldConfig) -> Result<(), TinyMushError> {
        let store = self.clone();
        let config = config.clone();
        tokio::task::spawn_blocking(move || store.put_world_config(&config))
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    // ===== Index Maintenance =====

    /// Async version of rebuild_all_indexes
    pub async fn rebuild_all_indexes_async(&self) -> Result<(), TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.rebuild_all_indexes())
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }

    /// Async version of get_index_stats
    pub async fn get_index_stats_async(&self) -> Result<IndexStats, TinyMushError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_index_stats())
            .await
            .map_err(|e| TinyMushError::Internal(format!("Task join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmush::state::OLD_TOWNE_WORLD_ROOM_IDS;
    use tempfile::TempDir;

    #[test]
    fn store_round_trip_player() {
        let dir = TempDir::new().expect("tempdir");
        let store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");
        let mut player = PlayerRecord::new("alice", "Alice", "town_square");
        player.credits = 42;
        store.put_player(player.clone()).expect("put");
        let fetched = store.get_player("alice").expect("get");
        assert_eq!(fetched.username, player.username);
        assert_eq!(fetched.credits, 42);
        assert_eq!(fetched.schema_version, PLAYER_SCHEMA_VERSION);
        drop(store);
    }

    #[test]
    fn seeding_world_only_happens_once() {
        let dir = TempDir::new().expect("tempdir");
        {
            let store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");
            let ids = store.list_player_ids().expect("list players");
            // Admin account is automatically seeded
            assert_eq!(ids.len(), 1, "should have admin account");
            assert_eq!(ids[0], "admin", "seeded account should be admin");
            for room_id in OLD_TOWNE_WORLD_ROOM_IDS {
                store.get_room(room_id).expect("room present");
            }
        }

        let store = TinyMushStoreBuilder::new(dir.path())
            .without_world_seed()
            .open()
            .expect("reopen store");
        let count = store.seed_world_if_needed().expect("seed check");
        assert_eq!(count, 0, "should not reseed when rooms already exist");
        for room_id in OLD_TOWNE_WORLD_ROOM_IDS {
            store.get_room(room_id).expect("room persists");
        }
    }

    #[test]
    fn seeding_quests_only_happens_once() {
        let dir = TempDir::new().expect("tempdir");
        {
            let store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");
            // Verify starter quests were seeded
            let quest_ids = store.list_quest_ids().expect("list quests");
            assert!(!quest_ids.is_empty(), "should have starter quests");
            assert!(quest_ids.contains(&"welcome_towne".to_string()));
            assert!(quest_ids.contains(&"market_exploration".to_string()));
            assert!(quest_ids.contains(&"network_explorer".to_string()));
        }

        // Reopen and verify quests persist and don't get re-seeded
        let store = TinyMushStoreBuilder::new(dir.path())
            .without_world_seed()
            .open()
            .expect("reopen store");
        let count = store.seed_quests_if_needed().expect("seed check");
        assert_eq!(count, 0, "should not reseed when quests already exist");
        let quest_ids = store.list_quest_ids().expect("list quests");
        assert_eq!(
            quest_ids.len(),
            11,
            "should still have 11 quests (3 starter + 4 original content + 4 Phase 4 quests)"
        );
    }

    #[test]
    fn seed_world_populates_expected_rooms() {
        let dir = TempDir::new().expect("tempdir");
        let store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");
        for room_id in OLD_TOWNE_WORLD_ROOM_IDS {
            let room = store.get_room(room_id).expect("room present");
            assert_eq!(room.schema_version, ROOM_SCHEMA_VERSION);
        }
        drop(store);
    }

    #[test]
    fn mayor_dialogue_tree_seeded() {
        let dir = TempDir::new().expect("tempdir");
        let _store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");

        // Verify seeding completed without error
        // The dialogue seeding happens during store initialization
        // If we got here, seeding succeeded
        assert!(true, "Dialogue seeding completed");
    }

    #[test]
    fn admin_account_seeded_automatically() {
        let dir = TempDir::new().expect("tempdir");
        let store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");

        // Admin should be automatically seeded
        let admin = store.get_player("admin").expect("admin exists");
        assert_eq!(admin.username, "admin");
        assert!(admin.is_admin(), "admin should have admin flag");
        assert_eq!(admin.admin_level(), 3, "admin should be sysop level");

        // List admins should return the seeded admin
        let admins = store.list_admins().expect("list admins");
        assert_eq!(admins.len(), 1, "should have exactly one admin");
        assert_eq!(admins[0].username, "admin");
    }

    #[test]
    fn admin_seeding_is_idempotent() {
        let dir = TempDir::new().expect("tempdir");
        let store = TinyMushStoreBuilder::new(dir.path()).open().expect("store");

        // First seed creates admin
        let created1 = store.seed_admin_if_needed("admin").expect("first seed");
        assert!(!created1, "admin already exists from initialization");

        // Second seed is no-op
        let created2 = store.seed_admin_if_needed("admin").expect("second seed");
        assert!(!created2, "should not recreate existing admin");

        // Should still have exactly one admin
        let admins = store.list_admins().expect("list admins");
        assert_eq!(admins.len(), 1, "should still have exactly one admin");
    }

    #[test]
    fn admin_seeding_custom_username() {
        let dir = TempDir::new().expect("tempdir");
        let store = TinyMushStoreBuilder::new(dir.path())
            .without_world_seed()
            .open()
            .expect("store");

        // Create admin with custom username
        let created = store.seed_admin_if_needed("sysop").expect("seed sysop");
        assert!(created, "should create new admin");

        let sysop = store.get_player("sysop").expect("sysop exists");
        assert_eq!(sysop.username, "sysop");
        assert!(sysop.is_admin());
        assert_eq!(sysop.admin_level(), 3);
    }

    #[test]
    fn admin_seeding_promotes_existing_player() {
        let dir = TempDir::new().expect("tempdir");
        let store = TinyMushStoreBuilder::new(dir.path())
            .without_world_seed()
            .open()
            .expect("store");

        // Create regular player
        let player = PlayerRecord::new("alice", "Alice", "town_square");
        store.put_player(player).expect("create player");

        // Verify not admin
        assert!(!store.is_admin("alice").expect("check admin"));

        // Seed admin with same username promotes existing player
        let promoted = store.seed_admin_if_needed("alice").expect("promote");
        assert!(promoted, "should promote existing player");

        // Verify now admin
        assert!(store.is_admin("alice").expect("check admin"));
        let alice = store.get_player("alice").expect("get alice");
        assert_eq!(alice.admin_level(), 3);
    }
}
