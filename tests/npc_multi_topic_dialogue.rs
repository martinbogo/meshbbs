#![cfg(feature = "meshtastic-proto")]
// Test: Multi-topic NPC dialogue system
// Tests Phase 1 of NPC Dialogue System: topic-based conversations

use meshbbs::bbs::server::BbsServer;
use meshbbs::config::Config;

#[tokio::test]
async fn test_talk_with_topic_wares() {
    let mut cfg = Config::default();
    let _td = tempfile::tempdir().unwrap();
    cfg.storage.data_dir = _td.path().to_string_lossy().to_string();
    cfg.apps.tinymush.enabled = true;
    cfg.apps.tinymush.db_path = Some(format!("{}/tinymush", _td.path().to_string_lossy()));

    let mut server = BbsServer::new(cfg.clone()).await.unwrap();

    // Create session and enter TinyMUSH
    let mut session = meshbbs::bbs::session::Session::new("node1".into(), "node1".into());
    session.login("alice".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    // Enter TinyMUSH - use explicit game name instead of menu position
    server
        .route_test_text_direct(&node_key, "TINYMUSH")
        .await
        .unwrap();

    // Register
    server
        .route_test_text_direct(&node_key, "REGISTER Alice password123")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "password123")
        .await
        .unwrap();

    // Move to Town Square, then to South Market (Mira's location)
    server.route_test_text_direct(&node_key, "N").await.unwrap();
    server.route_test_text_direct(&node_key, "S").await.unwrap();

    // Test TALK with topic
    let before = server.test_messages().len();
    server
        .route_test_text_direct(&node_key, "TALK MIRA WARES")
        .await
        .unwrap();

    let response: Vec<String> = server.test_messages()[before..]
        .iter()
        .filter(|(k, _)| k == &node_key)
        .map(|(_, m)| m.clone())
        .collect();

    let full_response = response.join(" ");
    assert!(
        full_response.contains("Mira"),
        "Expected Mira to respond, got: {}",
        full_response
    );
    assert!(
        !full_response.contains("doesn't know"),
        "Expected valid topic response, got: {}",
        full_response
    );
}

#[tokio::test]
async fn test_talk_list_topics() {
    let mut cfg = Config::default();
    let _td = tempfile::tempdir().unwrap();
    cfg.storage.data_dir = _td.path().to_string_lossy().to_string();
    cfg.apps.tinymush.enabled = true;
    cfg.apps.tinymush.db_path = Some(format!("{}/tinymush", _td.path().to_string_lossy()));

    let mut server = BbsServer::new(cfg.clone()).await.unwrap();

    let mut session = meshbbs::bbs::session::Session::new("node2".into(), "node2".into());
    session.login("bob".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "TINYMUSH")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "REGISTER Bob password123")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "password123")
        .await
        .unwrap();

    // Move to Town Square, then to South Market
    server.route_test_text_direct(&node_key, "N").await.unwrap();
    server.route_test_text_direct(&node_key, "S").await.unwrap();

    // Test LIST keyword
    let before = server.test_messages().len();
    server
        .route_test_text_direct(&node_key, "TALK MIRA LIST")
        .await
        .unwrap();

    let response: Vec<String> = server.test_messages()[before..]
        .iter()
        .filter(|(k, _)| k == &node_key)
        .map(|(_, m)| m.clone())
        .collect();

    let full_response = response.join(" ");
    assert!(
        full_response.contains("can talk about")
            || full_response.contains("wares")
            || full_response.contains("story"),
        "Expected topic list, got: {}",
        full_response
    );
}

#[tokio::test]
async fn test_talk_invalid_topic() {
    let mut cfg = Config::default();
    let _td = tempfile::tempdir().unwrap();
    cfg.storage.data_dir = _td.path().to_string_lossy().to_string();
    cfg.apps.tinymush.enabled = true;
    cfg.apps.tinymush.db_path = Some(format!("{}/tinymush", _td.path().to_string_lossy()));

    let mut server = BbsServer::new(cfg.clone()).await.unwrap();

    let mut session = meshbbs::bbs::session::Session::new("node3".into(), "node3".into());
    session.login("charlie".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "TINYMUSH")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "REGISTER Charlie password123")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "password123")
        .await
        .unwrap();

    // Move to Town Square, then to South Market
    server.route_test_text_direct(&node_key, "N").await.unwrap();
    server.route_test_text_direct(&node_key, "S").await.unwrap();

    // Test invalid topic
    let before = server.test_messages().len();
    server
        .route_test_text_direct(&node_key, "TALK MIRA NONSENSE")
        .await
        .unwrap();

    let response: Vec<String> = server.test_messages()[before..]
        .iter()
        .filter(|(k, _)| k == &node_key)
        .map(|(_, m)| m.clone())
        .collect();

    let full_response = response.join(" ");
    assert!(
        full_response.contains("doesn't know") || full_response.contains("Try:"),
        "Expected error for invalid topic, got: {}",
        full_response
    );
}

#[tokio::test]
async fn test_talk_guard_warning_topic() {
    let mut cfg = Config::default();
    let _td = tempfile::tempdir().unwrap();
    cfg.storage.data_dir = _td.path().to_string_lossy().to_string();
    cfg.apps.tinymush.enabled = true;
    cfg.apps.tinymush.db_path = Some(format!("{}/tinymush", _td.path().to_string_lossy()));

    let mut server = BbsServer::new(cfg.clone()).await.unwrap();

    let mut session = meshbbs::bbs::session::Session::new("node4".into(), "node4".into());
    session.login("diana".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    server
        .route_test_text_direct(&node_key, "TINYMUSH")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "REGISTER Diana password123")
        .await
        .unwrap();
    server
        .route_test_text_direct(&node_key, "password123")
        .await
        .unwrap();

    // Move to Town Square, then to Museum, then to North Gate (Guard's location)
    server.route_test_text_direct(&node_key, "N").await.unwrap();
    server.route_test_text_direct(&node_key, "E").await.unwrap();
    server.route_test_text_direct(&node_key, "S").await.unwrap();

    // Test Guard's warning topic
    let before = server.test_messages().len();
    server
        .route_test_text_direct(&node_key, "TALK GUARD WARNING")
        .await
        .unwrap();

    let response: Vec<String> = server.test_messages()[before..]
        .iter()
        .filter(|(k, _)| k == &node_key)
        .map(|(_, m)| m.clone())
        .collect();

    let full_response = response.join(" ");
    assert!(
        full_response.contains("Guard") || full_response.contains("Gate"),
        "Expected Guard response, got: {}",
        full_response
    );
}
