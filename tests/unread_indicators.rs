use meshbbs::bbs::BbsServer;
use meshbbs::config::{Config, BbsConfig, StorageConfig, MeshtasticConfig, LoggingConfig, MessageTopicConfig};
use std::collections::HashMap;
use chrono::{Utc, Duration};

fn last_for_node<'a>(msgs: &'a Vec<(String,String)>, node: &str) -> Option<&'a String> {
    for (to, m) in msgs.iter().rev() { if to == node { return Some(m); } }
    None
}

#[tokio::test]
async fn unread_indicators_topics_and_threads() {
    // Minimal config
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let cfg = Config {
        bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "Loc".into(), description: "Desc".into(), max_users: 10, session_timeout: 5, welcome_message: "Welcome".into(), sysop_password_hash: None },
        meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0, min_send_gap_ms: None, dm_resend_backoff_seconds: None, post_dm_broadcast_gap_ms: None, dm_to_dm_gap_ms: None, help_broadcast_delay_ms: None, scheduler_max_queue: None, scheduler_aging_threshold_ms: None, scheduler_stats_interval_ms: None },
        storage: StorageConfig { data_dir: data_dir.to_string_lossy().to_string(), max_message_size: 230 },
        message_topics: {
            let mut m = HashMap::new();
            m.insert("hello".into(), MessageTopicConfig { name: "hello".into(), description: "hi".into(), read_level: 0, post_level: 0 });
            m
        },
        logging: LoggingConfig { level: "info".into(), file: None, security_file: None },
        security: None,
    };

    let mut server = BbsServer::new(cfg.clone()).await.unwrap();

    // Register and login
    server.test_register("alice", "password123").await.unwrap();
    // Rewind last_login to the past
    let user_path = std::path::Path::new(&cfg.storage.data_dir).join("users").join("alice.json");
    let s = tokio::fs::read_to_string(&user_path).await.unwrap();
    let mut u: meshbbs::storage::User = serde_json::from_str(&s).unwrap();
    u.last_login = Utc::now() - Duration::minutes(60);
    tokio::fs::write(&user_path, serde_json::to_string_pretty(&u).unwrap()).await.unwrap();

    // Create a new message after last_login
    server.test_store_message("hello", "carol", "TitleA\n\nBody...").await.unwrap();

    // Start DM session and login
    let node = "n2";
    server.route_test_text_direct(node, "LOGIN alice password123").await.unwrap();

    // Go to topics
    server.route_test_text_direct(node, "M").await.unwrap();
    let m = last_for_node(server.test_messages(), node).expect("topics");
    assert!(m.contains("1. hello (1)"), "topics should show unread count: {}", m);

    // Enter topic and check threads star
    server.route_test_text_direct(node, "1").await.unwrap();
    let m = last_for_node(server.test_messages(), node).expect("threads");
    assert!(m.contains("1 TitleA*"), "threads should mark new thread with *: {}", m);
}
