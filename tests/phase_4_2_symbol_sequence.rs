//! Integration test for Phase 4.2: Symbol Sequence Tracking
//!
//! Tests that the examined_symbol_sequence field in PlayerRecord
//! properly tracks and persists symbol examination order.

use meshbbs::tmush::types::PlayerRecord;
use meshbbs::tmush::TinyMushStoreBuilder;
use tempfile::TempDir;

#[test]
fn player_has_examined_symbol_sequence_field() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a new player
    let player = PlayerRecord::new("tester", "Test Player", "town_square");

    // Verify the examined_symbol_sequence field exists and is empty
    assert!(
        player.examined_symbol_sequence.is_empty(),
        "New player should have empty examined_symbol_sequence"
    );

    // Save the player
    store.put_player(player.clone()).expect("save player");

    // Reload and verify field persists
    let loaded = store.get_player("tester").expect("get player");
    assert!(loaded.examined_symbol_sequence.is_empty());
}

#[test]
fn symbol_sequence_persists_across_saves() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a player
    let mut player = PlayerRecord::new("symbol_tracker", "Symbol Tracker", "town_square");

    // Add symbols to the sequence
    player.examined_symbol_sequence.push("oak".to_string());
    player.examined_symbol_sequence.push("elm".to_string());
    player.examined_symbol_sequence.push("willow".to_string());
    player.examined_symbol_sequence.push("ash".to_string());

    // Save
    store.put_player(player.clone()).expect("save player");

    // Reload
    let loaded = store.get_player("symbol_tracker").expect("get player");

    // Verify sequence persisted correctly
    assert_eq!(
        loaded.examined_symbol_sequence,
        vec!["oak", "elm", "willow", "ash"],
        "Symbol sequence should persist across saves"
    );
}

#[test]
fn symbol_sequence_can_be_wrong_order() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a player
    let mut player = PlayerRecord::new("wrong_order", "Wrong Order", "town_square");

    // Add symbols in WRONG order
    player.examined_symbol_sequence.push("ash".to_string());
    player.examined_symbol_sequence.push("oak".to_string());
    player.examined_symbol_sequence.push("elm".to_string());

    // Save
    store.put_player(player.clone()).expect("save player");

    // Reload
    let loaded = store.get_player("wrong_order").expect("get player");

    // Verify wrong sequence is still tracked
    assert_eq!(
        loaded.examined_symbol_sequence,
        vec!["ash", "oak", "elm"],
        "Wrong symbol sequence should still be tracked"
    );
}

#[test]
fn symbol_sequence_can_be_modified() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a player with initial sequence
    let mut player = PlayerRecord::new("modifier", "Modifier", "town_square");
    player.examined_symbol_sequence.push("oak".to_string());
    store.put_player(player.clone()).expect("save initial");

    // Load, modify, and save again
    let mut loaded = store.get_player("modifier").expect("get player");
    loaded.examined_symbol_sequence.push("elm".to_string());
    loaded.examined_symbol_sequence.push("willow".to_string());
    store.put_player(loaded.clone()).expect("save modified");

    // Reload and verify modifications
    let final_player = store.get_player("modifier").expect("get final");
    assert_eq!(
        final_player.examined_symbol_sequence,
        vec!["oak", "elm", "willow"],
        "Symbol sequence modifications should persist"
    );
}

#[test]
fn symbol_sequence_can_be_cleared() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a player with sequence
    let mut player = PlayerRecord::new("clearer", "Clearer", "town_square");
    player.examined_symbol_sequence.push("oak".to_string());
    player.examined_symbol_sequence.push("elm".to_string());
    store
        .put_player(player.clone())
        .expect("save with sequence");

    // Load, clear, and save
    let mut loaded = store.get_player("clearer").expect("get player");
    loaded.examined_symbol_sequence.clear();
    store.put_player(loaded.clone()).expect("save cleared");

    // Reload and verify cleared
    let final_player = store.get_player("clearer").expect("get final");
    assert!(
        final_player.examined_symbol_sequence.is_empty(),
        "Symbol sequence should be clearable"
    );
}
