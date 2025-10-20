/// Integration tests for @ROOM admin command system (Phase 4: Data-Driven Migration)
/// Tests CRUD operations for data-driven room management using the storage layer directly.
use meshbbs::tmush::types::{Direction, RoomFlag, RoomRecord};
use meshbbs::tmush::{TinyMushStore, TinyMushStoreBuilder};
use tempfile::TempDir;

fn setup_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStoreBuilder::new(temp_dir.path()).open().unwrap();
    (store, temp_dir)
}

#[test]
fn room_crud_operations() {
    let (store, _temp) = setup_test_store();

    // CREATE a new room
    let room = RoomRecord::world(
        "test_room",
        "Test Room",
        "A test room for testing",
        "This is a test room with a long description for testing purposes.",
    );
    store.put_room(room.clone()).unwrap();

    // Verify room exists
    assert!(
        store.room_exists("test_room").unwrap(),
        "Room should exist in database"
    );
    let retrieved = store.get_room("test_room").unwrap();
    assert_eq!(retrieved.name, "Test Room");
    assert_eq!(retrieved.short_desc, "A test room for testing");
    assert_eq!(retrieved.id, "test_room");

    // UPDATE room fields
    let mut updated = retrieved.clone();
    updated.name = "Updated Test Room".to_string();
    store.put_room(updated).unwrap();

    let retrieved = store.get_room("test_room").unwrap();
    assert_eq!(retrieved.name, "Updated Test Room");

    // DELETE room
    store.delete_room("test_room").unwrap();
    assert!(
        !store.room_exists("test_room").unwrap(),
        "Room should no longer exist"
    );
}

#[test]
fn room_edit_all_fields() {
    let (store, _temp) = setup_test_store();

    let mut room = RoomRecord::world(
        "edit_test",
        "Edit Test",
        "Original short",
        "Original long description",
    );
    store.put_room(room.clone()).unwrap();

    // Edit name
    room.name = "New Name".to_string();
    store.put_room(room.clone()).unwrap();
    let retrieved = store.get_room("edit_test").unwrap();
    assert_eq!(retrieved.name, "New Name");

    // Edit short description
    room.short_desc = "New short description".to_string();
    store.put_room(room.clone()).unwrap();
    let retrieved = store.get_room("edit_test").unwrap();
    assert_eq!(retrieved.short_desc, "New short description");

    // Edit long description
    room.long_desc = "This is a brand new long description for the room.".to_string();
    store.put_room(room.clone()).unwrap();
    let retrieved = store.get_room("edit_test").unwrap();
    assert_eq!(
        retrieved.long_desc,
        "This is a brand new long description for the room."
    );

    // Edit capacity
    room.max_capacity = 50;
    store.put_room(room.clone()).unwrap();
    let retrieved = store.get_room("edit_test").unwrap();
    assert_eq!(retrieved.max_capacity, 50);
}

#[test]
fn room_exit_management() {
    let (store, _temp) = setup_test_store();

    // Create two rooms
    let room1 = RoomRecord::world("room1", "Room One", "First room", "First room description");
    let room2 = RoomRecord::world(
        "room2",
        "Room Two",
        "Second room",
        "Second room description",
    );
    store.put_room(room1.clone()).unwrap();
    store.put_room(room2.clone()).unwrap();

    // Add exit from room1 to room2
    let mut room1 = store.get_room("room1").unwrap();
    room1.exits.insert(Direction::North, "room2".to_string());
    store.put_room(room1).unwrap();

    // Verify exit
    let retrieved = store.get_room("room1").unwrap();
    assert_eq!(retrieved.exits.len(), 1);
    assert_eq!(
        retrieved.exits.get(&Direction::North),
        Some(&"room2".to_string())
    );

    // Add multiple exits
    let mut room1 = store.get_room("room1").unwrap();
    room1.exits.insert(Direction::South, "room2".to_string());
    room1.exits.insert(Direction::East, "room2".to_string());
    room1.exits.insert(Direction::West, "room2".to_string());
    store.put_room(room1).unwrap();

    let retrieved = store.get_room("room1").unwrap();
    assert_eq!(retrieved.exits.len(), 4);
}

#[test]
fn room_flag_management() {
    let (store, _temp) = setup_test_store();

    let mut room = RoomRecord::world("flag_test", "Flag Test", "Test room", "Testing flags");
    store.put_room(room.clone()).unwrap();

    // Test all 13 room flags
    let flags = vec![
        RoomFlag::Safe,
        RoomFlag::Dark,
        RoomFlag::Indoor,
        RoomFlag::Shop,
        RoomFlag::QuestLocation,
        RoomFlag::PvpEnabled,
        RoomFlag::PlayerCreated,
        RoomFlag::Private,
        RoomFlag::Moderated,
        RoomFlag::Instanced,
        RoomFlag::Crowded,
        RoomFlag::HousingOffice,
        RoomFlag::NoTeleportOut,
    ];

    for flag in flags {
        room.flags.push(flag.clone());
        store.put_room(room.clone()).unwrap();
        let retrieved = store.get_room("flag_test").unwrap();
        assert!(
            retrieved.flags.contains(&flag),
            "Room should have flag {:?}",
            flag
        );
    }

    // Verify all flags present
    let retrieved = store.get_room("flag_test").unwrap();
    assert_eq!(retrieved.flags.len(), 13, "Room should have all 13 flags");
}

#[test]
fn room_multiple_flags() {
    let (store, _temp) = setup_test_store();

    let mut room = RoomRecord::world("multi_flag", "Multi Flag", "Test", "Testing multiple flags");

    // Add several common flags
    room.flags.push(RoomFlag::Safe);
    room.flags.push(RoomFlag::Indoor);
    room.flags.push(RoomFlag::QuestLocation);
    store.put_room(room.clone()).unwrap();

    let retrieved = store.get_room("multi_flag").unwrap();
    assert_eq!(retrieved.flags.len(), 3);
    assert!(retrieved.flags.contains(&RoomFlag::Safe));
    assert!(retrieved.flags.contains(&RoomFlag::Indoor));
    assert!(retrieved.flags.contains(&RoomFlag::QuestLocation));
}

#[test]
fn room_flag_deduplication_on_save() {
    let (store, _temp) = setup_test_store();

    let mut room = RoomRecord::world("flag_dupes", "Flag Dupes", "Testing", "Dedup flag vector");
    room.flags = vec![
        RoomFlag::Safe,
        RoomFlag::Indoor,
        RoomFlag::Safe,
        RoomFlag::QuestLocation,
        RoomFlag::Indoor,
    ];
    store.put_room(room).unwrap();

    let stored = store.get_room("flag_dupes").unwrap();
    assert_eq!(
        stored.flags,
        vec![RoomFlag::Safe, RoomFlag::Indoor, RoomFlag::QuestLocation]
    );
}

#[test]
fn room_exists_check() {
    let (store, _temp) = setup_test_store();

    // Should not exist initially
    assert!(!store.room_exists("nonexistent").unwrap());

    // Create room
    let room = RoomRecord::world("exists_test", "Exists Test", "Short", "Long");
    store.put_room(room).unwrap();

    // Should exist now
    assert!(store.room_exists("exists_test").unwrap());

    // Delete room
    store.delete_room("exists_test").unwrap();

    // Should not exist anymore
    assert!(!store.room_exists("exists_test").unwrap());
}

#[test]
fn default_rooms_seeded() {
    let (store, _temp) = setup_test_store();

    // Seed starter rooms from canonical_world_seed
    use chrono::Utc;
    use meshbbs::tmush::state::canonical_world_seed;
    for room in canonical_world_seed(Utc::now()) {
        store.put_room(room).unwrap();
    }

    // Verify some key seeded rooms exist
    let room_ids = store.list_room_ids().unwrap();
    assert!(room_ids.len() >= 20, "Should have at least 20 seeded rooms");

    // Verify specific important rooms
    assert!(room_ids.contains(&"gazebo_landing".to_string()));
    assert!(room_ids.contains(&"town_square".to_string()));
    assert!(room_ids.contains(&"city_hall_lobby".to_string()));
    assert!(room_ids.contains(&"mesh_museum".to_string()));

    // Verify town_square details
    let town_square = store.get_room("town_square").unwrap();
    assert_eq!(town_square.name, "Old Towne Square");
    assert!(town_square.flags.contains(&RoomFlag::Safe));
    assert!(town_square.flags.contains(&RoomFlag::Indoor));
    assert!(
        town_square.exits.len() >= 4,
        "Town square should have multiple exits"
    );

    // Verify gazebo_landing details
    let landing = store.get_room("gazebo_landing").unwrap();
    assert_eq!(landing.name, "Landing Gazebo");
    assert!(landing.flags.contains(&RoomFlag::Safe));
    assert!(landing.exits.contains_key(&Direction::North));
}
