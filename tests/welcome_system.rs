#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn welcome_messages_on_registration_and_first_login() {
    use meshbbs::config::Config;
    use meshbbs::bbs::BbsServer;
    use meshbbs::meshtastic::TextEvent;

    let mut cfg = Config::default();
    cfg.bbs.sysop = "sysop".to_string(); // Use a valid sysop name that's allowed for sysop role
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_str().unwrap().into();
    let mut server = BbsServer::new(cfg).await.unwrap();

    let node_id: u32 = 0x1234;
    
    // Test 1: Registration should show welcome message
    let register_event = TextEvent { 
        source: node_id, 
        dest: None, 
        is_direct: true, 
        channel: None, 
        content: "REGISTER welcometest password123".into() 
    };
    server.route_text_event(register_event).await.unwrap();

    // Check that session is logged in
    let session = server.test_get_session(&node_id.to_string()).expect("session created");
    assert!(session.is_logged_in(), "Session should be logged in after registration");
    assert_eq!(session.username.as_deref(), Some("welcometest"));

    // Check that registration response contains new compact welcome
    let mut found_registration_compact = false;
    for (_to, msg) in server.test_messages() {
        if msg.contains("Registered as welcometest.") && msg.contains("HELP LIST READ POST WHO") {
            found_registration_compact = true;
            break;
        }
    }
    assert!(found_registration_compact, "Compact registration message not found in test messages");

    // Record current message count for next phase
    let messages_after_registration = server.test_messages().len();

    // Test 2: Logout and login again - should show first login welcome
    let logout_event = TextEvent {
        source: node_id,
        dest: None, 
        is_direct: true,
        channel: None,
        content: "LOGOUT".into()
    };
    server.route_text_event(logout_event).await.unwrap();

    // Verify logout
    let session_after_logout = server.test_get_session(&node_id.to_string());
    assert!(session_after_logout.is_none() || !session_after_logout.unwrap().is_logged_in(), 
            "Session should be logged out");

    // Login again - should trigger first login welcome
    let login_event = TextEvent {
        source: node_id,
        dest: None,
        is_direct: true, 
        channel: None,
        content: "LOGIN welcometest password123".into()
    };
    server.route_text_event(login_event).await.unwrap();

    // Check that session is logged in again
    let session_after_login = server.test_get_session(&node_id.to_string()).expect("session recreated");
    assert!(session_after_login.is_logged_in(), "Session should be logged in after login");

    // First login welcome (previous extended version may differ; now expect basic login line reused)
    let mut found_first_login_basic = false;
    for (_to, msg) in server.test_messages().iter().skip(messages_after_registration) {
        if msg.contains("Welcome, welcometest you are now logged in") {
            found_first_login_basic = true;
            break;
        }
    }
    assert!(found_first_login_basic, "First login basic welcome not found");

    // Record message count for final phase
    let messages_after_first_login = server.test_messages().len();

    // Test 3: Logout and login again - should NOT show welcome message
    let logout_event2 = TextEvent {
        source: node_id,
        dest: None,
        is_direct: true,
        channel: None, 
        content: "LOGOUT".into()
    };
    server.route_text_event(logout_event2).await.unwrap();

    let login_event2 = TextEvent {
        source: node_id,
        dest: None,
        is_direct: true,
        channel: None,
        content: "LOGIN welcometest password123".into()
    };
    server.route_text_event(login_event2).await.unwrap();

    // Subsequent login: ensure only basic login confirmation (no extended celebratory banner expected now)
    let mut found_basic_login = false;
    for (_to, msg) in server.test_messages().iter().skip(messages_after_first_login) {
        if msg.contains("Welcome, welcometest you are now logged in") {
            found_basic_login = true;
            break;
        }
    }
    assert!(found_basic_login, "Basic login message should still appear on subsequent logins");
}

#[tokio::test]
async fn welcome_system_storage_persistence() {
    use meshbbs::storage::Storage;

    let tmpdir = tempfile::tempdir().unwrap();
    let datadir = tmpdir.path().join("data");
    let mut storage = Storage::new(datadir.to_str().unwrap()).await.unwrap();

    // Register a user
    storage.register_user("testuser", "password123", Some("test_node")).await.unwrap();

    // Check initial state - both welcome flags should be false
    let user = storage.get_user("testuser").await.unwrap().unwrap();
    assert!(!user.welcome_shown_on_registration);
    assert!(!user.welcome_shown_on_first_login);

    // Mark registration welcome as shown
    storage.mark_welcome_shown("testuser", true, false).await.unwrap();
    let user_after_reg = storage.get_user("testuser").await.unwrap().unwrap();
    assert!(user_after_reg.welcome_shown_on_registration);
    assert!(!user_after_reg.welcome_shown_on_first_login);

    // Mark first login welcome as shown
    storage.mark_welcome_shown("testuser", false, true).await.unwrap();
    let user_after_first_login = storage.get_user("testuser").await.unwrap().unwrap();
    assert!(user_after_first_login.welcome_shown_on_registration);
    assert!(user_after_first_login.welcome_shown_on_first_login);

    // Test that both can be marked at once
    storage.register_user("testuser2", "password456", Some("test_node2")).await.unwrap();
    storage.mark_welcome_shown("testuser2", true, true).await.unwrap();
    let user2 = storage.get_user("testuser2").await.unwrap().unwrap();
    assert!(user2.welcome_shown_on_registration);
    assert!(user2.welcome_shown_on_first_login);
}