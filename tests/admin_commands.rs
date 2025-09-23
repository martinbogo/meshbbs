//! Test administrative commands functionality
use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig};
use meshbbs::bbs::server::BbsServer;
use std::collections::HashMap;

async fn base_config() -> Config {
    Config {
        bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), description: "d".into(), max_users: 10, session_timeout: 10, welcome_message: "w".into(), sysop_password_hash: None },
        meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0 },
        storage: StorageConfig { data_dir: tempfile::tempdir().unwrap().path().join("data").to_str().unwrap().to_string(), max_message_size: 1024, message_retention_days: 30, max_messages_per_area: 100 },
        message_areas: HashMap::new(),
        logging: LoggingConfig { level: "error".into(), file: None, security_file: None },
        security: Default::default(),
    }
}

#[tokio::test]
async fn admin_commands_moderator_access() {
    let cfg = base_config().await;
    let mut server = BbsServer::new(cfg).await.unwrap();
    
    // Create test users
    server.test_register("alice", "password123").await.unwrap();
    server.test_register("bob", "password456").await.unwrap();
    server.test_update_level("alice", 5).await.unwrap(); // Make alice a moderator
    
    // Test session for moderator
    let mut alice_session = meshbbs::bbs::session::Session::new("alice_session".into(), "alice_node".into());
    alice_session.login("alice".into(), 5).await.unwrap();
    let alice_node = alice_session.node_id.clone();
    server.test_insert_session(alice_session);
    
    // Test USERS command
    server.route_test_text_direct(&alice_node, "USERS").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("Registered Users"));
    assert!(response.contains("alice"));
    assert!(response.contains("bob"));
    
    // Test WHO command  
    server.route_test_text_direct(&alice_node, "WHO").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("Logged In Users"));
    assert!(response.contains("alice"));
    
    // Test USERINFO command
    server.route_test_text_direct(&alice_node, "USERINFO bob").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("User Information"));
    assert!(response.contains("bob"));
    
    // Test SESSIONS command
    server.route_test_text_direct(&alice_node, "SESSIONS").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("Active Sessions"));
    
    // Test ADMIN command
    server.route_test_text_direct(&alice_node, "ADMIN").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("BBS Administration Dashboard"));
}

#[tokio::test]
async fn admin_commands_user_denied() {
    let cfg = base_config().await;
    let mut server = BbsServer::new(cfg).await.unwrap();
    
    // Create regular user
    server.test_register("charlie", "password789").await.unwrap();
    
    // Test session for regular user
    let mut charlie_session = meshbbs::bbs::session::Session::new("charlie_session".into(), "charlie_node".into());
    charlie_session.login("charlie".into(), 1).await.unwrap();
    let charlie_node = charlie_session.node_id.clone();
    server.test_insert_session(charlie_session);
    
    // Test that admin commands are denied for regular users
    let admin_commands = vec!["USERS", "WHO", "USERINFO bob", "SESSIONS", "KICK alice", "ADMIN"];
    
    for cmd in admin_commands {
        server.route_test_text_direct(&charlie_node, cmd).await.unwrap();
        let response = server.test_messages().last().unwrap().1.clone();
        assert!(response.contains("Permission denied"), "Command '{}' should be denied for regular user", cmd);
    }
}

#[tokio::test]
async fn broadcast_and_kick_commands() {
    let cfg = base_config().await;
    let mut server = BbsServer::new(cfg).await.unwrap();
    
    // Create users
    server.test_register("moderator", "password").await.unwrap();
    server.test_update_level("moderator", 5).await.unwrap(); // Make moderator a moderator
    
    // Create session
    let mut admin_session = meshbbs::bbs::session::Session::new("admin_session".into(), "admin_node".into());
    admin_session.login("moderator".into(), 5).await.unwrap();
    let admin_node = admin_session.node_id.clone();
    server.test_insert_session(admin_session);
    
    // Test BROADCAST command
    server.route_test_text_direct(&admin_node, "BROADCAST System maintenance in 5 minutes").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("Broadcast sent:"));
    assert!(response.contains("SYSTEM MAINTENANCE IN 5 MINUTES"));
    
    // Test KICK command
    server.route_test_text_direct(&admin_node, "KICK troublemaker").await.unwrap();
    let response = server.test_messages().last().unwrap().1.clone();
    assert!(response.contains("TROUBLEMAKER has been kicked"));
}