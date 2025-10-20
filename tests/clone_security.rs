//! Integration tests for object cloning security
//!
//! Tests exploit scenarios to verify security controls work correctly:
//! 1. Exponential cloning attack (depth limit enforcement)
//! 2. Currency duplication attempt (value stripping)
//! 3. Permission escalation test (ownership enforcement)
//! 4. Quota exhaustion test (rate limiting)
//! 5. Cooldown enforcement test (temporal limits)

mod common;

use meshbbs::tmush::{
    clone::{
        clone_object, CLONES_PER_HOUR, CLONE_COOLDOWN, MAX_CLONABLE_VALUE, MAX_CLONE_DEPTH,
        MAX_OBJECTS_PER_PLAYER,
    },
    errors::TinyMushError,
    storage::TinyMushStore,
    types::{CurrencyAmount, ObjectFlag, ObjectOwner, ObjectRecord, OwnershipReason, PlayerRecord},
};
use tempfile::TempDir;

fn setup_test() -> (TempDir, TinyMushStore) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp_dir.path()).unwrap();
    (temp_dir, store)
}

fn create_test_player(username: &str, store: &TinyMushStore) -> PlayerRecord {
    let mut player = PlayerRecord::new(username, username, "test_room");
    player.grant_builder(1); // Grant builder level 1 for clone testing
    store.put_player(player.clone()).unwrap();
    player
}

fn create_clonable_object(
    id: &str,
    name: &str,
    owner: &str,
    value: u32,
    store: &TinyMushStore,
) -> ObjectRecord {
    let object = ObjectRecord::new_player_owned(
        id,
        name,
        &format!("Test {}", name),
        owner,
        OwnershipReason::Created,
    );
    let mut obj = object;
    obj.flags.push(ObjectFlag::Clonable);
    obj.value = value;
    obj.currency_value = CurrencyAmount::multi_tier(value as i64);
    store.put_object(obj.clone()).unwrap();
    obj
}

#[test]
fn test_exponential_cloning_attack() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("attacker", &store);

    // Create a clonable object at depth 0
    let obj0 = create_clonable_object("obj0", "Widget", "attacker", 10, &store);
    assert_eq!(obj0.clone_depth, 0);

    // Clone 1: depth 0 -> 1 (should succeed)
    let clone1 = clone_object(&obj0.id, "attacker", &store).unwrap();
    assert_eq!(clone1.clone_depth, 1);
    assert_eq!(clone1.clone_source_id, Some(obj0.id.clone()));

    // Reset cooldown for next clone
    let mut player = store.get_player("attacker").unwrap();
    player.last_clone_time = 0;
    store.put_player(player).unwrap();

    // Clone 2: depth 1 -> 2 (should succeed)
    let clone2 = clone_object(&clone1.id, "attacker", &store).unwrap();
    assert_eq!(clone2.clone_depth, 2);

    // Reset cooldown for next clone
    let mut player = store.get_player("attacker").unwrap();
    player.last_clone_time = 0;
    store.put_player(player).unwrap();

    // Clone 3: depth 2 -> 3 (should succeed - at limit)
    let clone3 = clone_object(&clone2.id, "attacker", &store).unwrap();
    assert_eq!(clone3.clone_depth, 3);
    assert_eq!(clone3.clone_depth, MAX_CLONE_DEPTH);

    // Clone 4: depth 3 -> 4 (should FAIL - exceeds limit)
    let result = clone_object(&clone3.id, "attacker", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("maximum clone depth"));
        }
        _ => panic!("Expected PermissionDenied error for depth limit"),
    }

    println!(
        "✅ Exponential cloning attack blocked at depth {}",
        MAX_CLONE_DEPTH
    );
}

#[test]
fn test_currency_duplication_attempt() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("duper", &store);

    // Create a valuable object (50 gold, below threshold)
    let valuable = create_clonable_object("valuable", "Gold Ring", "duper", 50, &store);
    assert_eq!(valuable.value, 50);
    assert_eq!(valuable.currency_value.base_value(), 50);

    // Clone should succeed but value should be stripped
    let clone = clone_object(&valuable.id, "duper", &store).unwrap();
    assert_eq!(clone.value, 0, "Clone value should be stripped to 0");
    assert_eq!(
        clone.currency_value.base_value(),
        0,
        "Clone currency should be stripped to 0"
    );

    // Verify original still has value
    let original = store.get_object(&valuable.id).unwrap();
    assert_eq!(original.value, 50, "Original should keep its value");

    println!("✅ Currency duplication blocked - clone has 0 value");
}

#[test]
fn test_high_value_object_cloning_blocked() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("thief", &store);

    // Create object worth more than MAX_CLONABLE_VALUE
    let expensive = create_clonable_object(
        "expensive",
        "Diamond",
        "thief",
        MAX_CLONABLE_VALUE + 1,
        &store,
    );
    assert!(expensive.value > MAX_CLONABLE_VALUE);

    // Attempt to clone should fail
    let result = clone_object(&expensive.id, "thief", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("too valuable"));
            assert!(msg.contains(&MAX_CLONABLE_VALUE.to_string()));
        }
        _ => panic!("Expected PermissionDenied for high-value object"),
    }

    println!(
        "✅ High-value object cloning blocked (>{} gold)",
        MAX_CLONABLE_VALUE
    );
}

#[test]
fn test_permission_escalation_attempt() {
    let (_temp, store) = setup_test();
    let _alice = create_test_player("alice", &store);
    let _bob = create_test_player("bob", &store);

    // Alice creates a clonable object
    let alice_obj = create_clonable_object("alice_obj", "Alice's Widget", "alice", 10, &store);
    assert_eq!(
        alice_obj.owner,
        ObjectOwner::Player {
            username: "alice".to_string()
        }
    );

    // Bob tries to clone Alice's object (should FAIL - not owner)
    let result = clone_object(&alice_obj.id, "bob", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("don't own"));
        }
        _ => panic!("Expected PermissionDenied for non-owner clone attempt"),
    }

    // Alice clones her own object (should succeed)
    let clone = clone_object(&alice_obj.id, "alice", &store).unwrap();

    // Verify clone is owned by alice, not inherited
    assert_eq!(
        clone.owner,
        ObjectOwner::Player {
            username: "alice".to_string()
        }
    );
    assert_eq!(clone.created_by, "alice");

    println!("✅ Permission escalation blocked - only owner can clone");
}

#[test]
fn test_quota_exhaustion_attack() {
    let (_temp, store) = setup_test();
    create_test_player("spammer", &store);

    // Create a clonable object
    let obj = create_clonable_object("spam_obj", "Spam", "spammer", 5, &store);

    // Verify starting quota
    let mut player = store.get_player("spammer").unwrap();
    assert_eq!(player.clone_quota, CLONES_PER_HOUR);

    // Clone until quota exhausted
    for i in 0..CLONES_PER_HOUR {
        // Reset cooldown before each clone
        player = store.get_player("spammer").unwrap();
        player.last_clone_time = 0;
        store.put_player(player.clone()).unwrap();

        let result = clone_object(&obj.id, "spammer", &store);
        assert!(result.is_ok(), "Clone {} should succeed", i + 1);

        // Verify quota decrements
        player = store.get_player("spammer").unwrap();
        assert_eq!(player.clone_quota, CLONES_PER_HOUR - (i + 1));
    }

    // Verify quota is now 0
    player = store.get_player("spammer").unwrap();
    assert_eq!(player.clone_quota, 0);

    // Next clone should FAIL - quota exhausted
    let result = clone_object(&obj.id, "spammer", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("quota exhausted"));
            assert!(msg.contains("0/"));
        }
        _ => panic!("Expected PermissionDenied for quota exhaustion"),
    }

    println!(
        "✅ Quota exhaustion blocked after {} clones",
        CLONES_PER_HOUR
    );
}

#[test]
fn test_cooldown_enforcement() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("rusher", &store);

    // Create a clonable object
    let obj = create_clonable_object("cool_obj", "Cooldown Test", "rusher", 5, &store);

    // First clone should succeed
    let clone1 = clone_object(&obj.id, "rusher", &store).unwrap();
    assert!(clone1.clone_depth > 0);

    // Verify cooldown was set
    let player = store.get_player("rusher").unwrap();
    assert!(
        player.last_clone_time > 0,
        "Cooldown timestamp should be set"
    );
    let first_clone_time = player.last_clone_time;

    // Immediate second clone should FAIL - cooldown active
    let result = clone_object(&obj.id, "rusher", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("cooldown active"));
            assert!(msg.contains("seconds"));
        }
        _ => panic!("Expected PermissionDenied for cooldown violation"),
    }

    // Simulate time passage by manually updating player
    let mut player = store.get_player("rusher").unwrap();
    player.last_clone_time = first_clone_time - CLONE_COOLDOWN - 1; // Set time to past
    store.put_player(player).unwrap();

    // Clone should now succeed
    let clone2 = clone_object(&obj.id, "rusher", &store).unwrap();
    assert_eq!(clone2.clone_depth, 1);

    println!(
        "✅ Cooldown enforcement working ({}s required)",
        CLONE_COOLDOWN
    );
}

#[test]
fn test_storage_abuse_prevention() {
    let (_temp, store) = setup_test();
    let mut player = create_test_player("hoarder", &store);

    // Artificially set player near object limit
    player.total_objects_owned = MAX_OBJECTS_PER_PLAYER - 1;
    player.last_clone_time = 0; // Reset cooldown
    store.put_player(player.clone()).unwrap();

    // Create a clonable object
    let obj = create_clonable_object("last_obj", "Final Object", "hoarder", 5, &store);

    // Clone should succeed (brings us to exactly the limit)
    let clone1 = clone_object(&obj.id, "hoarder", &store).unwrap();
    assert_eq!(
        clone1.owner,
        ObjectOwner::Player {
            username: "hoarder".to_string()
        }
    );

    // Verify we're at the limit
    player = store.get_player("hoarder").unwrap();
    assert_eq!(player.total_objects_owned, MAX_OBJECTS_PER_PLAYER);

    // Reset cooldown for next clone attempt
    player.last_clone_time = 0;
    store.put_player(player).unwrap();

    // Next clone should FAIL - at limit
    let result = clone_object(&obj.id, "hoarder", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(
                msg.contains("ownership limit") || msg.contains("Object ownership limit"),
                "Expected ownership limit error, got: {}",
                msg
            );
        }
        other => panic!(
            "Expected PermissionDenied for storage limit, got: {:?}",
            other
        ),
    }

    println!(
        "✅ Storage abuse blocked at {} objects",
        MAX_OBJECTS_PER_PLAYER
    );
}

#[test]
fn test_quest_item_cloning_blocked() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("cheater", &store);

    // Create a quest item
    let mut quest_item = create_clonable_object("quest", "Magic Amulet", "cheater", 0, &store);
    quest_item.flags.push(ObjectFlag::QuestItem);
    store.put_object(quest_item.clone()).unwrap();

    // Attempt to clone should FAIL - quest items can't be cloned
    let result = clone_object(&quest_item.id, "cheater", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("quest item"));
        }
        _ => panic!("Expected PermissionDenied for quest item"),
    }

    println!("✅ Quest item cloning blocked");
}

#[test]
fn test_unique_item_cloning_blocked() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("collector", &store);

    // Create a unique item
    let mut unique_item = create_clonable_object("unique", "One of a Kind", "collector", 0, &store);
    unique_item.flags.push(ObjectFlag::Unique);
    store.put_object(unique_item.clone()).unwrap();

    // Attempt to clone should FAIL - unique items can't be cloned
    let result = clone_object(&unique_item.id, "collector", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("unique"));
        }
        _ => panic!("Expected PermissionDenied for unique item"),
    }

    println!("✅ Unique item cloning blocked");
}

#[test]
fn test_companion_cloning_blocked() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("tamer", &store);

    // Create a companion
    let mut companion = create_clonable_object("pet", "Loyal Dog", "tamer", 0, &store);
    companion.flags.push(ObjectFlag::Companion);
    store.put_object(companion.clone()).unwrap();

    // Attempt to clone should FAIL - companions can't be cloned
    let result = clone_object(&companion.id, "tamer", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("companion"));
        }
        _ => panic!("Expected PermissionDenied for companion"),
    }

    println!("✅ Companion cloning blocked");
}

#[test]
fn test_non_clonable_object_blocked() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("newbie", &store);

    // Create object WITHOUT Clonable flag
    let obj = ObjectRecord::new_player_owned(
        "noclon",
        "Normal Item",
        "A normal item",
        "newbie",
        OwnershipReason::Created,
    );
    assert!(!obj.flags.contains(&ObjectFlag::Clonable));
    store.put_object(obj.clone()).unwrap();

    // Attempt to clone should FAIL - not clonable
    let result = clone_object(&obj.id, "newbie", &store);
    assert!(result.is_err());
    match result {
        Err(TinyMushError::PermissionDenied(msg)) => {
            assert!(msg.contains("not clonable"));
        }
        _ => panic!("Expected PermissionDenied for non-clonable object"),
    }

    println!("✅ Non-clonable object cloning blocked");
}

#[test]
fn test_clone_genealogy_tracking() {
    let (_temp, store) = setup_test();
    let _player = create_test_player("scientist", &store);

    // Create original
    let obj0 = create_clonable_object("gen0", "Generation 0", "scientist", 5, &store);
    assert_eq!(obj0.clone_depth, 0);
    assert_eq!(obj0.clone_source_id, None);
    assert_eq!(obj0.clone_count, 0);

    // Clone once
    let clone1 = clone_object(&obj0.id, "scientist", &store).unwrap();
    assert_eq!(clone1.clone_depth, 1);
    assert_eq!(clone1.clone_source_id, Some(obj0.id.clone()));
    assert_eq!(clone1.created_by, "scientist");

    // Verify source tracking updated
    let updated_obj0 = store.get_object(&obj0.id).unwrap();
    assert_eq!(updated_obj0.clone_count, 1);

    // Reset cooldown before next clone
    let mut player = store.get_player("scientist").unwrap();
    player.last_clone_time = 0;
    store.put_player(player).unwrap();

    // Clone the clone
    let clone2 = clone_object(&clone1.id, "scientist", &store).unwrap();
    assert_eq!(clone2.clone_depth, 2);
    assert_eq!(clone2.clone_source_id, Some(clone1.id.clone()));

    // Verify intermediate source tracking
    let updated_clone1 = store.get_object(&clone1.id).unwrap();
    assert_eq!(updated_clone1.clone_count, 1);

    println!("✅ Clone genealogy tracking working correctly");
}

#[test]
fn test_non_builder_cannot_clone() {
    let (_temp, store) = setup_test();

    // Create a regular player without builder privileges
    let player = PlayerRecord::new("regular_joe", "regular_joe", "test_room");
    // Explicitly do NOT grant builder level (defaults to 0)
    store.put_player(player.clone()).unwrap();

    // Create a clonable object owned by this player
    let obj = create_clonable_object("obj_test", "Sword", "regular_joe", 10, &store);

    // Attempt to clone should fail with permission error
    let result = clone_object(&obj.id, "regular_joe", &store);
    assert!(result.is_err(), "Non-builder should not be able to clone");

    if let Err(TinyMushError::PermissionDenied(msg)) = result {
        assert!(
            msg.contains("builder privileges"),
            "Error should mention builder privileges"
        );
        assert!(
            msg.contains("level 1+"),
            "Error should mention level requirement"
        );
        println!("✅ Non-builder correctly denied: {}", msg);
    } else {
        panic!("Expected PermissionDenied error, got: {:?}", result);
    }
}
