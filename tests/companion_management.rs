use meshbbs::tmush::types::{CompanionBehavior, CompanionRecord, CompanionType};
use meshbbs::tmush::{TinyMushStore, TinyMushStoreBuilder};
use tempfile::TempDir;

fn setup_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStoreBuilder::new(temp_dir.path()).open().unwrap();
    (store, temp_dir)
}

#[test]
fn companion_crud_operations() {
    let (store, _temp) = setup_test_store();

    // CREATE
    let companion =
        CompanionRecord::new("test_horse", "Test Horse", CompanionType::Horse, "stable");
    store.put_companion(companion.clone()).unwrap();

    // READ
    let retrieved = store.get_companion("test_horse").unwrap();
    assert_eq!(retrieved.id, "test_horse");
    assert_eq!(retrieved.name, "Test Horse");
    assert_eq!(retrieved.companion_type, CompanionType::Horse);
    assert_eq!(retrieved.room_id, "stable");

    // UPDATE
    let mut updated = retrieved.clone();
    updated.name = "Battle Steed".to_string();
    store.put_companion(updated).unwrap();

    let retrieved = store.get_companion("test_horse").unwrap();
    assert_eq!(retrieved.name, "Battle Steed");

    // DELETE
    store.delete_companion("test_horse").unwrap();
    assert!(store.get_companion("test_horse").is_err());
}

#[test]
fn companion_edit_all_fields() {
    let (store, _temp) = setup_test_store();

    let mut companion =
        CompanionRecord::new("test_dog", "Test Dog", CompanionType::Dog, "town_square");
    store.put_companion(companion.clone()).unwrap();

    // Edit name
    companion.name = "Loyal Hound".to_string();
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_dog").unwrap();
    assert_eq!(retrieved.name, "Loyal Hound");

    // Edit description
    companion.description = "A fierce but loyal guard dog".to_string();
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_dog").unwrap();
    assert_eq!(retrieved.description, "A fierce but loyal guard dog");

    // Edit type
    companion.companion_type = CompanionType::Mercenary;
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_dog").unwrap();
    assert_eq!(retrieved.companion_type, CompanionType::Mercenary);

    // Edit room
    companion.room_id = "barracks".to_string();
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_dog").unwrap();
    assert_eq!(retrieved.room_id, "barracks");
}

#[test]
fn companion_type_variants() {
    let (store, _temp) = setup_test_store();

    let types = vec![
        (CompanionType::Horse, "test_horse"),
        (CompanionType::Dog, "test_dog"),
        (CompanionType::Cat, "test_cat"),
        (CompanionType::Familiar, "test_familiar"),
        (CompanionType::Mercenary, "test_mercenary"),
        (CompanionType::Construct, "test_construct"),
    ];

    for (companion_type, id) in types {
        let companion = CompanionRecord::new(id, "Test", companion_type.clone(), "test_room");
        store.put_companion(companion).unwrap();
        let retrieved = store.get_companion(id).unwrap();
        assert_eq!(retrieved.companion_type, companion_type);
    }
}

#[test]
fn companion_behavior_management() {
    let (store, _temp) = setup_test_store();

    let mut companion =
        CompanionRecord::new("test_dog", "Test Dog", CompanionType::Dog, "town_square");

    // Dogs start with 3 default behaviors (AutoFollow, AlertDanger, IdleChatter)
    // So behaviors.len() should be 3 initially
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_dog").unwrap();
    assert_eq!(
        retrieved.behaviors.len(),
        3,
        "Dog should have 3 default behaviors"
    );

    // Add ExtraStorage behavior
    companion
        .behaviors
        .push(CompanionBehavior::ExtraStorage { capacity: 30 });
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_dog").unwrap();
    assert_eq!(retrieved.behaviors.len(), 4, "Should have 4 behaviors now");
    match &retrieved.behaviors[3] {
        CompanionBehavior::ExtraStorage { capacity } => assert_eq!(*capacity, 30),
        _ => panic!("Expected ExtraStorage behavior at index 3"),
    }
}

#[test]
fn companion_complex_behaviors() {
    let (store, _temp) = setup_test_store();

    // Mercenary starts with 2 default behaviors (AutoFollow, CombatAssist with damage_bonus=15)
    let mut companion = CompanionRecord::new(
        "test_mercenary",
        "Guard",
        CompanionType::Mercenary,
        "barracks",
    );

    // Verify default behaviors exist
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_mercenary").unwrap();
    assert_eq!(
        retrieved.behaviors.len(),
        2,
        "Mercenary should have 2 default behaviors"
    );

    // Add another CombatAssist (testing we can add multiples)
    companion
        .behaviors
        .push(CompanionBehavior::CombatAssist { damage_bonus: 10 });
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_mercenary").unwrap();
    assert_eq!(retrieved.behaviors.len(), 3, "Should have 3 behaviors now");
    match &retrieved.behaviors[2] {
        CompanionBehavior::CombatAssist { damage_bonus } => assert_eq!(*damage_bonus, 10),
        _ => panic!("Expected CombatAssist behavior at index 2"),
    }

    // Healing
    companion.behaviors.push(CompanionBehavior::Healing {
        heal_amount: 15,
        cooldown_seconds: 300,
    });
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_mercenary").unwrap();
    assert_eq!(retrieved.behaviors.len(), 4, "Should have 4 behaviors now");
    match &retrieved.behaviors[3] {
        CompanionBehavior::Healing {
            heal_amount,
            cooldown_seconds,
        } => {
            assert_eq!(*heal_amount, 15);
            assert_eq!(*cooldown_seconds, 300);
        }
        _ => panic!("Expected Healing behavior at index 3"),
    }

    // SkillBoost
    companion.behaviors.push(CompanionBehavior::SkillBoost {
        skill: "combat".to_string(),
        bonus: 5,
    });
    store.put_companion(companion.clone()).unwrap();
    let retrieved = store.get_companion("test_mercenary").unwrap();
    assert_eq!(retrieved.behaviors.len(), 5, "Should have 5 behaviors now");
    match &retrieved.behaviors[4] {
        CompanionBehavior::SkillBoost { skill, bonus } => {
            assert_eq!(skill, "combat");
            assert_eq!(*bonus, 5);
        }
        _ => panic!("Expected SkillBoost behavior at index 4"),
    }

    // IdleChatter - test with a Cat which already has IdleChatter default
    let mut cat = CompanionRecord::new("test_cat", "Chatty Cat", CompanionType::Cat, "library");
    cat.behaviors.push(CompanionBehavior::IdleChatter {
        messages: vec!["*purrs*".to_string(), "*meows*".to_string()],
    });
    store.put_companion(cat).unwrap();
    let retrieved = store.get_companion("test_cat").unwrap();
    // Cat has 1 default IdleChatter behavior, we added another, so 2 total
    assert_eq!(
        retrieved.behaviors.len(),
        2,
        "Cat should have 2 IdleChatter behaviors"
    );
    match &retrieved.behaviors[1] {
        CompanionBehavior::IdleChatter { messages } => {
            assert_eq!(messages.len(), 2);
            assert_eq!(messages[0], "*purrs*");
            assert_eq!(messages[1], "*meows*");
        }
        _ => panic!("Expected IdleChatter behavior at index 1"),
    }
}

#[test]
fn companion_exists_check() {
    let (store, _temp) = setup_test_store();

    // Should not exist initially
    assert!(!store.companion_exists("nonexistent").unwrap());

    // Create companion
    let companion =
        CompanionRecord::new("test_horse", "Test Horse", CompanionType::Horse, "stable");
    store.put_companion(companion).unwrap();

    // Should exist now
    assert!(store.companion_exists("test_horse").unwrap());

    // Delete companion
    store.delete_companion("test_horse").unwrap();

    // Should not exist anymore
    assert!(!store.companion_exists("test_horse").unwrap());
}

#[test]
fn default_companions_seeded() {
    let (store, _temp) = setup_test_store();

    // Seed starter companions (gentle_mare, loyal_hound, shadow_cat)
    use meshbbs::tmush::state::seed_starter_companions;
    for companion in seed_starter_companions() {
        store.put_companion(companion).unwrap();
    }

    // Verify all 3 seeded companions exist
    let companion_ids = store.list_companion_ids().unwrap();
    assert_eq!(companion_ids.len(), 3);
    assert!(companion_ids.contains(&"gentle_mare".to_string()));
    assert!(companion_ids.contains(&"loyal_hound".to_string()));
    assert!(companion_ids.contains(&"shadow_cat".to_string()));

    // Verify gentle_mare details
    let mare = store.get_companion("gentle_mare").unwrap();
    assert_eq!(mare.name, "Gentle Mare");
    assert_eq!(mare.companion_type, CompanionType::Horse);
    assert_eq!(mare.room_id, "south_market");

    // Verify loyal_hound details
    let hound = store.get_companion("loyal_hound").unwrap();
    assert_eq!(hound.name, "Loyal Hound");
    assert_eq!(hound.companion_type, CompanionType::Dog);
    assert_eq!(hound.room_id, "town_square");

    // Verify shadow_cat details
    let cat = store.get_companion("shadow_cat").unwrap();
    assert_eq!(cat.name, "Shadow Cat");
    assert_eq!(cat.companion_type, CompanionType::Cat);
    assert_eq!(cat.room_id, "mesh_museum");
}
