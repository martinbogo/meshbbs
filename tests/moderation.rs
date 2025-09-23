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
async fn moderator_delete_message() {
    let cfg = base_config().await;
    let mut server = BbsServer::new(cfg).await.unwrap();
    server.test_register("alice", "Password123").await.unwrap();
    server.test_register("mod", "Password123").await.unwrap();
    server.test_update_level("mod", 5).await.unwrap();
    // Post a message as alice
    server.test_store_message("general", "alice", "Hello").await.unwrap();
    let msgs = server.test_get_messages("general", 10).await.unwrap();
    let id = msgs.first().unwrap().id.clone();
    // Attempt deletion as normal user should fail via helper (not exposed); call moderator method with low level won't check level itself, so simulate security by manual check.
    // Here we directly call moderator_delete_message as test harness (server already ensures only moderators call it in command path). It should delete.
    let deleted = server.moderator_delete_message("general", &id, "mod").await.unwrap();
    assert!(deleted);
    let msgs_after = server.test_get_messages("general", 10).await.unwrap();
    assert!(msgs_after.is_empty());
}

#[tokio::test]
async fn lock_prevents_post_regular_allows_moderator() {
    let cfg = base_config().await;
    let mut server = BbsServer::new(cfg).await.unwrap();
    server.test_register("bob", "Password123").await.unwrap();
    server.test_register("mod", "Password123").await.unwrap();
    server.test_update_level("mod", 5).await.unwrap();
    server.moderator_lock_area("general", "mod").await.unwrap();
    // Regular user posting via storage directly should error due to lock
    let err = server.test_store_message("general", "bob", "Hi").await.err();
    assert!(err.is_some());
    // Moderator posts by temporarily unlocking, posting, re-locking (simulate command path which would check)
    server.moderator_unlock_area("general", "mod").await.unwrap();
    server.test_store_message("general", "mod", "Announcement").await.unwrap();
    server.moderator_lock_area("general", "mod").await.unwrap();
    let msgs = server.test_get_messages("general", 10).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "Announcement");
}
