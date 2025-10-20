use chrono::Utc;
/// Integration tests for fuzzy object name matching and disambiguation
/// Tests partial name matching, disambiguation menus, and selection handling
use meshbbs::tmush::{
    storage::TinyMushStore,
    types::{ObjectOwner, ObjectRecord, RoomOwner, RoomRecord, RoomVisibility},
};
use std::collections::HashMap;
use tempfile::tempdir;

fn create_test_store() -> TinyMushStore {
    let dir = tempdir().unwrap();
    TinyMushStore::open(dir.path()).unwrap()
}

fn create_test_object(id: &str, name: &str, takeable: bool) -> ObjectRecord {
    ObjectRecord {
        id: id.to_string(),
        name: name.to_string(),
        description: format!("A test {}", name),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: meshbbs::tmush::types::CurrencyAmount::decimal(10),
        value: 10,
        takeable,
        usable: false,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: meshbbs::tmush::types::OBJECT_SCHEMA_VERSION,
    }
}

#[allow(dead_code)] // Helper function for potential future tests
fn create_test_room(id: &str, name: &str) -> RoomRecord {
    RoomRecord {
        id: id.to_string(),
        name: name.to_string(),
        short_desc: format!("Test {}", name),
        long_desc: format!("A test room called {}", name),
        owner: RoomOwner::World,
        created_at: Utc::now(),
        visibility: RoomVisibility::Public,
        exits: HashMap::new(),
        items: vec![],
        flags: vec![],
        max_capacity: 15,
        housing_filter_tags: vec![],
        locked: false,
        schema_version: meshbbs::tmush::types::ROOM_SCHEMA_VERSION,
    }
}

#[test]
fn test_disambiguation_session_creation() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let matched_ids = vec!["healing_potion".to_string(), "mana_potion".to_string()];
    let matched_names = vec!["Healing Potion".to_string(), "Mana Potion".to_string()];

    let session = DisambiguationSession::new(
        "testuser",
        "take",
        "potion",
        matched_ids.clone(),
        matched_names.clone(),
        DisambiguationContext::Room,
    );

    assert_eq!(session.player_id, "testuser");
    assert_eq!(session.command, "take");
    assert_eq!(session.search_term, "potion");
    assert_eq!(session.matched_ids.len(), 2);
    assert_eq!(session.matched_names.len(), 2);
}

#[test]
fn test_disambiguation_format_prompt() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let session = DisambiguationSession::new(
        "testuser",
        "take",
        "key",
        vec![
            "ancient_key".to_string(),
            "rusty_key".to_string(),
            "broken_key".to_string(),
        ],
        vec![
            "Ancient Key".to_string(),
            "Rusty Key".to_string(),
            "Broken Key".to_string(),
        ],
        DisambiguationContext::Room,
    );

    let prompt = session.format_prompt();

    assert!(prompt.contains("Did you mean:"));
    assert!(prompt.contains("1) Ancient Key"));
    assert!(prompt.contains("2) Rusty Key"));
    assert!(prompt.contains("3) Broken Key"));
    assert!(prompt.contains("Enter the number"));
}

#[test]
fn test_disambiguation_get_selection_valid() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let session = DisambiguationSession::new(
        "testuser",
        "take",
        "potion",
        vec!["healing_potion".to_string(), "mana_potion".to_string()],
        vec!["Healing Potion".to_string(), "Mana Potion".to_string()],
        DisambiguationContext::Room,
    );

    // Test selecting first item (1-indexed)
    let selection = session.get_selection(1);
    assert!(selection.is_some());
    let (id, name) = selection.unwrap();
    assert_eq!(id, "healing_potion");
    assert_eq!(name, "Healing Potion");

    // Test selecting second item
    let selection = session.get_selection(2);
    assert!(selection.is_some());
    let (id, name) = selection.unwrap();
    assert_eq!(id, "mana_potion");
    assert_eq!(name, "Mana Potion");
}

#[test]
fn test_disambiguation_get_selection_invalid() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let session = DisambiguationSession::new(
        "testuser",
        "take",
        "potion",
        vec!["healing_potion".to_string()],
        vec!["Healing Potion".to_string()],
        DisambiguationContext::Room,
    );

    // Test invalid selections
    assert!(session.get_selection(0).is_none()); // Too low
    assert!(session.get_selection(2).is_none()); // Too high
    assert!(session.get_selection(999).is_none()); // Way too high
}

#[test]
fn test_disambiguation_storage_operations() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let store = create_test_store();

    let session = DisambiguationSession::new(
        "testuser",
        "examine",
        "stone",
        vec!["moonstone".to_string(), "keystone".to_string()],
        vec!["Moonstone".to_string(), "Keystone".to_string()],
        DisambiguationContext::Room,
    );

    // Test put
    let result = store.put_disambiguation_session(session.clone());
    assert!(result.is_ok(), "Failed to store disambiguation session");

    // Test get
    let retrieved = store.get_disambiguation_session("testuser").unwrap();
    assert!(
        retrieved.is_some(),
        "Failed to retrieve disambiguation session"
    );

    let retrieved_session = retrieved.unwrap();
    assert_eq!(retrieved_session.player_id, "testuser");
    assert_eq!(retrieved_session.command, "examine");
    assert_eq!(retrieved_session.search_term, "stone");
    assert_eq!(retrieved_session.matched_ids.len(), 2);

    // Test delete
    let result = store.delete_disambiguation_session("testuser");
    assert!(result.is_ok(), "Failed to delete disambiguation session");

    // Verify deletion
    let after_delete = store.get_disambiguation_session("testuser").unwrap();
    assert!(after_delete.is_none(), "Session should be deleted");
}

#[test]
fn test_disambiguation_context_enum() {
    use meshbbs::tmush::types::DisambiguationContext;

    // Test that enum variants exist and can be created
    let room = DisambiguationContext::Room;
    let inventory = DisambiguationContext::Inventory;
    let both = DisambiguationContext::Both;

    // Test equality
    assert_eq!(room, DisambiguationContext::Room);
    assert_eq!(inventory, DisambiguationContext::Inventory);
    assert_eq!(both, DisambiguationContext::Both);

    // Test inequality
    assert_ne!(room, inventory);
    assert_ne!(inventory, both);
    assert_ne!(room, both);
}

#[test]
fn test_multiple_players_disambiguation_sessions() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let store = create_test_store();

    // Create sessions for multiple players
    let session1 = DisambiguationSession::new(
        "alice",
        "take",
        "key",
        vec!["key1".to_string()],
        vec!["Key 1".to_string()],
        DisambiguationContext::Room,
    );

    let session2 = DisambiguationSession::new(
        "bob",
        "drop",
        "sword",
        vec!["sword1".to_string()],
        vec!["Sword 1".to_string()],
        DisambiguationContext::Inventory,
    );

    // Store both
    store.put_disambiguation_session(session1).unwrap();
    store.put_disambiguation_session(session2).unwrap();

    // Retrieve each player's session
    let alice_session = store.get_disambiguation_session("alice").unwrap();
    assert!(alice_session.is_some());
    assert_eq!(alice_session.unwrap().command, "take");

    let bob_session = store.get_disambiguation_session("bob").unwrap();
    assert!(bob_session.is_some());
    assert_eq!(bob_session.unwrap().command, "drop");

    // Delete one, ensure other remains
    store.delete_disambiguation_session("alice").unwrap();
    assert!(store.get_disambiguation_session("alice").unwrap().is_none());
    assert!(store.get_disambiguation_session("bob").unwrap().is_some());
}

#[test]
fn test_fuzzy_matching_helper_partial_match() {
    // This tests the concept behind find_objects_by_partial_name
    let objects = vec![
        create_test_object("healing_potion", "Healing Potion", true),
        create_test_object("mana_potion", "Mana Potion", true),
        create_test_object("ancient_key", "Ancient Key", true),
    ];

    let search_term = "POTION";
    let matches: Vec<&ObjectRecord> = objects
        .iter()
        .filter(|obj| obj.name.to_uppercase().contains(search_term))
        .collect();

    assert_eq!(matches.len(), 2, "Should match both potions");
    assert!(matches.iter().any(|o| o.id == "healing_potion"));
    assert!(matches.iter().any(|o| o.id == "mana_potion"));
}

#[test]
fn test_fuzzy_matching_single_result() {
    let objects = vec![
        create_test_object("healing_potion", "Healing Potion", true),
        create_test_object("ancient_key", "Ancient Key", true),
    ];

    let search_term = "KEY";
    let matches: Vec<&ObjectRecord> = objects
        .iter()
        .filter(|obj| obj.name.to_uppercase().contains(search_term))
        .collect();

    assert_eq!(matches.len(), 1, "Should match only Ancient Key");
    assert_eq!(matches[0].id, "ancient_key");
}

#[test]
fn test_fuzzy_matching_no_results() {
    let objects = vec![
        create_test_object("healing_potion", "Healing Potion", true),
        create_test_object("ancient_key", "Ancient Key", true),
    ];

    let search_term = "SWORD";
    let matches: Vec<&ObjectRecord> = objects
        .iter()
        .filter(|obj| obj.name.to_uppercase().contains(search_term))
        .collect();

    assert_eq!(matches.len(), 0, "Should match nothing");
}

#[test]
fn test_fuzzy_matching_case_insensitive() {
    let objects = vec![create_test_object("test1", "Healing Potion", true)];

    // Test various case combinations
    for search in &[
        "HEALING", "healing", "Healing", "hEaLiNg", "POTION", "potion",
    ] {
        let matches: Vec<&ObjectRecord> = objects
            .iter()
            .filter(|obj| obj.name.to_uppercase().contains(&search.to_uppercase()))
            .collect();

        assert_eq!(
            matches.len(),
            1,
            "Should match regardless of case: {}",
            search
        );
    }
}

#[test]
fn test_fuzzy_matching_substring() {
    let objects = vec![create_test_object("test1", "Crystal Sword of Power", true)];

    // Test various substrings
    for search in &[
        "Crystal", "Sword", "Power", "Sword of", "of Power", "stal Swo",
    ] {
        let matches: Vec<&ObjectRecord> = objects
            .iter()
            .filter(|obj| obj.name.to_uppercase().contains(&search.to_uppercase()))
            .collect();

        assert_eq!(matches.len(), 1, "Should match substring: {}", search);
    }
}

#[test]
fn test_disambiguation_session_timeout_simulation() {
    use meshbbs::tmush::types::{DisambiguationContext, DisambiguationSession};

    let store = create_test_store();

    let session = DisambiguationSession::new(
        "testuser",
        "take",
        "key",
        vec!["key1".to_string()],
        vec!["Key 1".to_string()],
        DisambiguationContext::Room,
    );

    // Store session
    store.put_disambiguation_session(session).unwrap();

    // Verify it exists
    assert!(store
        .get_disambiguation_session("testuser")
        .unwrap()
        .is_some());

    // Simulate timeout by deleting
    store.delete_disambiguation_session("testuser").unwrap();

    // Verify it's gone
    assert!(store
        .get_disambiguation_session("testuser")
        .unwrap()
        .is_none());
}

#[test]
fn test_exact_match_vs_partial_match() {
    let objects = vec![
        create_test_object("healing_potion", "Healing Potion", true),
        create_test_object("potion_bottle", "Potion Bottle", true),
    ];

    // Partial match - should get both
    let search_term = "POTION";
    let partial_matches: Vec<&ObjectRecord> = objects
        .iter()
        .filter(|obj| obj.name.to_uppercase().contains(search_term))
        .collect();
    assert_eq!(partial_matches.len(), 2);

    // More specific search - should still get both if using contains
    let search_term = "HEALING";
    let specific_matches: Vec<&ObjectRecord> = objects
        .iter()
        .filter(|obj| obj.name.to_uppercase().contains(search_term))
        .collect();
    assert_eq!(specific_matches.len(), 1);
    assert_eq!(specific_matches[0].id, "healing_potion");
}
