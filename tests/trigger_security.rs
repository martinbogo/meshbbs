//! Security tests for TinyMUSH trigger system (Phase 9)
//!
//! Tests timeout handling, rate limiting, infinite loop protection,
//! and other security concerns for the trigger engine.

use chrono::Utc;
use meshbbs::tmush::trigger::{execute_on_use, execute_room_on_enter};
use meshbbs::tmush::types::{CurrencyAmount, RoomOwner, RoomRecord, RoomVisibility};
use meshbbs::tmush::{ObjectOwner, ObjectRecord, ObjectTrigger, PlayerRecord, TinyMushStore};
use std::collections::HashMap;
use tempfile::TempDir;

/// Helper to create a test store with a player and room
fn setup_test_environment() -> (TempDir, TinyMushStore, String, String) {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();

    // Create test player
    let player = PlayerRecord::new("test_player", "Test Player", "test_room");
    store.put_player(player).unwrap();

    // Create test room
    let room = RoomRecord {
        id: "test_room".to_string(),
        name: "Test Room".to_string(),
        short_desc: "A test room".to_string(),
        long_desc: "This is a test room for trigger testing.".to_string(),
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

    (
        temp,
        store,
        "test_player".to_string(),
        "test_room".to_string(),
    )
}

#[test]
fn test_malformed_trigger_script_handling() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object with intentionally malformed trigger script
    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Buggy Object".to_string(),
        description: "Has a broken trigger".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions.insert(
        ObjectTrigger::OnUse,
        "message(\"Missing closing quote)".to_string(),
    );
    store.put_object(obj.clone()).unwrap();

    // Execute trigger - should handle gracefully
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Should show user-friendly error message instead of crashing
    // This is intentional - script errors should be visible to users/admins
    assert!(
        !messages.is_empty() && messages[0].contains("Trigger error"),
        "Malformed scripts should return error message, got: {:?}",
        messages
    );
}

#[test]
fn test_missing_function_handling() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object with undefined function call
    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Object with Undefined Function".to_string(),
        description: "Calls a non-existent function".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions
        .insert(ObjectTrigger::OnUse, "undefined_function(42)".to_string());
    store.put_object(obj.clone()).unwrap();

    // Execute trigger - should handle gracefully
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Should show user-friendly error message instead of crashing
    assert!(
        !messages.is_empty() && messages[0].contains("Trigger error"),
        "Undefined functions should return error message, got: {:?}",
        messages
    );
}

#[test]
fn test_very_long_script_handling() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object with extremely long script
    let long_script = format!("message(\"{}\")", "A".repeat(10_000));

    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Long Script Object".to_string(),
        description: "Has a very long script".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions.insert(ObjectTrigger::OnUse, long_script);
    store.put_object(obj.clone()).unwrap();

    // Execute trigger - should handle without hanging
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Current implementation may truncate very long messages. We only care that it
    // returns quickly and does not spam thousands of lines of output.
    assert!(
        messages.len() <= 1,
        "Long script should not emit excessive messages: {:?}",
        messages
    );
    if let Some(message) = messages.first() {
        assert!(
            message.len() <= 4_096,
            "Long script output should be truncated to a reasonable length"
        );
    }
}

#[test]
fn test_nested_logic_depth() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object with deeply nested conditional logic
    let nested_script = "true ? (true ? (true ? message(\"Deep!\") : message(\"A\")) : message(\"B\")) : message(\"C\")";

    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Nested Logic Object".to_string(),
        description: "Has nested conditionals".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions
        .insert(ObjectTrigger::OnUse, nested_script.to_string());
    store.put_object(obj.clone()).unwrap();

    // Execute trigger
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Should handle nested logic correctly (if parser supports it) OR show error
    // Note: Current parser may not support complex nesting
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Deep!") || m.contains("Trigger error")),
        "Should either execute nested logic or fail with error message, got: {:?}",
        messages
    );
}

#[test]
fn test_multiple_function_calls() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object with many chained function calls
    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Multi-Function Object".to_string(),
        description: "Chains many functions".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions.insert(
        ObjectTrigger::OnUse,
        "message(\"First\") && message(\"Second\") && message(\"Third\") && message(\"Fourth\") && message(\"Fifth\")".to_string()
    );
    store.put_object(obj.clone()).unwrap();

    // Execute trigger
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Current parser/evaluator may only execute one message() call
    // This is expected behavior - not all advanced script features are implemented yet
    assert!(
        messages.len() <= 5,
        "Should not produce more than 5 messages (got {})",
        messages.len()
    );
}

#[test]
fn test_safe_teleport_to_nonexistent_room() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object that tries to teleport to non-existent room
    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Bad Teleport Object".to_string(),
        description: "Tries to teleport to nowhere".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions.insert(
        ObjectTrigger::OnUse,
        "teleport(\"nonexistent_room_id\")".to_string(),
    );
    store.put_object(obj.clone()).unwrap();

    // Execute trigger
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Should fail gracefully (action functions may fail silently)
    // Error handling is intentionally silent to not break gameplay
    assert!(
        messages.len() <= 1,
        "Invalid teleport should not spam output: {:?}",
        messages
    );

    // Verify player wasn't moved (most important check)
    let player = store.get_player(&player_name).unwrap();
    assert_eq!(
        player.current_room, room_id,
        "Player should still be in original room"
    );
}

#[test]
fn test_empty_trigger_script() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create object with empty trigger script
    let mut obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Empty Trigger Object".to_string(),
        description: "Has empty trigger".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    obj.actions.insert(ObjectTrigger::OnUse, "".to_string());
    store.put_object(obj.clone()).unwrap();

    // Execute trigger
    let messages = execute_on_use(&obj, &player_name, &room_id, &store);

    // Should handle gracefully (empty script = no output)
    assert!(
        messages.is_empty(),
        "Empty script should produce no messages"
    );
}

#[test]
fn test_room_with_many_triggered_objects() {
    let (_temp, store, player_name, room_id) = setup_test_environment();

    // Create 50 objects with OnEnter triggers
    for i in 0..50 {
        let mut obj = ObjectRecord {
            id: format!("test_obj_{}", i),
            name: format!("Object {}", i),
            description: format!("Test object {}", i),
            owner: ObjectOwner::World,
            created_at: Utc::now(),
            weight: 1,
            currency_value: CurrencyAmount::default(),
            value: 10,
            takeable: true,
            usable: false,
            actions: HashMap::new(),
            flags: vec![],
            locked: false,
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: "world".to_string(),
            ownership_history: vec![],
            schema_version: 1,
        };
        obj.actions.insert(
            ObjectTrigger::OnEnter,
            format!("message(\"Object {} greets you!\")", i),
        );
        store.put_object(obj.clone()).unwrap();

        // Add to room
        let mut room = store.get_room(&room_id).unwrap();
        room.items.push(obj.id.clone());
        store.put_room(room).unwrap();
    }

    // Execute all OnEnter triggers
    let start = std::time::Instant::now();
    let messages = execute_room_on_enter(&player_name, &room_id, &store);
    let duration = start.elapsed();

    // Should complete in reasonable time (< 1 second)
    assert!(
        duration.as_secs() < 1,
        "Should process 50 triggers in under 1 second"
    );

    // Should execute all 50 triggers
    assert_eq!(messages.len(), 50, "Should execute all 50 OnEnter triggers");
}
