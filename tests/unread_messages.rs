use meshbbs::config::{Config, BbsConfig, StorageConfig, MeshtasticConfig, MessageAreaConfig, LoggingConfig};
use meshbbs::bbs::BbsServer;
use chrono::{Utc, Duration};
use std::collections::HashMap;

// Basic integration test for unread message counting on login.
#[tokio::test]
async fn unread_message_count_on_login() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let cfg = Config {
        bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "Loc".into(), zipcode: "00000".into(), description: "Desc".into(), max_users: 10, session_timeout: 5, welcome_message: "Welcome".into(), sysop_password_hash: None },
        meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0 },
        storage: StorageConfig { data_dir: data_dir.to_string_lossy().to_string(), max_message_size: 230, message_retention_days: 30, max_messages_per_area: 1000 },
        message_areas: {
            let mut m = HashMap::new();
            m.insert("general".into(), MessageAreaConfig { name: "General".into(), description: "Gen".into(), read_level: 0, post_level: 0 });
            m
        },
        logging: LoggingConfig { level: "info".into(), file: None, security_file: None },
        security: None,
    };
    let cfg_clone = cfg.clone();
    let mut server = BbsServer::new(cfg_clone).await.unwrap();

    // Register a user and store initial last_login
    server.test_register("alice", "passw").await.unwrap();
    // Create two messages after a simulated earlier last_login
    // Manually adjust last_login backward to count messages
    // Access storage base dir indirectly by reading user then constructing path from configured data_dir
    let user_path = std::path::Path::new(&cfg.storage.data_dir).join("users").join("alice.json");
    let content = tokio::fs::read_to_string(&user_path).await.unwrap();
    let mut user: meshbbs::storage::User = serde_json::from_str(&content).unwrap();
    user.last_login = Utc::now() - Duration::minutes(10);
    let new_json = serde_json::to_string_pretty(&user).unwrap();
    tokio::fs::write(&user_path, new_json).await.unwrap();

    server.test_store_message("general", "alice", "hello one").await.unwrap();
    server.test_store_message("general", "alice", "hello two").await.unwrap();

    // Simulate LOGIN path (direct)
    // We call storage.get_user then count
    let before = server.get_user("alice").await.unwrap().unwrap();
    // Use storage via counting by creating a Storage handle is not possible directly (private); simulate call by instantiating a new Storage for scan
    let storage = meshbbs::storage::Storage::new(&cfg.storage.data_dir).await.unwrap();
    let unread = storage.count_messages_since(before.last_login).await.unwrap();
    assert!(unread >= 2, "Expected at least 2 unread messages, got {}", unread);
}
