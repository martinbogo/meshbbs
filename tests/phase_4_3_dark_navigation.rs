//! Integration test for Phase 4.3: Dark Navigation System
//!
//! Tests that ObjectFlag::LightSource exists and can be used on objects.

use chrono::Utc;
use meshbbs::tmush::types::{CurrencyAmount, ObjectFlag, ObjectOwner, ObjectRecord};
use meshbbs::tmush::TinyMushStoreBuilder;
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn light_source_flag_variant_exists() {
    // Create a LightSource flag
    let flag = ObjectFlag::LightSource;

    // Verify it exists and has correct debug representation
    let debug_str = format!("{:?}", flag);
    assert!(
        debug_str.contains("LightSource"),
        "ObjectFlag::LightSource should exist and be debuggable"
    );
}

#[test]
fn object_can_have_light_source_flag() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create an object with LightSource flag
    let torch = ObjectRecord {
        id: "test_torch".to_string(),
        name: "Test Torch".to_string(),
        description: "A bright torch for testing".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::decimal(10),
        value: 10,
        takeable: true,
        usable: false,
        flags: vec![ObjectFlag::LightSource],
        actions: HashMap::new(),
        locked: false,
        ownership_history: vec![],
        clone_count: 0,
        clone_depth: 0,
        clone_source_id: None,
        created_by: "system".to_string(),
        schema_version: 1,
    };

    // Save and reload
    store.put_object(torch.clone()).expect("save torch");
    let loaded = store.get_object("test_torch").expect("get torch");

    // Verify flag persists
    assert!(
        loaded.flags.contains(&ObjectFlag::LightSource),
        "LightSource flag should persist on objects"
    );
}

#[test]
fn multiple_flags_including_light_source() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create an object with multiple flags including LightSource
    let lantern = ObjectRecord {
        id: "test_lantern".to_string(),
        name: "Magic Lantern".to_string(),
        description: "A magical glowing lantern".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 2,
        currency_value: CurrencyAmount::decimal(50),
        value: 50,
        takeable: true,
        usable: false,
        flags: vec![ObjectFlag::LightSource, ObjectFlag::Container],
        actions: HashMap::new(),
        locked: false,
        ownership_history: vec![],
        clone_count: 0,
        clone_depth: 0,
        clone_source_id: None,
        created_by: "system".to_string(),
        schema_version: 1,
    };

    // Save and reload
    store.put_object(lantern.clone()).expect("save lantern");
    let loaded = store.get_object("test_lantern").expect("get lantern");

    // Verify both flags persist
    assert!(loaded.flags.contains(&ObjectFlag::LightSource));
    assert!(loaded.flags.contains(&ObjectFlag::Container));
    assert_eq!(loaded.flags.len(), 2);
}

#[test]
fn object_without_light_source_flag() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a regular object without LightSource flag
    let stick = ObjectRecord {
        id: "test_stick".to_string(),
        name: "Wooden Stick".to_string(),
        description: "A plain wooden stick".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::decimal(1),
        value: 1,
        takeable: true,
        usable: false,
        flags: vec![],
        actions: HashMap::new(),
        locked: false,
        ownership_history: vec![],
        clone_count: 0,
        clone_depth: 0,
        clone_source_id: None,
        created_by: "system".to_string(),
        schema_version: 1,
    };

    // Save and reload
    store.put_object(stick.clone()).expect("save stick");
    let loaded = store.get_object("test_stick").expect("get stick");

    // Verify it doesn't have LightSource flag
    assert!(
        !loaded.flags.contains(&ObjectFlag::LightSource),
        "Regular objects should not have LightSource flag"
    );
}

#[test]
fn light_source_flag_can_be_added() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create object without flag
    let mut candle = ObjectRecord {
        id: "test_candle".to_string(),
        name: "Unlit Candle".to_string(),
        description: "An unlit candle".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::decimal(5),
        value: 5,
        takeable: true,
        usable: false,
        flags: vec![],
        actions: HashMap::new(),
        locked: false,
        ownership_history: vec![],
        clone_count: 0,
        clone_depth: 0,
        clone_source_id: None,
        created_by: "system".to_string(),
        schema_version: 1,
    };

    store.put_object(candle.clone()).expect("save unlit");

    // Add LightSource flag (simulating lighting the candle)
    candle.flags.push(ObjectFlag::LightSource);
    candle.name = "Lit Candle".to_string();
    store.put_object(candle.clone()).expect("save lit");

    // Reload and verify flag was added
    let loaded = store.get_object("test_candle").expect("get candle");
    assert!(
        loaded.flags.contains(&ObjectFlag::LightSource),
        "LightSource flag should be addable to existing objects"
    );
    assert_eq!(loaded.name, "Lit Candle");
}
