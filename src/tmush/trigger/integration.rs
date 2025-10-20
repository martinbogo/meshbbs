//! Integration helpers for executing triggers from game commands.
//!
//! This module provides convenience functions for executing triggers when
//! players interact with objects. Each function handles fetching required
//! records, creating the execution context, and running the trigger.

use super::admin::{check_trigger_allowed, record_trigger_execution};
use super::{execute_trigger, TriggerContext, TriggerResult};
use crate::tmush::storage::TinyMushStore;
use crate::tmush::types::{ObjectRecord, ObjectTrigger};
use log::{error, warn};

/// Execute OnLook trigger when player examines an object
///
/// # Arguments
/// * `object` - The object being looked at
/// * `player_username` - Username of the player looking
/// * `room_id` - Current room ID
/// * `store` - Storage reference
///
/// # Returns
/// Vec of messages to display to the player (empty if no trigger or trigger failed)
pub fn execute_on_look(
    object: &ObjectRecord,
    player_username: &str,
    room_id: &str,
    store: &TinyMushStore,
) -> Vec<String> {
    // Check if this object has an OnLook trigger
    let script = match object.actions.get(&ObjectTrigger::OnLook) {
        Some(s) => s,
        None => return vec![],
    };

    // Check rate limiting
    if let Err(reason) = check_trigger_allowed(&object.id, player_username) {
        warn!("execute_on_look: Rate limited - {}", reason);
        return vec![];
    }

    // Get player record
    let player = match store.get_player(player_username) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "execute_on_look: Failed to get player {}: {}",
                player_username, e
            );
            return vec![];
        }
    };

    // Get room record
    let room = match store.get_room(room_id) {
        Ok(r) => r,
        Err(e) => {
            warn!("execute_on_look: Failed to get room {}: {}", room_id, e);
            return vec![];
        }
    };

    // Create context
    let mut context = TriggerContext::new(&player, object, &room);

    // Execute the trigger
    let result = match execute_trigger(ObjectTrigger::OnLook, script, &mut context, store) {
        Ok(TriggerResult::Success(messages)) => {
            // Record successful execution for rate limiting
            record_trigger_execution(&object.id, player_username);
            messages
        }
        Ok(TriggerResult::NoScript) => vec![],
        Ok(TriggerResult::Skipped) => vec![],
        Ok(TriggerResult::RateLimited) => vec![],
        Ok(TriggerResult::Failed(_)) => vec![],
        Ok(TriggerResult::TimedOut) => vec![],
        Err(e) => {
            error!("execute_on_look: Trigger execution failed: {}", e);
            vec![]
        }
    };

    result
}

/// Execute OnTake trigger when player picks up an object
///
/// # Arguments
/// * `object` - The object being taken
/// * `player_username` - Username of the player taking
/// * `room_id` - Current room ID
/// * `store` - Storage reference
///
/// # Returns
/// Vec of messages to display to the player
pub fn execute_on_take(
    object: &ObjectRecord,
    player_username: &str,
    room_id: &str,
    store: &TinyMushStore,
) -> Vec<String> {
    let script = match object.actions.get(&ObjectTrigger::OnTake) {
        Some(s) => s,
        None => return vec![],
    };

    let player = match store.get_player(player_username) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "execute_on_take: Failed to get player {}: {}",
                player_username, e
            );
            return vec![];
        }
    };

    let room = match store.get_room(room_id) {
        Ok(r) => r,
        Err(e) => {
            warn!("execute_on_take: Failed to get room {}: {}", room_id, e);
            return vec![];
        }
    };

    let mut context = TriggerContext::new(&player, object, &room);

    match execute_trigger(ObjectTrigger::OnTake, script, &mut context, store) {
        Ok(TriggerResult::Success(messages)) => messages,
        Ok(TriggerResult::NoScript) => vec![],
        Ok(TriggerResult::Skipped) => vec![],
        Ok(TriggerResult::RateLimited) => vec![],
        Ok(TriggerResult::Failed(_)) => vec![],
        Ok(TriggerResult::TimedOut) => vec![],
        Err(e) => {
            error!("execute_on_take: Trigger execution failed: {}", e);
            vec![]
        }
    }
}

/// Execute OnDrop trigger when player drops an object
///
/// # Arguments
/// * `object` - The object being dropped
/// * `player_username` - Username of the player dropping
/// * `room_id` - Current room ID
/// * `store` - Storage reference
///
/// # Returns
/// Vec of messages to display to the player
pub fn execute_on_drop(
    object: &ObjectRecord,
    player_username: &str,
    room_id: &str,
    store: &TinyMushStore,
) -> Vec<String> {
    let script = match object.actions.get(&ObjectTrigger::OnDrop) {
        Some(s) => s,
        None => return vec![],
    };

    let player = match store.get_player(player_username) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "execute_on_drop: Failed to get player {}: {}",
                player_username, e
            );
            return vec![];
        }
    };

    let room = match store.get_room(room_id) {
        Ok(r) => r,
        Err(e) => {
            warn!("execute_on_drop: Failed to get room {}: {}", room_id, e);
            return vec![];
        }
    };

    let mut context = TriggerContext::new(&player, object, &room);

    match execute_trigger(ObjectTrigger::OnDrop, script, &mut context, store) {
        Ok(TriggerResult::Success(messages)) => messages,
        Ok(TriggerResult::NoScript) => vec![],
        Ok(TriggerResult::Skipped) => vec![],
        Ok(TriggerResult::RateLimited) => vec![],
        Ok(TriggerResult::Failed(_)) => vec![],
        Ok(TriggerResult::TimedOut) => vec![],
        Err(e) => {
            error!("execute_on_drop: Trigger execution failed: {}", e);
            vec![]
        }
    }
}

/// Execute OnUse trigger when player uses an object
///
/// # Arguments
/// * `object` - The object being used
/// * `player_username` - Username of the player using
/// * `room_id` - Current room ID
/// * `store` - Storage reference
///
/// # Returns
/// Vec of messages to display to the player
pub fn execute_on_use(
    object: &ObjectRecord,
    player_username: &str,
    room_id: &str,
    store: &TinyMushStore,
) -> Vec<String> {
    let script = match object.actions.get(&ObjectTrigger::OnUse) {
        Some(s) => s,
        None => return vec![],
    };

    let player = match store.get_player(player_username) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "execute_on_use: Failed to get player {}: {}",
                player_username, e
            );
            return vec![];
        }
    };

    let room = match store.get_room(room_id) {
        Ok(r) => r,
        Err(e) => {
            warn!("execute_on_use: Failed to get room {}: {}", room_id, e);
            return vec![];
        }
    };

    let mut context = TriggerContext::new(&player, object, &room);

    match execute_trigger(ObjectTrigger::OnUse, script, &mut context, store) {
        Ok(TriggerResult::Success(messages)) => messages,
        Ok(TriggerResult::NoScript) => vec![],
        Ok(TriggerResult::Skipped) => vec![],
        Ok(TriggerResult::RateLimited) => vec!["⏰ Action rate limited.".to_string()],
        Ok(TriggerResult::Failed(reason)) => {
            warn!("execute_on_use: Trigger failed: {}", reason);
            vec![format!("⚠️  Trigger error: {}", reason)]
        }
        Ok(TriggerResult::TimedOut) => vec!["⏱️  Trigger execution timed out.".to_string()],
        Err(e) => {
            error!("execute_on_use: Trigger execution failed: {}", e);
            vec![format!("⚠️  Trigger system error: {}", e)]
        }
    }
}

/// Execute OnPoke trigger when player pokes an object (interactive action)
///
/// # Arguments
/// * `object` - The object being poked
/// * `player_username` - Username of the player poking
/// * `room_id` - Current room ID
/// * `store` - Storage reference
///
/// # Returns
/// Vec of messages to display to the player
pub fn execute_on_poke(
    object: &ObjectRecord,
    player_username: &str,
    room_id: &str,
    store: &TinyMushStore,
) -> Vec<String> {
    let script = match object.actions.get(&ObjectTrigger::OnPoke) {
        Some(s) => s,
        None => return vec![],
    };

    let player = match store.get_player(player_username) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "execute_on_poke: Failed to get player {}: {}",
                player_username, e
            );
            return vec![];
        }
    };

    let room = match store.get_room(room_id) {
        Ok(r) => r,
        Err(e) => {
            warn!("execute_on_poke: Failed to get room {}: {}", room_id, e);
            return vec![];
        }
    };

    let mut context = TriggerContext::new(&player, object, &room);

    match execute_trigger(ObjectTrigger::OnPoke, script, &mut context, store) {
        Ok(TriggerResult::Success(messages)) => messages,
        Ok(TriggerResult::NoScript) => vec![],
        Ok(TriggerResult::Skipped) => vec![],
        Ok(TriggerResult::RateLimited) => vec![],
        Ok(TriggerResult::Failed(_)) => vec![],
        Ok(TriggerResult::TimedOut) => vec![],
        Err(e) => {
            error!("execute_on_poke: Trigger execution failed: {}", e);
            vec![]
        }
    }
}

/// Execute all OnEnter triggers for objects in a room
///
/// This fires when a player enters a room, checking all objects in the room
/// for OnEnter triggers and executing them in order.
pub fn execute_room_on_enter(
    player_username: &str,
    room_id: &str,
    store: &TinyMushStore,
) -> Vec<String> {
    let mut all_messages = Vec::new();

    // Get the room
    let room = match store.get_room(room_id) {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "execute_room_on_enter: Failed to get room {}: {}",
                room_id, e
            );
            return vec![];
        }
    };

    // Get player record
    let player = match store.get_player(player_username) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "execute_room_on_enter: Failed to get player {}: {}",
                player_username, e
            );
            return vec![];
        }
    };

    // For each object in the room
    for object_id in &room.items {
        // Get the object
        let object = match store.get_object(object_id) {
            Ok(obj) => obj,
            Err(e) => {
                warn!(
                    "execute_room_on_enter: Failed to get object {}: {}",
                    object_id, e
                );
                continue; // Skip this object but process others
            }
        };

        // Check if it has an OnEnter trigger
        let script = match object.actions.get(&ObjectTrigger::OnEnter) {
            Some(s) => s,
            None => continue,
        };

        // Create context
        let mut context = TriggerContext::new(&player, &object, &room);

        // Execute the trigger
        match execute_trigger(ObjectTrigger::OnEnter, script, &mut context, store) {
            Ok(TriggerResult::Success(messages)) => {
                all_messages.extend(messages);
            }
            Ok(TriggerResult::NoScript) => {}
            Ok(TriggerResult::Skipped) => {}
            Ok(TriggerResult::RateLimited) => {}
            Ok(TriggerResult::Failed(_)) => {}
            Ok(TriggerResult::TimedOut) => {}
            Err(e) => {
                error!(
                    "execute_room_on_enter: Trigger execution failed for object {}: {}",
                    object_id, e
                );
            }
        }
    }

    all_messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmush::types::{CurrencyAmount, ObjectOwner, RoomOwner, RoomVisibility};
    use chrono::Utc;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_store() -> (TempDir, TinyMushStore) {
        let temp = TempDir::new().unwrap();
        let store = TinyMushStore::open(temp.path()).unwrap();
        (temp, store)
    }

    #[test]
    fn test_execute_on_look_with_message() {
        let (_temp, store) = create_test_store();

        // Create player
        let player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        store.put_player(player).unwrap();

        // Create room
        let room = crate::tmush::types::RoomRecord {
            id: "test_room".to_string(),
            name: "Test Room".to_string(),
            short_desc: "A test room".to_string(),
            long_desc: "This is a test room".to_string(),
            owner: RoomOwner::World,
            created_at: Utc::now(),
            visibility: RoomVisibility::Public,
            exits: HashMap::new(),
            items: vec![],
            flags: vec![],
            max_capacity: 10,
            housing_filter_tags: vec![],
            locked: false,
            schema_version: 1,
        };
        store.put_room(room).unwrap();

        // Create object with OnLook trigger
        let mut actions = HashMap::new();
        actions.insert(
            ObjectTrigger::OnLook,
            "message(\"You see something special!\")".to_string(),
        );

        let object = ObjectRecord {
            id: "test_obj".to_string(),
            name: "Test Object".to_string(),
            description: "A test".to_string(),
            owner: ObjectOwner::World,
            created_at: Utc::now(),
            weight: 1,
            currency_value: CurrencyAmount::default(),
            value: 0,
            takeable: true,
            usable: false,
            actions,
            flags: Vec::new(),
            locked: false,
            ownership_history: Vec::new(),
            schema_version: 1,
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: String::new(),
        };

        // Execute OnLook
        let messages = execute_on_look(&object, "test_player", "test_room", &store);

        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("something special"));
    }

    #[test]
    fn test_execute_on_look_no_trigger() {
        let (_temp, store) = create_test_store();

        // Create player
        let player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        store.put_player(player).unwrap();

        // Create room
        let room = crate::tmush::types::RoomRecord {
            id: "test_room".to_string(),
            name: "Test Room".to_string(),
            short_desc: "A test room".to_string(),
            long_desc: "This is a test room".to_string(),
            owner: RoomOwner::World,
            created_at: Utc::now(),
            visibility: RoomVisibility::Public,
            exits: HashMap::new(),
            items: vec![],
            flags: vec![],
            max_capacity: 10,
            housing_filter_tags: vec![],
            locked: false,
            schema_version: 1,
        };
        store.put_room(room).unwrap();

        // Create object without OnLook trigger
        let object = ObjectRecord {
            id: "test_obj".to_string(),
            name: "Test Object".to_string(),
            description: "A test".to_string(),
            owner: ObjectOwner::World,
            created_at: Utc::now(),
            weight: 1,
            currency_value: CurrencyAmount::default(),
            value: 0,
            takeable: true,
            usable: false,
            actions: HashMap::new(),
            flags: Vec::new(),
            locked: false,
            ownership_history: Vec::new(),
            schema_version: 1,
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: String::new(),
        };

        // Execute OnLook
        let messages = execute_on_look(&object, "test_player", "test_room", &store);

        assert_eq!(messages.len(), 0);
    }
}
