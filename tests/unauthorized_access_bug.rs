//! Test to verify that unauthorized users CANNOT access BBS functions without logging in
#![cfg(feature = "meshtastic-proto")]

use meshbbs::bbs::session::SessionState;
use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;
use meshbbs::meshtastic::TextEvent;
use tempfile::TempDir;

/// This test verifies the FIX: unauthenticated users CANNOT access the Topics menu
#[tokio::test]
async fn test_unauthenticated_blocked_from_topics() {
    let tmpdir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = tmpdir.path().join("data").to_string_lossy().to_string();
    config.bbs.sysop = "testsysop".to_string();

    let mut server = BbsServer::new(config).await.expect("server creation");

    // Send first message to establish session
    let unauth_node_id = 123;
    let initial_msg = TextEvent {
        source: unauth_node_id,
        dest: Some(1),
        is_direct: true,
        channel: None,
        content: "HI".into(),
    };
    server
        .route_text_event(initial_msg)
        .await
        .expect("initial message");

    // NOW try to access Topics with 'M' command (session already exists)
    let try_messages = TextEvent {
        source: unauth_node_id,
        dest: Some(1),
        is_direct: true,
        channel: None,
        content: "M".into(),
    };
    server
        .route_text_event(try_messages)
        .await
        .expect("messages command");

    // Check the session state
    let session = server.test_get_session(&unauth_node_id.to_string());
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();

    // Check what was sent to the user
    let messages = server.test_messages();
    let response = messages
        .iter()
        .filter(|(node, _)| *node == unauth_node_id.to_string())
        .last()
        .map(|(_, msg)| msg.as_str())
        .unwrap_or("");

    println!("\n========================================");
    println!("AUTHENTICATION SECURITY TEST");
    println!("========================================");
    println!("Session state: {:?}", session.state);
    println!("Username: {:?}", session.username);
    println!("User level: {}", session.user_level);
    println!("\nResponse to 'M' command:");
    println!("{}", response);
    println!("========================================\n");

    // THE FIX: Unauthenticated users should NOT be able to transition to Topics state
    // They should remain in MainMenu and receive an authentication required message
    assert_eq!(
        session.state,
        SessionState::MainMenu,
        "Unauthenticated user should remain in MainMenu, not transition to Topics"
    );

    assert!(session.username.is_none(), "User should not be logged in");
    assert_eq!(session.user_level, 0, "User should have level 0");

    // Verify the response contains authentication required message
    assert!(
        response.contains("Authentication required")
            || response.contains("REGISTER")
            || response.contains("LOGIN"),
        "Response should indicate authentication is required. Got: {}",
        response
    );

    println!("✓ SECURITY FIX VERIFIED!");
    println!("  Unauthenticated user was properly blocked from accessing Topics");
}

/// Test that unauthenticated users are blocked from Games menu
#[tokio::test]
async fn test_unauthenticated_blocked_from_games() {
    let tmpdir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = tmpdir.path().join("data").to_string_lossy().to_string();
    config.bbs.sysop = "testsysop".to_string();

    let mut server = BbsServer::new(config).await.expect("server creation");

    // Try to access Games with 'G' command WITHOUT logging in
    let unauth_node_id = 456;
    let try_games = TextEvent {
        source: unauth_node_id,
        dest: Some(1),
        is_direct: true,
        channel: None,
        content: "G".into(),
    };
    server
        .route_text_event(try_games)
        .await
        .expect("games command");

    let messages = server.test_messages();
    let response = messages
        .iter()
        .find(|(node, _)| *node == unauth_node_id.to_string())
        .map(|(_, msg)| msg.as_str())
        .unwrap_or("");

    // Check session - user should not be logged in
    let session = server.test_get_session(&unauth_node_id.to_string());
    if let Some(session) = session {
        assert!(session.username.is_none(), "User should not be logged in");
        assert_eq!(
            session.state,
            SessionState::MainMenu,
            "User should remain in MainMenu"
        );

        // FIX: Response should indicate authentication is required
        assert!(
            response.contains("Authentication required")
                || response.contains("REGISTER")
                || response.contains("LOGIN"),
            "Response should indicate authentication is required for games. Got: {}",
            response
        );

        println!("✓ Games menu properly blocked for unauthenticated user");
    }
}

/// Test that unauthenticated users are blocked from Preferences menu
#[tokio::test]
async fn test_unauthenticated_blocked_from_preferences() {
    let tmpdir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = tmpdir.path().join("data").to_string_lossy().to_string();
    config.bbs.sysop = "testsysop".to_string();

    let mut server = BbsServer::new(config).await.expect("server creation");

    // Try to access Preferences with 'P' command WITHOUT logging in
    let unauth_node_id = 789;
    let try_prefs = TextEvent {
        source: unauth_node_id,
        dest: Some(1),
        is_direct: true,
        channel: None,
        content: "P".into(),
    };
    server
        .route_text_event(try_prefs)
        .await
        .expect("preferences command");

    // Check the session state
    let session = server.test_get_session(&unauth_node_id.to_string());
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();

    println!("\n=== Preferences Access Test ===");
    println!("Session state: {:?}", session.state);
    println!("Username: {:?}", session.username);

    // FIX: Unauthenticated users should be blocked from UserMenu state
    assert_eq!(
        session.state,
        SessionState::MainMenu,
        "Unauthenticated user should remain in MainMenu, not access UserMenu"
    );

    assert!(session.username.is_none(), "User is not logged in");

    let messages = server.test_messages();
    let response = messages
        .iter()
        .filter(|(node, _)| *node == unauth_node_id.to_string())
        .last()
        .map(|(_, msg)| msg.as_str())
        .unwrap_or("");

    assert!(
        response.contains("Authentication required")
            || response.contains("REGISTER")
            || response.contains("LOGIN"),
        "Response should indicate authentication is required. Got: {}",
        response
    );

    println!("✓ Preferences menu properly blocked for unauthenticated user");
}
