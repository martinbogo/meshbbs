//! Integration tests for TinyMUSH functionality validation.
//! Basic tests to ensure TinyMUSH module is properly integrated.

use chrono::Utc;
use meshbbs::bbs::session::{Session, SessionState};
use meshbbs::config::Config;
use meshbbs::storage::Storage;
use meshbbs::tmush::commands::TinyMushProcessor;
use meshbbs::tmush::should_route_to_tinymush;
use meshbbs::tmush::storage::TinyMushStore;
use meshbbs::tmush::types::{PlayerRecord, TutorialState, TutorialStep};
use tempfile::TempDir;

/// Test basic session routing logic
#[tokio::test]
async fn test_tinymush_session_routing() {
    // Test session without game slug - should not route to TinyMUSH
    let session_no_game = Session::new("test_node".to_string(), "12345".to_string());
    assert!(!should_route_to_tinymush(&session_no_game));

    // Test session with TinyMUSH game slug - should route to TinyMUSH
    let mut session_tinymush = Session::new("test_node".to_string(), "12345".to_string());
    session_tinymush.current_game_slug = Some("tinymush".to_string());
    assert!(should_route_to_tinymush(&session_tinymush));

    // Test session with different game slug - should not route to TinyMUSH
    let mut session_other = Session::new("test_node".to_string(), "12345".to_string());
    session_other.current_game_slug = Some("tinyhack".to_string());
    assert!(!should_route_to_tinymush(&session_other));
}

/// Test that TinyMUSH constants are accessible
#[test]
fn test_tinymush_constants_available() {
    // Test that we can access TinyMUSH constants
    use meshbbs::tmush::state::{REQUIRED_LANDING_LOCATION_ID, REQUIRED_START_LOCATION_ID};

    assert_eq!(REQUIRED_LANDING_LOCATION_ID, "gazebo_landing");
    assert_eq!(REQUIRED_START_LOCATION_ID, "town_square");
}

/// Test that room manager is accessible and working
#[test]
fn test_room_manager_available() {
    use meshbbs::tmush::room_manager::RoomManager;

    // Create temp directory and store for testing
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp_dir.path().join("test.db")).unwrap();

    // Test that we can create a room manager
    let room_manager = RoomManager::new(store);

    // Verify cache stats are initialized properly
    let stats = room_manager.cache_stats();
    assert_eq!(stats.cache_size, 0);
    assert_eq!(stats.total_accesses, 0);
    assert_eq!(stats.total_rooms, 0);
    assert!(stats.max_cache_size > 0); // Should have some capacity
}

#[tokio::test]
async fn test_initialize_player_begins_in_gazebo() {
    let temp_dir = TempDir::new().unwrap();
    let tinymush_path = temp_dir.path().join("tinymush");
    let store = TinyMushStore::open(&tinymush_path).unwrap();
    let mut processor = TinyMushProcessor::new(store.clone());

    let mut config = Config::default();
    config.apps.tinymush.enabled = true;
    config.apps.tinymush.db_path = Some(tinymush_path.to_string_lossy().into_owned());
    let storage_dir = temp_dir.path().join("storage");
    config.storage.data_dir = storage_dir.to_string_lossy().into_owned();

    let mut storage = Storage::new(&config.storage.data_dir)
        .await
        .expect("storage");

    let mut session = Session::new("session".into(), "node".into());
    session.username = Some("gazebo_new".into());
    session.user_level = 1;

    let welcome = processor
        .initialize_player(&mut session, &mut storage, &config)
        .await
        .expect("initialize");

    assert!(
        welcome.contains("Landing Gazebo"),
        "expected gazebo description, got: {}",
        welcome
    );

    let player = store.get_player("gazebo_new").unwrap();
    assert!(meshbbs::tmush::state::is_personal_landing(
        &player.current_room
    ));
    assert!(matches!(
        player.tutorial_state,
        TutorialState::InProgress {
            step: TutorialStep::WelcomeAtGazebo
        }
    ));
}

#[tokio::test]
async fn test_initialize_player_relocates_step_one_players() {
    let temp_dir = TempDir::new().unwrap();
    let tinymush_path = temp_dir.path().join("tinymush");
    let store = TinyMushStore::open(&tinymush_path).unwrap();
    let mut processor = TinyMushProcessor::new(store.clone());

    let mut config = Config::default();
    config.apps.tinymush.enabled = true;
    config.apps.tinymush.db_path = Some(tinymush_path.to_string_lossy().into_owned());
    let storage_dir = temp_dir.path().join("storage");
    config.storage.data_dir = storage_dir.to_string_lossy().into_owned();

    let mut storage = Storage::new(&config.storage.data_dir)
        .await
        .expect("storage");

    let mut player = PlayerRecord::new("wanderer", "wanderer", "town_square");
    player.current_room = "town_square".to_string();
    player.tutorial_state = TutorialState::InProgress {
        step: TutorialStep::WelcomeAtGazebo,
    };
    store.put_player(player).unwrap();

    let mut session = Session::new("session2".into(), "node2".into());
    session.username = Some("wanderer".into());
    session.user_level = 1;

    let response = processor
        .initialize_player(&mut session, &mut storage, &config)
        .await
        .expect("initialize");

    assert!(
        response.contains("Landing Gazebo"),
        "expected relocation messaging to include gazebo, got: {}",
        response
    );

    let player = store.get_player("wanderer").unwrap();
    assert!(meshbbs::tmush::state::is_personal_landing(
        &player.current_room
    ));
    assert!(matches!(
        player.tutorial_state,
        TutorialState::InProgress {
            step: TutorialStep::WelcomeAtGazebo
        }
    ));
}

#[tokio::test]
async fn test_players_receive_unique_landing_instances() {
    let temp_dir = TempDir::new().unwrap();
    let tinymush_path = temp_dir.path().join("tinymush_unique");
    let store = TinyMushStore::open(&tinymush_path).unwrap();
    let mut processor = TinyMushProcessor::new(store.clone());

    let mut config = Config::default();
    config.apps.tinymush.enabled = true;
    config.apps.tinymush.db_path = Some(tinymush_path.to_string_lossy().into_owned());
    let storage_dir = temp_dir.path().join("storage");
    config.storage.data_dir = storage_dir.to_string_lossy().into_owned();

    let mut storage = Storage::new(&config.storage.data_dir)
        .await
        .expect("storage");

    let mut session_alice = Session::new("session_a".into(), "node".into());
    session_alice.username = Some("AliceUser".into());
    session_alice.user_level = 1;

    let mut session_bob = Session::new("session_b".into(), "node".into());
    session_bob.username = Some("BobUser".into());
    session_bob.user_level = 1;

    processor
        .initialize_player(&mut session_alice, &mut storage, &config)
        .await
        .expect("alice init");
    processor
        .initialize_player(&mut session_bob, &mut storage, &config)
        .await
        .expect("bob init");

    let alice = store.get_player("AliceUser").unwrap();
    let bob = store.get_player("BobUser").unwrap();

    assert!(meshbbs::tmush::state::is_personal_landing(
        &alice.current_room
    ));
    assert!(meshbbs::tmush::state::is_personal_landing(
        &bob.current_room
    ));
    assert_ne!(alice.current_room, bob.current_room);
}

#[tokio::test]
async fn test_mayor_dialog_available_after_tutorial_completion() {
    let temp_dir = TempDir::new().unwrap();
    let tinymush_path = temp_dir.path().join("tinymush_dialog");
    let store = TinyMushStore::open(&tinymush_path).unwrap();
    let mut processor = TinyMushProcessor::new(store.clone());

    // Ensure dialog tree is present for mayor
    meshbbs::tmush::state::seed_npc_dialogues_if_needed(&store).unwrap();

    let mut config = Config::default();
    config.apps.tinymush.enabled = true;
    config.apps.tinymush.db_path = Some(tinymush_path.to_string_lossy().into_owned());
    let storage_dir = temp_dir.path().join("storage");
    config.storage.data_dir = storage_dir.to_string_lossy().into_owned();

    let mut storage = Storage::new(&config.storage.data_dir)
        .await
        .expect("storage");

    let mut player = PlayerRecord::new("CompletedUser", "CompletedUser", "mayor_office");
    player.tutorial_state = TutorialState::Completed {
        completed_at: Utc::now(),
    };
    store.put_player(player).unwrap();

    let mut session = Session::new("CompletedUser".into(), "CompletedUser".into());
    session.username = Some("CompletedUser".into());
    session.user_level = 1;
    session.state = SessionState::TinyMush;
    session.current_game_slug = Some("tinymush".into());

    let response = processor
        .process_command(&mut session, "TALK MAYOR", &mut storage, &config)
        .await
        .expect("talk response");

    assert!(
        response.contains("Mayor Thompson"),
        "expected mayor greeting, got: {}",
        response
    );
    assert!(
        response
            .to_lowercase()
            .contains("tell me about the tutorial"),
        "expected branching dialog options, got: {}",
        response
    );
}

#[tokio::test]
async fn test_tutorial_requires_talking_to_mayor() {
    let temp_dir = TempDir::new().unwrap();
    let tinymush_path = temp_dir.path().join("tinymush_meet_mayor");
    let store = TinyMushStore::open(&tinymush_path).unwrap();
    let mut processor = TinyMushProcessor::new(store.clone());

    meshbbs::tmush::state::seed_npc_dialogues_if_needed(&store).unwrap();

    let mut config = Config::default();
    config.apps.tinymush.enabled = true;
    config.apps.tinymush.db_path = Some(tinymush_path.to_string_lossy().into_owned());
    let storage_dir = temp_dir.path().join("storage");
    config.storage.data_dir = storage_dir.to_string_lossy().into_owned();

    let mut storage = Storage::new(&config.storage.data_dir)
        .await
        .expect("storage");

    let mut player = PlayerRecord::new("Trailblazer", "Trailblazer", "city_hall_lobby");
    player.tutorial_state = TutorialState::InProgress {
        step: TutorialStep::MeetTheMayor,
    };
    store.put_player(player).unwrap();

    let mut session = Session::new("Trailblazer".into(), "Trailblazer".into());
    session.username = Some("Trailblazer".into());
    session.user_level = 1;
    session.state = SessionState::TinyMush;
    session.current_game_slug = Some("tinymush".into());

    let move_response = processor
        .process_command(&mut session, "NORTH", &mut storage, &config)
        .await
        .expect("move north");

    assert!(move_response.contains("Mayor's Office"));

    let player_after_move = store.get_player("Trailblazer").unwrap();
    assert!(matches!(
        player_after_move.tutorial_state,
        TutorialState::InProgress {
            step: TutorialStep::MeetTheMayor
        }
    ));

    let talk_response = processor
        .process_command(&mut session, "TALK MAYOR THOMPSON", &mut storage, &config)
        .await
        .expect("talk mayor");

    assert!(talk_response.contains("Tutorial Complete"));

    let completed_player = store.get_player("Trailblazer").unwrap();
    assert!(matches!(
        completed_player.tutorial_state,
        TutorialState::Completed { .. }
    ));
}
