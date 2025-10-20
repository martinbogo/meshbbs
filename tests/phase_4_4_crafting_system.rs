//! Integration test for Phase 4.4: Crafting System
//!
//! Tests that crafted items can be created, stored, and have proper ownership.
//! Also tests InventoryConfig structure with max_stacks field.

use chrono::Utc;
use meshbbs::tmush::types::{
    CurrencyAmount, InventoryConfig, ObjectOwner, ObjectRecord, PlayerRecord,
};
use meshbbs::tmush::TinyMushStoreBuilder;
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn inventory_config_has_max_stacks_field() {
    // Verify InventoryConfig structure has correct fields
    let config = InventoryConfig {
        allow_stacking: true,
        max_weight: 1000,
        max_stacks: 100,
    };

    assert_eq!(config.max_stacks, 100);
    assert_eq!(config.max_weight, 1000);
    assert!(config.allow_stacking);
}

#[test]
fn crafted_item_with_player_owner() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a crafted item owned by a player
    let crafted_item = ObjectRecord {
        id: "crafted_signal_booster_12345".to_string(),
        name: "Signal Booster".to_string(),
        description: "A hand-crafted signal booster made from scrap components".to_string(),
        owner: ObjectOwner::Player {
            username: "engineer_bob".to_string(),
        },
        created_at: Utc::now(),
        weight: 3,
        currency_value: CurrencyAmount::decimal(50),
        value: 50,
        takeable: true,
        usable: false,
        flags: vec![],
        actions: HashMap::new(),
        locked: false,
        ownership_history: vec![],
        clone_count: 0,
        clone_depth: 0,
        clone_source_id: None,
        created_by: "engineer_bob".to_string(),
        schema_version: 1,
    };

    // Save the crafted item
    store
        .put_object(crafted_item.clone())
        .expect("save crafted item");

    // Reload and verify
    let loaded = store
        .get_object("crafted_signal_booster_12345")
        .expect("get crafted item");

    assert_eq!(loaded.name, "Signal Booster");
    assert_eq!(loaded.weight, 3);
    assert_eq!(loaded.value, 50);
    assert!(loaded.takeable);

    // Verify player ownership
    match loaded.owner {
        ObjectOwner::Player { username } => {
            assert_eq!(username, "engineer_bob");
        }
        _ => panic!("Expected Player owner"),
    }
}

#[test]
fn crafted_basic_antenna() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a crafted basic antenna
    let antenna = ObjectRecord {
        id: "crafted_basic_antenna_67890".to_string(),
        name: "Basic Antenna".to_string(),
        description: "A simple but functional antenna crafted from a rod and wire".to_string(),
        owner: ObjectOwner::Player {
            username: "radio_enthusiast".to_string(),
        },
        created_at: Utc::now(),
        weight: 2,
        currency_value: CurrencyAmount::decimal(25),
        value: 25,
        takeable: true,
        usable: false,
        flags: vec![],
        actions: HashMap::new(),
        locked: false,
        ownership_history: vec![],
        clone_count: 0,
        clone_depth: 0,
        clone_source_id: None,
        created_by: "radio_enthusiast".to_string(),
        schema_version: 1,
    };

    // Save and reload
    store.put_object(antenna.clone()).expect("save antenna");
    let loaded = store
        .get_object("crafted_basic_antenna_67890")
        .expect("get antenna");

    assert_eq!(loaded.name, "Basic Antenna");
    assert_eq!(loaded.weight, 2);
    assert_eq!(loaded.value, 25);
}

#[test]
fn multiple_crafted_items_with_unique_ids() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create multiple crafted items with unique IDs
    for i in 1..=5 {
        let item = ObjectRecord {
            id: format!("crafted_signal_booster_{}", i),
            name: "Signal Booster".to_string(),
            description: format!("Signal booster #{}", i),
            owner: ObjectOwner::Player {
                username: "crafter".to_string(),
            },
            created_at: Utc::now(),
            weight: 3,
            currency_value: CurrencyAmount::decimal(50),
            value: 50,
            takeable: true,
            usable: false,
            flags: vec![],
            actions: HashMap::new(),
            locked: false,
            ownership_history: vec![],
            clone_count: 0,
            clone_depth: 0,
            clone_source_id: None,
            created_by: "crafter".to_string(),
            schema_version: 1,
        };

        store.put_object(item).expect(&format!("save item {}", i));
    }

    // Verify all items exist with unique IDs
    for i in 1..=5 {
        let id = format!("crafted_signal_booster_{}", i);
        let loaded = store.get_object(&id).expect(&format!("get item {}", i));
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.description, format!("Signal booster #{}", i));
    }
}

#[test]
fn player_can_own_crafted_items() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a player
    let player = PlayerRecord::new("master_crafter", "Master Crafter", "workshop");
    store.put_player(player.clone()).expect("save player");

    // Create multiple crafted items owned by this player
    let item_names = vec!["Signal Booster", "Basic Antenna", "Advanced Circuit"];

    for (i, name) in item_names.iter().enumerate() {
        let item = ObjectRecord {
            id: format!("crafted_{}_{}", name.to_lowercase().replace(" ", "_"), i),
            name: name.to_string(),
            description: format!("A crafted {}", name),
            owner: ObjectOwner::Player {
                username: "master_crafter".to_string(),
            },
            created_at: Utc::now(),
            weight: 1 + i as u8,
            currency_value: CurrencyAmount::decimal(10 * (i as i64 + 1)),
            value: 10 * (i as u32 + 1),
            takeable: true,
            usable: false,
            flags: vec![],
            actions: HashMap::new(),
            locked: false,
            ownership_history: vec![],
            clone_count: 0,
            clone_depth: 0,
            clone_source_id: None,
            created_by: "master_crafter".to_string(),
            schema_version: 1,
        };

        store.put_object(item).expect(&format!("save {}", name));
    }

    // Verify player exists and items are stored
    let loaded_player = store.get_player("master_crafter").expect("get player");
    assert_eq!(loaded_player.username, "master_crafter");

    // Verify all crafted items exist
    for (i, name) in item_names.iter().enumerate() {
        let id = format!("crafted_{}_{}", name.to_lowercase().replace(" ", "_"), i);
        let item = store.get_object(&id).expect(&format!("get {}", name));
        assert_eq!(item.name, *name);

        match item.owner {
            ObjectOwner::Player { username } => {
                assert_eq!(username, "master_crafter");
            }
            _ => panic!("Expected Player owner for {}", name),
        }
    }
}
