// Integration tests for Phase 4 quest content
// Tests the quest system with Phase 4.2-4.4 mechanics

use meshbbs::tmush::storage::{TinyMushStore, TinyMushStoreBuilder};
use meshbbs::tmush::types::{
    ObjectFlag, ObjectOwner, ObjectiveType, PlayerRecord, QuestObjective, QuestRecord,
};
use std::collections::HashMap;
use tempfile::TempDir;

/// Helper to create test store without world seed
fn create_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let store = TinyMushStoreBuilder::new(temp_dir.path().to_path_buf())
        .without_world_seed()
        .open()
        .expect("Failed to create store");
    (store, temp_dir)
}

#[test]
fn test_cipher_quest_has_examine_sequence_objective() {
    let (store, _temp) = create_test_store();

    // Create "the_cipher" quest
    let cipher_quest = QuestRecord::new(
        "the_cipher",
        "The Cipher",
        "Examine symbols in the correct sequence.",
        "old_elm",
        4,
    )
    .with_objective(QuestObjective::new(
        "Examine symbols in correct sequence",
        ObjectiveType::ExamineSequence {
            object_ids: vec![
                "cipher_spring".to_string(),
                "cipher_summer".to_string(),
                "cipher_autumn".to_string(),
                "cipher_winter".to_string(),
            ],
        },
        1,
    ));

    // Store and retrieve quest
    store
        .put_quest(cipher_quest.clone())
        .expect("Failed to store quest");
    let retrieved = store
        .get_quest("the_cipher")
        .expect("Failed to retrieve quest");

    assert_eq!(retrieved.id, "the_cipher");
    assert_eq!(retrieved.objectives.len(), 1);

    // Verify objective type
    match &retrieved.objectives[0].objective_type {
        ObjectiveType::ExamineSequence { object_ids } => {
            assert_eq!(object_ids.len(), 4);
            assert_eq!(object_ids[0], "cipher_spring");
            assert_eq!(object_ids[3], "cipher_winter");
        }
        _ => panic!("Expected ExamineSequence objective"),
    }
}

#[test]
fn test_dark_navigation_quest_has_light_requirement() {
    let (store, _temp) = create_test_store();

    // Create "into_the_depths" quest
    let depths_quest = QuestRecord::new(
        "into_the_depths",
        "Into the Depths",
        "Navigate dark caverns with a light source.",
        "old_graybeard",
        4,
    )
    .with_objective(QuestObjective::new(
        "Obtain a light source",
        ObjectiveType::ObtainLightSource,
        1,
    ))
    .with_objective(QuestObjective::new(
        "Navigate to dark caverns",
        ObjectiveType::NavigateDarkRoom {
            room_id: "deep_caverns_entrance".to_string(),
            requires_light: true,
        },
        1,
    ));

    store
        .put_quest(depths_quest.clone())
        .expect("Failed to store quest");
    let retrieved = store
        .get_quest("into_the_depths")
        .expect("Failed to retrieve quest");

    assert_eq!(retrieved.objectives.len(), 2);

    // Verify first objective is ObtainLightSource
    match &retrieved.objectives[0].objective_type {
        ObjectiveType::ObtainLightSource => {
            // Good!
        }
        _ => panic!("Expected ObtainLightSource objective"),
    }

    // Verify second objective is NavigateDarkRoom
    match &retrieved.objectives[1].objective_type {
        ObjectiveType::NavigateDarkRoom {
            room_id,
            requires_light,
        } => {
            assert_eq!(room_id, "deep_caverns_entrance");
            assert_eq!(*requires_light, true);
        }
        _ => panic!("Expected NavigateDarkRoom objective"),
    }
}

#[test]
fn test_crafting_quest_has_craft_objectives() {
    let (store, _temp) = create_test_store();

    // Create "master_artisan" quest
    let artisan_quest = QuestRecord::new(
        "master_artisan",
        "Master Artisan",
        "Craft multiple items to prove your skills.",
        "tinker_brass",
        5,
    )
    .with_objective(QuestObjective::new(
        "Craft a basic antenna",
        ObjectiveType::CraftItem {
            item_id: "basic_antenna".to_string(),
            count: 1,
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Craft a relay module",
        ObjectiveType::CraftItem {
            item_id: "relay_module".to_string(),
            count: 1,
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Craft an advanced signal array",
        ObjectiveType::CraftItem {
            item_id: "signal_array_advanced".to_string(),
            count: 1,
        },
        1,
    ));

    store
        .put_quest(artisan_quest.clone())
        .expect("Failed to store quest");
    let retrieved = store
        .get_quest("master_artisan")
        .expect("Failed to retrieve quest");

    assert_eq!(retrieved.objectives.len(), 3);

    // Verify all objectives are CraftItem type
    for (idx, objective) in retrieved.objectives.iter().enumerate() {
        match &objective.objective_type {
            ObjectiveType::CraftItem { item_id, count } => {
                assert_eq!(*count, 1);
                match idx {
                    0 => assert_eq!(item_id, "basic_antenna"),
                    1 => assert_eq!(item_id, "relay_module"),
                    2 => assert_eq!(item_id, "signal_array_advanced"),
                    _ => panic!("Unexpected objective index"),
                }
            }
            _ => panic!("Expected CraftItem objective at index {}", idx),
        }
    }
}

#[test]
fn test_epic_quest_combines_all_mechanics() {
    let (store, _temp) = create_test_store();

    // Create "the_lost_artifact" quest combining all Phase 4 mechanics
    let artifact_quest = QuestRecord::new(
        "the_lost_artifact",
        "The Lost Artifact",
        "Epic quest using symbols, dark navigation, and crafting.",
        "old_elm",
        5,
    )
    .with_objective(QuestObjective::new(
        "Decipher entrance sequence",
        ObjectiveType::ExamineSequence {
            object_ids: vec![
                "ruins_glyph_alpha".to_string(),
                "ruins_glyph_beta".to_string(),
                "ruins_glyph_gamma".to_string(),
                "ruins_glyph_delta".to_string(),
            ],
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Navigate dark passage",
        ObjectiveType::NavigateDarkRoom {
            room_id: "ruins_dark_passage".to_string(),
            requires_light: true,
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Craft chamber key",
        ObjectiveType::CraftItem {
            item_id: "artifact_chamber_key".to_string(),
            count: 1,
        },
        1,
    ));

    store
        .put_quest(artifact_quest.clone())
        .expect("Failed to store quest");
    let retrieved = store
        .get_quest("the_lost_artifact")
        .expect("Failed to retrieve quest");

    assert_eq!(retrieved.objectives.len(), 3);

    // Verify it has all three types of objectives
    let has_examine_sequence = retrieved
        .objectives
        .iter()
        .any(|obj| matches!(obj.objective_type, ObjectiveType::ExamineSequence { .. }));
    let has_navigate_dark = retrieved
        .objectives
        .iter()
        .any(|obj| matches!(obj.objective_type, ObjectiveType::NavigateDarkRoom { .. }));
    let has_craft = retrieved
        .objectives
        .iter()
        .any(|obj| matches!(obj.objective_type, ObjectiveType::CraftItem { .. }));

    assert!(
        has_examine_sequence,
        "Should have ExamineSequence objective"
    );
    assert!(has_navigate_dark, "Should have NavigateDarkRoom objective");
    assert!(has_craft, "Should have CraftItem objective");
}

#[test]
fn test_player_can_track_examined_symbol_sequence() {
    let (store, _temp) = create_test_store();

    // Create player
    let mut player = PlayerRecord::new("testuser", "Test User", "town_square");

    // Simulate examining symbols in sequence
    player
        .examined_symbol_sequence
        .push("cipher_spring".to_string());
    player
        .examined_symbol_sequence
        .push("cipher_summer".to_string());
    player
        .examined_symbol_sequence
        .push("cipher_autumn".to_string());
    player
        .examined_symbol_sequence
        .push("cipher_winter".to_string());

    // Store and retrieve
    store.put_player(player).expect("Failed to store player");
    let retrieved = store
        .get_player("testuser")
        .expect("Failed to retrieve player");

    assert_eq!(retrieved.examined_symbol_sequence.len(), 4);
    assert_eq!(retrieved.examined_symbol_sequence[0], "cipher_spring");
    assert_eq!(retrieved.examined_symbol_sequence[3], "cipher_winter");
}

#[test]
fn test_light_source_objects_have_correct_flag() {
    let (store, _temp) = create_test_store();
    let now = chrono::Utc::now();

    // Create objects with LightSource flag
    let torch = meshbbs::tmush::types::ObjectRecord {
        id: "torch".to_string(),
        name: "Torch".to_string(),
        description: "A burning torch".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: meshbbs::tmush::types::CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![ObjectFlag::LightSource],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 4,
    };

    let lantern = meshbbs::tmush::types::ObjectRecord {
        id: "lantern".to_string(),
        name: "LED Lantern".to_string(),
        description: "A bright LED lantern".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: meshbbs::tmush::types::CurrencyAmount::default(),
        value: 50,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![ObjectFlag::LightSource],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 4,
    };

    store.put_object(torch).expect("Failed to store torch");
    store.put_object(lantern).expect("Failed to store lantern");

    let retrieved_torch = store.get_object("torch").expect("Failed to retrieve torch");
    let retrieved_lantern = store
        .get_object("lantern")
        .expect("Failed to retrieve lantern");

    assert!(
        retrieved_torch.flags.contains(&ObjectFlag::LightSource),
        "Torch should have LightSource flag"
    );
    assert!(
        retrieved_lantern.flags.contains(&ObjectFlag::LightSource),
        "Lantern should have LightSource flag"
    );
}

#[test]
fn test_player_reputation_tracking() {
    let (store, _temp) = create_test_store();

    let mut player = PlayerRecord::new("testuser", "Test User", "town_square");

    // Add reputation with various factions
    player.add_reputation("tinkers", 50);
    player.add_reputation("scholars", 75);
    player.add_reputation("traders", -25);

    // Store and retrieve
    store.put_player(player).expect("Failed to store player");
    let retrieved = store
        .get_player("testuser")
        .expect("Failed to retrieve player");

    assert_eq!(retrieved.get_reputation("tinkers"), 50);
    assert_eq!(retrieved.get_reputation("scholars"), 75);
    assert_eq!(retrieved.get_reputation("traders"), -25);
    assert_eq!(retrieved.get_reputation("unknown_faction"), 0);
}

#[test]
fn test_reputation_levels_calculated_correctly() {
    use meshbbs::tmush::types::ReputationLevel;

    assert_eq!(ReputationLevel::from_points(-100), ReputationLevel::Hated);
    assert_eq!(ReputationLevel::from_points(-75), ReputationLevel::Hated);
    assert_eq!(ReputationLevel::from_points(-50), ReputationLevel::Hostile);
    assert_eq!(
        ReputationLevel::from_points(-25),
        ReputationLevel::Unfriendly
    );
    assert_eq!(ReputationLevel::from_points(0), ReputationLevel::Neutral);
    assert_eq!(ReputationLevel::from_points(25), ReputationLevel::Friendly);
    assert_eq!(ReputationLevel::from_points(50), ReputationLevel::Honored);
    assert_eq!(ReputationLevel::from_points(75), ReputationLevel::Revered);
    assert_eq!(ReputationLevel::from_points(100), ReputationLevel::Revered);
}

#[test]
fn test_reputation_clamped_to_bounds() {
    let (store, _temp) = create_test_store();

    let mut player = PlayerRecord::new("testuser", "Test User", "town_square");

    // Try to add reputation beyond bounds
    player.add_reputation("tinkers", 150); // Should clamp to 100
    player.add_reputation("traders", -150); // Should clamp to -100

    store.put_player(player).expect("Failed to store player");
    let retrieved = store
        .get_player("testuser")
        .expect("Failed to retrieve player");

    assert_eq!(retrieved.get_reputation("tinkers"), 100);
    assert_eq!(retrieved.get_reputation("traders"), -100);
}
