/// Integration tests for @QUEST admin command system
///
/// Tests the data-driven quest management interface that allows admins
/// to create, edit, and delete quests without editing code.
use meshbbs::tmush::types::{ObjectiveType, QuestObjective, QuestRecord};
use meshbbs::tmush::{CurrencyAmount, TinyMushStore, TinyMushStoreBuilder};
use tempfile::TempDir;

fn setup_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStoreBuilder::new(temp_dir.path()).open().unwrap();
    (store, temp_dir)
}

#[test]
fn quest_crud_operations() {
    let (store, _temp) = setup_test_store();

    // CREATE a new quest
    let quest = QuestRecord::new("test_quest", "TestQuest", "", "mayor_npc", 1);
    store.put_quest(quest.clone()).unwrap();

    // Verify quest exists
    assert!(
        store.quest_exists("test_quest").unwrap(),
        "quest should exist in database"
    );
    let retrieved_quest = store.get_quest("test_quest").unwrap();
    assert_eq!(retrieved_quest.name, "TestQuest");
    assert_eq!(retrieved_quest.difficulty, 1); // Default difficulty
    assert_eq!(retrieved_quest.quest_giver_npc, "mayor_npc");

    // UPDATE quest fields
    let mut updated_quest = retrieved_quest.clone();
    updated_quest.description = "A test quest description".to_string();
    updated_quest.difficulty = 3;
    store.put_quest(updated_quest).unwrap();

    // Verify updates
    let quest_after_update = store.get_quest("test_quest").unwrap();
    assert_eq!(quest_after_update.description, "A test quest description");
    assert_eq!(quest_after_update.difficulty, 3);

    // LIST all quests
    let quest_ids = store.list_quest_ids().unwrap();
    assert!(
        quest_ids.contains(&"test_quest".to_string()),
        "LIST should show our quest"
    );

    // DELETE quest
    store.delete_quest("test_quest").unwrap();

    // Verify quest is gone
    assert!(
        !store.quest_exists("test_quest").unwrap(),
        "quest should be deleted from database"
    );
}

#[test]
fn quest_edit_all_fields() {
    let (store, _temp) = setup_test_store();

    // Create base quest
    let quest = QuestRecord::new("epic_quest", "EpicQuest", "", "elder_wizard", 1);
    store.put_quest(quest).unwrap();

    // EDIT DESCRIPTION
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.description = "A legendary quest for brave adventurers".to_string();
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert_eq!(quest.description, "A legendary quest for brave adventurers");

    // EDIT GIVER
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.quest_giver_npc = "elder_wizard".to_string();
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert_eq!(quest.quest_giver_npc, "elder_wizard");

    // EDIT DIFFICULTY (1-5)
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.difficulty = 4;
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert_eq!(quest.difficulty, 4);

    // EDIT REWARD CURRENCY
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.rewards.currency = Some(CurrencyAmount::decimal(1000));
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    if let Some(CurrencyAmount::Decimal { minor_units }) = quest.rewards.currency {
        assert_eq!(minor_units, 1000);
    } else {
        panic!("Expected currency reward to be set");
    }

    // EDIT REWARD XP
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.rewards.experience = 500;
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert_eq!(quest.rewards.experience, 500);

    // EDIT REWARD ITEM
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.rewards.items.push("legendary_sword".to_string());
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert!(quest.rewards.items.contains(&"legendary_sword".to_string()));

    // EDIT PREREQUISITE
    let mut quest = store.get_quest("epic_quest").unwrap();
    quest.prerequisites.push("welcome_towne".to_string());
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert!(quest.prerequisites.contains(&"welcome_towne".to_string()));

    // ADD OBJECTIVE
    let mut quest = store.get_quest("epic_quest").unwrap();
    let objective = QuestObjective::new(
        "Slay 5 dragons",
        ObjectiveType::KillEnemy {
            enemy_type: "dragon".to_string(),
            count: 5,
        },
        5,
    );
    quest.objectives.push(objective);
    store.put_quest(quest.clone()).unwrap();
    let quest = store.get_quest("epic_quest").unwrap();
    assert!(!quest.objectives.is_empty());
}

#[test]
fn quest_edit_objective_remove() {
    let (store, _temp) = setup_test_store();

    // Create quest with objectives
    let mut quest = QuestRecord::new("obj_test", "ObjectiveTest", "", "mayor_npc", 1);

    let objective1 = QuestObjective::new(
        "Kill 3 rats",
        ObjectiveType::KillEnemy {
            enemy_type: "rat".to_string(),
            count: 3,
        },
        3,
    );

    let objective2 = QuestObjective::new(
        "Collect 5 herbs",
        ObjectiveType::CollectItem {
            item_id: "herb".to_string(),
            count: 5,
        },
        5,
    );

    quest.objectives.push(objective1);
    quest.objectives.push(objective2);
    store.put_quest(quest).unwrap();

    // Verify both objectives exist
    let quest = store.get_quest("obj_test").unwrap();
    assert_eq!(quest.objectives.len(), 2);

    // REMOVE first objective (index 0)
    let mut quest = store.get_quest("obj_test").unwrap();
    quest.objectives.remove(0);
    store.put_quest(quest).unwrap();

    // Verify only one objective remains
    let quest = store.get_quest("obj_test").unwrap();
    assert_eq!(quest.objectives.len(), 1);
    assert!(quest.objectives[0].description.contains("herb"));
}

#[test]
fn quest_exists_check() {
    let (store, _temp) = setup_test_store();

    // Initially should not exist
    assert!(!store.quest_exists("nonexistent").unwrap());

    // Create quest
    let quest = QuestRecord::new("exists_test", "ExistsTest", "", "mayor_npc", 1);
    store.put_quest(quest).unwrap();

    // Now should exist
    assert!(store.quest_exists("exists_test").unwrap());

    // Delete quest
    store.delete_quest("exists_test").unwrap();

    // Should not exist again
    assert!(!store.quest_exists("exists_test").unwrap());
}

#[test]
fn default_quests_seeded() {
    let (store, _temp) = setup_test_store();

    // Should have 11 quests seeded (3 starter + 4 original + 4 Phase 4)
    let quest_ids = store.list_quest_ids().unwrap();
    assert!(
        quest_ids.len() >= 3,
        "should have at least 3 starter quests seeded, got: {}",
        quest_ids.len()
    );

    // Verify some key quests exist
    assert!(store.quest_exists("welcome_towne").unwrap());
    assert!(store.quest_exists("market_exploration").unwrap());
    assert!(store.quest_exists("network_explorer").unwrap());
}
