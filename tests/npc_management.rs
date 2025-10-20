/// Integration tests for @NPC admin command system (Phase 2: Data-Driven Migration)
/// Tests CRUD operations for data-driven NPC management using the storage layer directly.
use meshbbs::tmush::types::{NpcFlag, NpcRecord};
use meshbbs::tmush::{TinyMushStore, TinyMushStoreBuilder};
use tempfile::TempDir;

fn setup_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStoreBuilder::new(temp_dir.path()).open().unwrap();
    (store, temp_dir)
}

#[test]
fn npc_crud_operations() {
    let (store, _temp) = setup_test_store();

    // CREATE a new NPC
    let npc = NpcRecord::new(
        "test_npc",
        "Test NPC",
        "Test Title",
        "A test NPC for testing",
        "test_room",
    );
    store.put_npc(npc.clone()).unwrap();

    // Verify NPC exists
    assert!(
        store.npc_exists("test_npc").unwrap(),
        "NPC should exist in database"
    );
    let retrieved = store.get_npc("test_npc").unwrap();
    assert_eq!(retrieved.name, "Test NPC");
    assert_eq!(retrieved.title, "Test Title");
    assert_eq!(retrieved.room_id, "test_room");

    // UPDATE NPC fields
    let mut updated = retrieved.clone();
    updated.name = "Updated NPC".to_string();
    updated.title = "Updated Title".to_string();
    updated.description = "Updated description".to_string();
    updated.room_id = "new_room".to_string();
    store.put_npc(updated).unwrap();

    // Verify updates
    let after_update = store.get_npc("test_npc").unwrap();
    assert_eq!(after_update.name, "Updated NPC");
    assert_eq!(after_update.title, "Updated Title");
    assert_eq!(after_update.description, "Updated description");
    assert_eq!(after_update.room_id, "new_room");

    // LIST all NPCs
    let ids = store.list_npc_ids().unwrap();
    assert!(
        ids.contains(&"test_npc".to_string()),
        "LIST should show our NPC"
    );

    // DELETE NPC
    store.delete_npc("test_npc").unwrap();

    // Verify NPC is gone
    assert!(
        !store.npc_exists("test_npc").unwrap(),
        "NPC should be deleted from database"
    );
}

#[test]
fn npc_edit_all_fields() {
    let (store, _temp) = setup_test_store();

    let mut npc = NpcRecord::new(
        "full_test",
        "Full Test",
        "Test Title",
        "Initial description",
        "start_room",
    );
    store.put_npc(npc.clone()).unwrap();

    // Edit name
    npc.name = "New Name".to_string();
    store.put_npc(npc.clone()).unwrap();
    let retrieved = store.get_npc("full_test").unwrap();
    assert_eq!(retrieved.name, "New Name");

    // Edit title
    npc.title = "New Title".to_string();
    store.put_npc(npc.clone()).unwrap();
    let retrieved = store.get_npc("full_test").unwrap();
    assert_eq!(retrieved.title, "New Title");

    // Edit description
    npc.description = "New description".to_string();
    store.put_npc(npc.clone()).unwrap();
    let retrieved = store.get_npc("full_test").unwrap();
    assert_eq!(retrieved.description, "New description");

    // Edit room
    npc.room_id = "new_room".to_string();
    store.put_npc(npc.clone()).unwrap();
    let retrieved = store.get_npc("full_test").unwrap();
    assert_eq!(retrieved.room_id, "new_room");
}

#[test]
fn npc_dialogue_management() {
    let (store, _temp) = setup_test_store();

    let mut npc = NpcRecord::new(
        "dialogue_test",
        "Dialogue Test",
        "Tester",
        "NPC for dialogue testing",
        "test_room",
    );

    // Add dialogue responses
    npc.dialog
        .insert("greeting".to_string(), "Hello, traveler!".to_string());
    npc.dialog
        .insert("farewell".to_string(), "Safe travels!".to_string());
    npc.dialog
        .insert("quest".to_string(), "I have a task for you.".to_string());

    store.put_npc(npc.clone()).unwrap();

    // Verify dialogue was saved
    let retrieved = store.get_npc("dialogue_test").unwrap();
    assert_eq!(retrieved.dialog.len(), 3);
    assert_eq!(
        retrieved.dialog.get("greeting").unwrap(),
        "Hello, traveler!"
    );
    assert_eq!(retrieved.dialog.get("farewell").unwrap(), "Safe travels!");
    assert_eq!(
        retrieved.dialog.get("quest").unwrap(),
        "I have a task for you."
    );

    // Add more dialogue
    npc.dialog.insert(
        "trade".to_string(),
        "What do you want to trade?".to_string(),
    );
    store.put_npc(npc.clone()).unwrap();

    let retrieved = store.get_npc("dialogue_test").unwrap();
    assert_eq!(retrieved.dialog.len(), 4);
    assert_eq!(
        retrieved.dialog.get("trade").unwrap(),
        "What do you want to trade?"
    );
}

#[test]
fn npc_flag_management() {
    let (store, _temp) = setup_test_store();

    // Test all 5 NPC flags
    let flags = vec![
        NpcFlag::Vendor,
        NpcFlag::Guard,
        NpcFlag::TutorialNpc,
        NpcFlag::QuestGiver,
        NpcFlag::Immortal,
    ];

    for (i, flag) in flags.iter().enumerate() {
        let npc = NpcRecord::new(
            &format!("flag_{}", i),
            &format!("Flag {} Test", i),
            "Flag Tester",
            "Testing flag",
            "test_room",
        )
        .with_flag(flag.clone());

        store.put_npc(npc).unwrap();

        let retrieved = store.get_npc(&format!("flag_{}", i)).unwrap();
        assert!(retrieved.flags.contains(flag));
        assert_eq!(retrieved.flags.len(), 1);
    }
}

#[test]
fn npc_multiple_flags() {
    let (store, _temp) = setup_test_store();

    let npc = NpcRecord::new(
        "multi_flag",
        "Multi Flag Test",
        "Multi Tester",
        "Testing multiple flags",
        "test_room",
    )
    .with_flag(NpcFlag::Vendor)
    .with_flag(NpcFlag::QuestGiver);

    store.put_npc(npc).unwrap();

    let retrieved = store.get_npc("multi_flag").unwrap();
    assert_eq!(retrieved.flags.len(), 2);
    assert!(retrieved.flags.contains(&NpcFlag::Vendor));
    assert!(retrieved.flags.contains(&NpcFlag::QuestGiver));
}

#[test]
fn npc_exists_check() {
    let (store, _temp) = setup_test_store();

    // Should not exist initially
    assert!(!store.npc_exists("nonexistent").unwrap());

    // Create NPC
    let npc = NpcRecord::new(
        "exists_test",
        "Exists Test",
        "Tester",
        "Test existence check",
        "test_room",
    );
    store.put_npc(npc).unwrap();

    // Should exist now
    assert!(store.npc_exists("exists_test").unwrap());

    // Delete NPC
    store.delete_npc("exists_test").unwrap();

    // Should not exist anymore
    assert!(!store.npc_exists("exists_test").unwrap());
}

#[test]
fn default_npcs_seeded() {
    use meshbbs::tmush::state::seed_starter_npcs;

    let (store, _temp) = setup_test_store();

    // Seed default NPCs
    for npc in seed_starter_npcs() {
        store.put_npc(npc).unwrap();
    }

    // Verify some known starter NPCs exist
    let ids = store.list_npc_ids().unwrap();

    // Check that we have the expected starter NPCs
    assert!(ids.len() >= 5, "Should have at least 5 starter NPCs");

    // Verify mayor_thompson exists (from seed_starter_npcs)
    assert!(
        ids.contains(&"mayor_thompson".to_string()),
        "mayor_thompson NPC should be seeded"
    );

    // Verify city_clerk exists
    assert!(
        ids.contains(&"city_clerk".to_string()),
        "city_clerk NPC should be seeded"
    );

    // Verify a seeded NPC has dialogue
    let mayor = store.get_npc("mayor_thompson").unwrap();
    assert!(
        mayor.dialog.contains_key("greeting"),
        "Mayor should have greeting dialogue"
    );
    assert!(
        !mayor.dialog.is_empty(),
        "Mayor should have dialogue responses"
    );
}
