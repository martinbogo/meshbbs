use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig, MessageTopicConfig};
use meshbbs::bbs::server::BbsServer;
use std::collections::HashMap;

async fn config_with_areas(dir: &str) -> Config {
    let mut areas = HashMap::new();
    areas.insert("general".into(), MessageTopicConfig { name: "General".into(), description: "g".into(), read_level: 0, post_level: 0 });
    areas.insert("mods".into(), MessageTopicConfig { name: "Mods".into(), description: "m".into(), read_level: 5, post_level: 5 });
    areas.insert("ann".into(), MessageTopicConfig { name: "Ann".into(), description: "a".into(), read_level: 0, post_level: 10 });
    Config {
        bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), description: "d".into(), max_users: 10, session_timeout: 10, welcome_message: "w".into(), sysop_password_hash: None },
    meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0, min_send_gap_ms: None, dm_resend_backoff_seconds: None, post_dm_broadcast_gap_ms: None, dm_to_dm_gap_ms: None, help_broadcast_delay_ms: None },
        storage: StorageConfig { data_dir: dir.to_string(), max_message_size: 1024 },
        message_topics: areas,
        logging: LoggingConfig { level: "error".into(), file: None, security_file: None },
        security: Default::default(),
    }
}

#[tokio::test]
async fn permission_enforcement_read_post() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let cfg = config_with_areas(data_dir.to_str().unwrap()).await;
    let mut server = BbsServer::new(cfg).await.unwrap();

    server.test_register("alice", "Password123").await.unwrap();
    server.test_register("mod", "Password123").await.unwrap();
    server.test_register("admin_user", "Password123").await.unwrap();
    server.test_update_level("mod",5).await.unwrap();
    server.test_update_level("admin_user",10).await.unwrap();

    // General area accessible
    server.test_store_message("general", "alice", "hi").await.unwrap();

    // mods area: alice (lvl1) cannot post
    let err = server.test_store_message("mods", "alice", "secret").await.err();
    assert!(err.is_some());
    // moderator can post
    server.test_store_message("mods", "mod", "notice").await.unwrap();

    // ann area post_level 10: moderator cannot post
    let err2 = server.test_store_message("ann", "mod", "announcement").await.err();
    assert!(err2.is_some());
}
