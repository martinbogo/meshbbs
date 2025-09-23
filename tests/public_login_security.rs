#![cfg(feature = "meshtastic-proto")]
use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;
use meshbbs::meshtastic::TextEvent;
use tempfile::TempDir;

/// Test that public login for password-protected users requires authentication
#[tokio::test]
async fn public_login_with_password_requires_auth() {
    let tmpdir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = tmpdir.path().join("data").to_string_lossy().to_string();
    config.bbs.sysop = "testsysop".to_string(); // Set valid sysop name

    let mut server = BbsServer::new(config).await.expect("server creation");

    // First, create a user with a password using the regular registration flow
    let register_event = TextEvent {
        source: 123,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "REGISTER alice secretpass123".into(),
    };
    server.route_text_event(register_event).await.expect("user registration");

    // Logout the user to clean slate
    let logout_event = TextEvent {
        source: 123,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "LOGOUT".into(),
    };
    server.route_text_event(logout_event).await.expect("logout");

    // Now simulate public LOGIN attempt
    let public_event = TextEvent {
        source: 123,
        dest: None,
        is_direct: false,
        channel: None,
        content: "^LOGIN alice".into(),
    };
    server.route_text_event(public_event).await.expect("public login");

    // Open DM - should NOT auto-login since user has password
    let dm_event = TextEvent {
        source: 123,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "HI".into(),
    };
    server.route_text_event(dm_event).await.expect("dm hi");

    // Try to access authenticated commands - should fail since not logged in
    let read_event = TextEvent {
        source: 123,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "READ".into(),
    };
    server.route_text_event(read_event).await.expect("read attempt");

    // Check that user is NOT logged in by checking session state
    let session = server.test_get_session("123");
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();
    assert!(session.username.is_none(), "User should NOT be auto-logged in");

    // Now provide correct password to complete login
    let login_with_pass = TextEvent {
        source: 123,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "LOGIN alice secretpass123".into(),
    };
    server.route_text_event(login_with_pass).await.expect("login with password");

    // Now check that user IS logged in
    let session = server.test_get_session("123");
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();
    assert!(session.username.is_some(), "User should be logged in after password");
    assert_eq!(session.username.as_ref().unwrap(), "alice", "Correct username");
}

/// Test that public login for passwordless users still works (backward compatibility)
#[tokio::test]
async fn public_login_without_password_auto_succeeds() {
    let tmpdir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = tmpdir.path().join("data").to_string_lossy().to_string();
    config.bbs.sysop = "testsysop".to_string(); // Set valid sysop name

    let mut server = BbsServer::new(config).await.expect("server creation");

    // Simulate public LOGIN for new user (no password)
    let public_event = TextEvent {
        source: 456,
        dest: None,
        is_direct: false,
        channel: None,
        content: "^LOGIN bob".into(),
    };
    server.route_text_event(public_event).await.expect("public login");

    // Open DM - should auto-login since user has no password
    let dm_event = TextEvent {
        source: 456,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "HI".into(),
    };
    server.route_text_event(dm_event).await.expect("dm hi");

    // Check that user IS logged in automatically
    let session = server.test_get_session("456");
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();
    assert!(session.username.is_some(), "User should be auto-logged in");
    assert_eq!(session.username.as_ref().unwrap(), "bob", "Correct username");
}

/// Test that wrong password after public login is rejected
#[tokio::test]
async fn public_login_wrong_password_rejected() {
    let tmpdir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = tmpdir.path().join("data").to_string_lossy().to_string();
    config.bbs.sysop = "testsysop".to_string(); // Set valid sysop name

    let mut server = BbsServer::new(config).await.expect("server creation");

    // Create user with password
    let register_event = TextEvent {
        source: 789,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "REGISTER charlie mypassword".into(),
    };
    server.route_text_event(register_event).await.expect("user registration");

    // Logout
    let logout_event = TextEvent {
        source: 789,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "LOGOUT".into(),
    };
    server.route_text_event(logout_event).await.expect("logout");

    // Public login
    let public_event = TextEvent {
        source: 789,
        dest: None,
        is_direct: false,
        channel: None,
        content: "^LOGIN charlie".into(),
    };
    server.route_text_event(public_event).await.expect("public login");

    // Open DM
    let dm_event = TextEvent {
        source: 789,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "HI".into(),
    };
    server.route_text_event(dm_event).await.expect("dm hi");

    // Try wrong password
    let wrong_pass = TextEvent {
        source: 789,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "LOGIN charlie wrongpassword".into(),
    };
    server.route_text_event(wrong_pass).await.expect("wrong password attempt");

    // Check that user is still NOT logged in
    let session = server.test_get_session("789");
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();
    assert!(session.username.is_none(), "User should NOT be logged in with wrong password");

    // Try correct password
    let correct_pass = TextEvent {
        source: 789,
        dest: Some(999),
        is_direct: true,
        channel: None,
        content: "LOGIN charlie mypassword".into(),
    };
    server.route_text_event(correct_pass).await.expect("correct password");

    // Now should be logged in
    let session = server.test_get_session("789");
    assert!(session.is_some(), "Session should exist");
    let session = session.unwrap();
    assert!(session.username.is_some(), "User should be logged in with correct password");
    assert_eq!(session.username.as_ref().unwrap(), "charlie", "Correct username");
}