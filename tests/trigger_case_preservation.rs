//! Test that trigger scripts preserve case in string literals
//! This was a bug where all input was uppercased, breaking trigger DSL

use chrono::Utc;
use meshbbs::tmush::types::CurrencyAmount;
use meshbbs::tmush::{ObjectOwner, ObjectRecord, ObjectTrigger, TinyMushStore};
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn test_trigger_case_preservation_in_storage() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();

    // Create an object with a trigger that has mixed case
    let mut obj = ObjectRecord {
        id: "test_stone".to_string(),
        name: "Test Stone".to_string(),
        description: "A magical stone".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 5,
        currency_value: CurrencyAmount::default(),
        value: 100,
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

    // Add trigger with mixed case string literals
    let trigger_script =
        r#"message("✨ The Stone Flashes Brilliantly!") && teleport("Town_Square")"#;
    obj.actions
        .insert(ObjectTrigger::OnUse, trigger_script.to_string());

    // Store the object
    store.put_object(obj.clone()).unwrap();

    // Retrieve and verify case was preserved
    let loaded = store.get_object("test_stone").unwrap();
    let saved_trigger = loaded.actions.get(&ObjectTrigger::OnUse).unwrap();

    // String literals should preserve case
    assert!(
        saved_trigger.contains("✨ The Stone Flashes Brilliantly!"),
        "Message case not preserved: {}",
        saved_trigger
    );
    assert!(
        saved_trigger.contains("Town_Square"),
        "Room ID case not preserved: {}",
        saved_trigger
    );

    // Should NOT be all uppercase
    assert!(
        !saved_trigger.contains("THE STONE FLASHES BRILLIANTLY"),
        "Trigger was incorrectly uppercased: {}",
        saved_trigger
    );
    assert!(
        !saved_trigger.contains("TOWN_SQUARE") || saved_trigger.contains("Town_Square"),
        "Room ID was incorrectly uppercased: {}",
        saved_trigger
    );
}
