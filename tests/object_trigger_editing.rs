//! Integration test for @OBJECT trigger editing functionality.

use meshbbs::tmush::storage::TinyMushStore;
use meshbbs::tmush::types::{ObjectOwner, ObjectRecord, ObjectTrigger, OBJECT_SCHEMA_VERSION};
use std::collections::HashMap;
use tempfile::TempDir;

/// Test creating an object and adding/modifying/removing triggers via API
#[test]
fn test_object_trigger_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp_dir.path()).unwrap();

    // Create a basic object
    let mut test_object = ObjectRecord {
        id: "test_mushroom".to_string(),
        name: "Test Mushroom".to_string(),
        description: "A test mushroom for trigger editing.".to_string(),
        owner: ObjectOwner::World,
        created_at: chrono::Utc::now(),
        weight: 1,
        currency_value: Default::default(),
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
        schema_version: OBJECT_SCHEMA_VERSION,
    };

    // Save initial object
    store.put_object(test_object.clone()).unwrap();

    // Verify object starts with no triggers
    let loaded = store.get_object("test_mushroom").unwrap();
    assert_eq!(
        loaded.actions.len(),
        0,
        "Object should start with no triggers"
    );

    // Add OnEnter trigger
    test_object.actions.insert(
        ObjectTrigger::OnEnter,
        "message(\"🍄 The mushroom hums!\")".to_string(),
    );
    store.put_object(test_object.clone()).unwrap();

    let loaded = store.get_object("test_mushroom").unwrap();
    assert_eq!(loaded.actions.len(), 1, "Object should have 1 trigger");
    assert_eq!(
        loaded.actions.get(&ObjectTrigger::OnEnter),
        Some(&"message(\"🍄 The mushroom hums!\")".to_string())
    );

    // Add OnLook trigger
    test_object.actions.insert(
        ObjectTrigger::OnLook,
        "message(\"It glows softly.\")".to_string(),
    );
    store.put_object(test_object.clone()).unwrap();

    let loaded = store.get_object("test_mushroom").unwrap();
    assert_eq!(loaded.actions.len(), 2, "Object should have 2 triggers");

    // Update OnEnter trigger
    test_object.actions.insert(
        ObjectTrigger::OnEnter,
        "message(\"🍄 The mushroom sings!\")".to_string(),
    );
    store.put_object(test_object.clone()).unwrap();

    let loaded = store.get_object("test_mushroom").unwrap();
    assert_eq!(
        loaded.actions.get(&ObjectTrigger::OnEnter),
        Some(&"message(\"🍄 The mushroom sings!\")".to_string()),
        "OnEnter trigger should be updated"
    );

    // Remove OnLook trigger
    test_object.actions.remove(&ObjectTrigger::OnLook);
    store.put_object(test_object.clone()).unwrap();

    let loaded = store.get_object("test_mushroom").unwrap();
    assert_eq!(
        loaded.actions.len(),
        1,
        "Object should have 1 trigger after removal"
    );
    assert!(
        !loaded.actions.contains_key(&ObjectTrigger::OnLook),
        "OnLook trigger should be removed"
    );

    // Remove all triggers
    test_object.actions.clear();
    store.put_object(test_object.clone()).unwrap();

    let loaded = store.get_object("test_mushroom").unwrap();
    assert_eq!(loaded.actions.len(), 0, "Object should have no triggers");
}

/// Test that all trigger types can be added
#[test]
fn test_all_trigger_types() {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp_dir.path()).unwrap();

    let mut test_object = ObjectRecord {
        id: "test_multi_trigger".to_string(),
        name: "Multi Trigger Object".to_string(),
        description: "An object with all trigger types.".to_string(),
        owner: ObjectOwner::World,
        created_at: chrono::Utc::now(),
        weight: 1,
        currency_value: Default::default(),
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
        schema_version: OBJECT_SCHEMA_VERSION,
    };

    // Add all trigger types
    test_object
        .actions
        .insert(ObjectTrigger::OnEnter, "message(\"OnEnter\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnLook, "message(\"OnLook\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnTake, "message(\"OnTake\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnDrop, "message(\"OnDrop\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnUse, "message(\"OnUse\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnPoke, "message(\"OnPoke\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnFollow, "message(\"OnFollow\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnIdle, "message(\"OnIdle\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnCombat, "message(\"OnCombat\")".to_string());
    test_object
        .actions
        .insert(ObjectTrigger::OnHeal, "message(\"OnHeal\")".to_string());

    store.put_object(test_object).unwrap();

    let loaded = store.get_object("test_multi_trigger").unwrap();
    assert_eq!(
        loaded.actions.len(),
        10,
        "Object should have all 10 trigger types"
    );

    // Verify each trigger
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnEnter));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnLook));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnTake));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnDrop));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnUse));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnPoke));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnFollow));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnIdle));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnCombat));
    assert!(loaded.actions.contains_key(&ObjectTrigger::OnHeal));
}

/// Test complex trigger scripts with multiple commands
#[test]
fn test_complex_trigger_scripts() {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp_dir.path()).unwrap();

    let mut potion = ObjectRecord {
        id: "test_healing_potion".to_string(),
        name: "Healing Potion".to_string(),
        description: "A glowing red potion.".to_string(),
        owner: ObjectOwner::World,
        created_at: chrono::Utc::now(),
        weight: 1,
        currency_value: Default::default(),
        value: 50,
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
        schema_version: OBJECT_SCHEMA_VERSION,
    };

    // Add complex trigger with chained commands
    potion.actions.insert(
        ObjectTrigger::OnUse,
        "message(\"✨ The potion glows!\") && heal(50) && consume()".to_string(),
    );

    store.put_object(potion).unwrap();

    let loaded = store.get_object("test_healing_potion").unwrap();
    assert_eq!(
        loaded.actions.get(&ObjectTrigger::OnUse),
        Some(&"message(\"✨ The potion glows!\") && heal(50) && consume()".to_string()),
        "Complex trigger script should be preserved"
    );
}
