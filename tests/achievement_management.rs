/// Integration tests for @ACHIEVEMENT admin command system (Phase 1: Data-Driven Migration)
/// Tests CRUD operations for data-driven achievement management using the storage layer directly.
use meshbbs::tmush::types::{AchievementCategory, AchievementRecord, AchievementTrigger};
use meshbbs::tmush::{TinyMushStore, TinyMushStoreBuilder};
use tempfile::TempDir;

fn setup_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStoreBuilder::new(temp_dir.path()).open().unwrap();
    (store, temp_dir)
}

#[test]
fn achievement_crud_operations() {
    let (store, _temp) = setup_test_store();

    // CREATE a new achievement
    let achievement = AchievementRecord::new(
        "test_ach",
        "Test Achievement",
        "A test achievement",
        AchievementCategory::Special,
        AchievementTrigger::KillCount { required: 1 },
    );
    store.put_achievement(achievement.clone()).unwrap();

    // Verify achievement exists
    assert!(
        store.achievement_exists("test_ach").unwrap(),
        "achievement should exist in database"
    );
    let retrieved = store.get_achievement("test_ach").unwrap();
    assert_eq!(retrieved.name, "Test Achievement");
    assert_eq!(retrieved.description, "A test achievement");

    // UPDATE achievement fields
    let mut updated = retrieved.clone();
    updated.description = "Updated description".to_string();
    updated.category = AchievementCategory::Combat;
    store.put_achievement(updated).unwrap();

    // Verify updates
    let after_update = store.get_achievement("test_ach").unwrap();
    assert_eq!(after_update.description, "Updated description");
    matches!(after_update.category, AchievementCategory::Combat);

    // LIST all achievements
    let ids = store.list_achievement_ids().unwrap();
    assert!(
        ids.contains(&"test_ach".to_string()),
        "LIST should show our achievement"
    );

    // DELETE achievement
    store.delete_achievement("test_ach").unwrap();

    // Verify achievement is gone
    assert!(
        !store.achievement_exists("test_ach").unwrap(),
        "achievement should be deleted from database"
    );
}

#[test]
fn achievement_edit_all_fields() {
    let (store, _temp) = setup_test_store();

    let mut achievement = AchievementRecord::new(
        "full_test",
        "Full Test",
        "Initial description",
        AchievementCategory::Special,
        AchievementTrigger::KillCount { required: 1 },
    );
    store.put_achievement(achievement.clone()).unwrap();

    // Edit description
    achievement.description = "New description".to_string();
    store.put_achievement(achievement.clone()).unwrap();
    let retrieved = store.get_achievement("full_test").unwrap();
    assert_eq!(retrieved.description, "New description");

    // Edit category (test all 6 categories)
    for category in [
        AchievementCategory::Combat,
        AchievementCategory::Exploration,
        AchievementCategory::Social,
        AchievementCategory::Economic,
        AchievementCategory::Quest,
        AchievementCategory::Special,
    ] {
        achievement.category = category.clone();
        store.put_achievement(achievement.clone()).unwrap();
        let retrieved = store.get_achievement("full_test").unwrap();
        assert_eq!(
            format!("{:?}", retrieved.category),
            format!("{:?}", category)
        );
    }

    // Edit title
    achievement.title = Some("the Brave".to_string());
    store.put_achievement(achievement.clone()).unwrap();
    let retrieved = store.get_achievement("full_test").unwrap();
    assert_eq!(retrieved.title, Some("the Brave".to_string()));

    // Clear title
    achievement.title = None;
    store.put_achievement(achievement.clone()).unwrap();
    let retrieved = store.get_achievement("full_test").unwrap();
    assert_eq!(retrieved.title, None);

    // Edit hidden flag
    achievement.hidden = true;
    store.put_achievement(achievement.clone()).unwrap();
    let retrieved = store.get_achievement("full_test").unwrap();
    assert_eq!(retrieved.hidden, true);

    achievement.hidden = false;
    store.put_achievement(achievement.clone()).unwrap();
    let retrieved = store.get_achievement("full_test").unwrap();
    assert_eq!(retrieved.hidden, false);
}

#[test]
fn achievement_trigger_types() {
    let (store, _temp) = setup_test_store();

    // Test all 9 trigger types
    let triggers = vec![
        ("killcount", AchievementTrigger::KillCount { required: 10 }),
        (
            "roomvisits",
            AchievementTrigger::RoomVisits { required: 50 },
        ),
        (
            "friendcount",
            AchievementTrigger::FriendCount { required: 5 },
        ),
        (
            "messagessent",
            AchievementTrigger::MessagesSent { required: 100 },
        ),
        (
            "tradecount",
            AchievementTrigger::TradeCount { required: 20 },
        ),
        (
            "currencyearned",
            AchievementTrigger::CurrencyEarned { amount: 1000 },
        ),
        (
            "questcompletion",
            AchievementTrigger::QuestCompletion { required: 3 },
        ),
        (
            "visitlocation",
            AchievementTrigger::VisitLocation {
                room_id: "secret_room".to_string(),
            },
        ),
        (
            "completequest",
            AchievementTrigger::CompleteQuest {
                quest_id: "epic_quest".to_string(),
            },
        ),
    ];

    for (id, trigger) in triggers {
        let achievement = AchievementRecord::new(
            id,
            &format!("{} test", id),
            "Test trigger",
            AchievementCategory::Special,
            trigger.clone(),
        );
        store.put_achievement(achievement).unwrap();

        let retrieved = store.get_achievement(id).unwrap();
        assert_eq!(format!("{:?}", retrieved.trigger), format!("{:?}", trigger));
    }
}

#[test]
fn achievement_category_validation() {
    let (store, _temp) = setup_test_store();

    let categories = vec![
        AchievementCategory::Combat,
        AchievementCategory::Exploration,
        AchievementCategory::Social,
        AchievementCategory::Economic,
        AchievementCategory::Quest,
        AchievementCategory::Special,
    ];

    for (i, category) in categories.iter().enumerate() {
        let achievement = AchievementRecord::new(
            &format!("cat_{}", i),
            &format!("Category {} Test", i),
            "Category test",
            category.clone(),
            AchievementTrigger::KillCount { required: 1 },
        );
        store.put_achievement(achievement).unwrap();

        let retrieved = store.get_achievement(&format!("cat_{}", i)).unwrap();
        assert_eq!(
            format!("{:?}", retrieved.category),
            format!("{:?}", category)
        );
    }
}

#[test]
fn achievement_exists_check() {
    let (store, _temp) = setup_test_store();

    // Should not exist initially
    assert!(!store.achievement_exists("nonexistent").unwrap());

    // Create achievement
    let achievement = AchievementRecord::new(
        "exists_test",
        "Exists Test",
        "Test existence check",
        AchievementCategory::Special,
        AchievementTrigger::KillCount { required: 1 },
    );
    store.put_achievement(achievement).unwrap();

    // Should exist now
    assert!(store.achievement_exists("exists_test").unwrap());

    // Delete achievement
    store.delete_achievement("exists_test").unwrap();

    // Should not exist anymore
    assert!(!store.achievement_exists("exists_test").unwrap());
}

#[test]
fn achievement_list_filtering() {
    let (store, _temp) = setup_test_store();

    // Create achievements in different categories
    let combat_ach = AchievementRecord::new(
        "custom_combat1",
        "Custom Combat 1",
        "Custom combat achievement",
        AchievementCategory::Combat,
        AchievementTrigger::KillCount { required: 10 },
    );
    store.put_achievement(combat_ach).unwrap();

    let explore_ach = AchievementRecord::new(
        "custom_explore1",
        "Custom Explore 1",
        "Custom exploration achievement",
        AchievementCategory::Exploration,
        AchievementTrigger::RoomVisits { required: 50 },
    );
    store.put_achievement(explore_ach).unwrap();

    let social_ach = AchievementRecord::new(
        "custom_social1",
        "Custom Social 1",
        "Custom social achievement",
        AchievementCategory::Social,
        AchievementTrigger::FriendCount { required: 5 },
    );
    store.put_achievement(social_ach).unwrap();

    // List all achievements (includes our custom ones + seeded ones)
    let all_ids = store.list_achievement_ids().unwrap();
    assert!(all_ids.contains(&"custom_combat1".to_string()));
    assert!(all_ids.contains(&"custom_explore1".to_string()));
    assert!(all_ids.contains(&"custom_social1".to_string()));

    // Filter by category - should at least contain our custom ones
    let combat_achs = store
        .get_achievements_by_category(&AchievementCategory::Combat)
        .unwrap();
    assert!(combat_achs.iter().any(|a| a.id == "custom_combat1"));

    let explore_achs = store
        .get_achievements_by_category(&AchievementCategory::Exploration)
        .unwrap();
    assert!(explore_achs.iter().any(|a| a.id == "custom_explore1"));

    let social_achs = store
        .get_achievements_by_category(&AchievementCategory::Social)
        .unwrap();
    assert!(social_achs.iter().any(|a| a.id == "custom_social1"));
}

#[test]
fn default_achievements_seeded() {
    use meshbbs::tmush::state::seed_starter_achievements;

    let (store, _temp) = setup_test_store();

    // Seed default achievements
    for achievement in seed_starter_achievements() {
        store.put_achievement(achievement).unwrap();
    }

    // Verify some known starter achievements exist
    let ids = store.list_achievement_ids().unwrap();

    // Check that we have multiple achievements seeded
    assert!(
        ids.len() >= 10,
        "Should have at least 10 starter achievements"
    );

    // Verify first_blood exists (from seed_starter_achievements)
    assert!(
        ids.contains(&"first_blood".to_string()),
        "first_blood achievement should be seeded"
    );
}
