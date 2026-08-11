/// Integration tests for player monitoring commands (Phase 9.4)
///
/// Tests the @PLAYERS, WHERE, and @GOTO commands added in Phase 9.3
/// for admin oversight and alpha testing management.
///
/// Note: These tests use test_tmush_ensure_player_exists() to force
/// TinyMUSH player record creation before granting admin privileges.
/// This helper sends a "look" command to trigger lazy initialization.
use meshbbs::bbs::server::BbsServer;
use meshbbs::bbs::session::Session;
use meshbbs::config::Config;

/// Helper to create a base config for testing
async fn test_config() -> Config {
    let mut cfg = Config::default();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().to_string_lossy().to_string();
    cfg.storage.data_dir = temp_path.clone();
    cfg.bbs.max_users = 20;
    cfg.apps.tinymush.enabled = true;
    cfg.apps.tinymush.db_path = Some(format!("{}/tinymush", temp_path));
    // Keep temp_dir alive by leaking it (tests are short-lived anyway)
    std::mem::forget(temp_dir);
    cfg
}

#[tokio::test]
async fn test_players_command_lists_all() {
    let config = test_config().await;
    let data_dir = config.storage.data_dir.clone();

    // Clean up any existing TinyMUSH database to start fresh
    let tmush_db = std::path::Path::new(&data_dir).join("tinymush");
    if tmush_db.exists() {
        std::fs::remove_dir_all(&tmush_db).ok();
    }

    let mut server = BbsServer::new(config).await.unwrap();

    // Register alice in BBS
    server.test_register("alice", "password123").await.unwrap();

    // Create session for alice
    let mut session_alice = Session::new("node_alice".into(), "node_alice".into());
    session_alice.login("alice".into(), 1).await.unwrap();
    let alice_key = session_alice.node_id.clone();
    server.test_insert_session(session_alice);

    // Create a session with username "admin" to match the seeded TinyMUSH admin player
    // No need to register in BBS since we just need the username to match
    let mut session_admin = Session::new("node_admin".into(), "node_admin".into());
    session_admin.username = Some("admin".to_string());
    session_admin.user_level = 1;
    let admin_key = session_admin.node_id.clone();
    server.test_insert_session(session_admin);

    // Enter TinyMUSH as alice - this creates alice's player
    server
        .route_test_text_direct(&alice_key, "G1")
        .await
        .unwrap();

    // Enter TinyMUSH as admin - this will find the seeded admin player with sysop privileges
    server
        .route_test_text_direct(&admin_key, "G1")
        .await
        .unwrap();

    // Grant alice admin using the @SETADMIN command AS admin
    server
        .route_test_text_direct(&admin_key, "@setadmin alice 2")
        .await
        .unwrap();

    // Execute @PLAYERS command as alice
    server
        .route_test_text_direct(&alice_key, "@players")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    // Verify alice is listed and total count is shown
    assert!(
        result.contains("alice") || result.contains("ALICE"),
        "Should list alice: {}",
        result
    );
    assert!(
        result.contains("Total:") || result.contains("players"),
        "Should show player count: {}",
        result
    );
}

/// Test @PLAYERS command denies access to non-admins

#[tokio::test]
async fn test_players_command_requires_admin() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server
        .test_register("charlie", "password789")
        .await
        .unwrap();

    let mut session = Session::new("node201".into(), "node201".into());
    session.login("charlie".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "@players")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("Permission denied") || result.contains("⛔"),
        "Should deny access"
    );
}

/// Test WHERE command shows own location for regular users

#[tokio::test]
async fn test_where_shows_own_location() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server.test_register("dave", "password301").await.unwrap();

    let mut session = Session::new("node301".into(), "node301".into());
    session.login("dave".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "WHERE")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("You are in:") || result.contains("📍"),
        "Should show location: {}",
        result
    );

    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("You are in:") || result.contains("📍"),
        "Should show location"
    );
}

/// Test WHERE <player> locates another player (admin only)

#[tokio::test]
async fn test_where_player_locates_others_admin_only() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server
        .test_register("moderator1", "password401")
        .await
        .unwrap();
    server.test_register("eve", "password402").await.unwrap();

    // Create moderator session
    let mut mod_session = Session::new("node401".into(), "node401".into());
    mod_session.login("moderator1".into(), 1).await.unwrap();
    let mod_key = mod_session.node_id.clone();
    server.test_insert_session(mod_session);

    // Create eve session
    let mut eve_session = Session::new("node402".into(), "node402".into());
    eve_session.login("eve".into(), 1).await.unwrap();
    let eve_key = eve_session.node_id.clone();
    server.test_insert_session(eve_session);

    // Both enter TinyMUSH
    server.route_test_text_direct(&mod_key, "G1").await.unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&mod_key)
        .await
        .unwrap();

    // Grant admin AFTER entering TinyMUSH
    server
        .test_tmush_grant_admin("moderator1", 1)
        .await
        .unwrap();

    server.route_test_text_direct(&eve_key, "G1").await.unwrap();

    // Force TinyMUSH player record creation for eve too
    server
        .test_tmush_ensure_player_exists(&eve_key)
        .await
        .unwrap();

    // Moderator locates eve
    server
        .route_test_text_direct(&mod_key, "where eve")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("📍") || result.contains("eve") || result.contains("EVE"),
        "Should show target player location"
    );
}

/// Test WHERE <player> denies access to non-admins

#[tokio::test]
async fn test_where_player_requires_admin() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server.test_register("frank", "password501").await.unwrap();
    server.test_register("grace", "password502").await.unwrap();

    let mut session = Session::new("node501".into(), "node501".into());
    session.login("frank".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "where grace")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("Permission denied") || result.contains("⛔"),
        "Should deny access"
    );
}

/// Test WHERE <player> handles player-not-found

#[tokio::test]
async fn test_where_player_not_found() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server
        .test_register("moderator2", "password601")
        .await
        .unwrap();

    let mut session = Session::new("node601".into(), "node601".into());
    session.login("moderator2".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    // Grant admin AFTER entering TinyMUSH
    server
        .test_tmush_grant_admin("moderator2", 2)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "where nonexistent")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("not found") || result.contains("❌"),
        "Should show not found error"
    );
}

/// Test @GOTO <room> teleports admin to specific room

#[tokio::test]
async fn test_goto_room_teleports_admin() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server
        .test_register("moderator3", "password701")
        .await
        .unwrap();

    let mut session = Session::new("node701".into(), "node701".into());
    session.login("moderator3".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    // Grant admin AFTER entering TinyMUSH
    server
        .test_tmush_grant_admin("moderator3", 1)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "@goto market")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("✈️") || result.contains("market") || result.contains("MARKET"),
        "Should teleport to market"
    );
}

/// Test @GOTO denies access to non-admins

#[tokio::test]
async fn test_goto_requires_admin() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server.test_register("iris", "password801").await.unwrap();

    let mut session = Session::new("node801".into(), "node801".into());
    session.login("iris".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "@goto market")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result.contains("Permission denied") || result.contains("⛔"),
        "Should deny access"
    );
}

/// Test @GOTO handles invalid target

#[tokio::test]
async fn test_goto_invalid_target() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    server
        .test_register("moderator4", "password901")
        .await
        .unwrap();

    let mut session = Session::new("node901".into(), "node901".into());
    session.login("moderator4".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();

    // Force TinyMUSH player record creation
    server
        .test_tmush_ensure_player_exists(&node_key)
        .await
        .unwrap();

    // Grant admin AFTER entering TinyMUSH
    server
        .test_tmush_grant_admin("moderator4", 2)
        .await
        .unwrap();

    server
        .route_test_text_direct(&node_key, "@goto nonexistent_place")
        .await
        .unwrap();
    let messages = server.test_messages();
    let result = messages
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    eprintln!("GOTO INVALID TARGET RESULT:\n{}", result);
    assert!(
        result.contains("not found") || result.contains("❌"),
        "Should show not found error. Got: {}",
        result
    );
}

/// Test permission levels (Moderator, Admin, Sysop all work)

#[tokio::test]
async fn test_monitoring_commands_all_admin_levels() {
    let config = test_config().await;
    let mut server = BbsServer::new(config).await.unwrap();

    // Test each admin level (1=Moderator, 2=Admin, 3=Sysop)
    for level in 1..=3 {
        let username = format!("moderator{}", level);
        server
            .test_register(&username, "password123")
            .await
            .unwrap();

        let node_id = format!("node1{:03}", level);
        let mut session = Session::new(node_id.clone(), node_id.clone());
        session.login(username.clone(), 1).await.unwrap();
        let node_key = session.node_id.clone();
        server.test_insert_session(session);

        server
            .route_test_text_direct(&node_key, "G1")
            .await
            .unwrap();

        // Force TinyMUSH player record creation
        server
            .test_tmush_ensure_player_exists(&node_key)
            .await
            .unwrap();

        // Grant admin AFTER entering TinyMUSH
        server
            .test_tmush_grant_admin(&username, level)
            .await
            .unwrap();

        // Test @PLAYERS
        server
            .route_test_text_direct(&node_key, "@players")
            .await
            .unwrap();
        let messages = server.test_messages();
        let result = messages
            .iter()
            .map(|(_, msg)| msg.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !result.contains("Permission denied"),
            "Level {} should have access to @PLAYERS",
            level
        );

        // Test @GOTO
        server
            .route_test_text_direct(&node_key, "@goto market")
            .await
            .unwrap();
        let messages = server.test_messages();
        let result = messages
            .iter()
            .map(|(_, msg)| msg.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !result.contains("Permission denied"),
            "Level {} should have access to @GOTO",
            level
        );
    }
}
