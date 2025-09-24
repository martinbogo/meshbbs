use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig};
use meshbbs::bbs::server::BbsServer;
use std::collections::HashMap;

async fn base_config(dir: &str) -> Config {
    Config {
    bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), description: "d".into(), max_users: 10, session_timeout: 10, welcome_message: "w".into(), sysop_password_hash: None },
    meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0, min_send_gap_ms: None, dm_resend_backoff_seconds: None, post_dm_broadcast_gap_ms: None, dm_to_dm_gap_ms: None, help_broadcast_delay_ms: None, scheduler_max_queue: None, scheduler_aging_threshold_ms: None, scheduler_stats_interval_ms: None },
        storage: StorageConfig { data_dir: dir.to_string(), max_message_size: 1024 },
        message_topics: HashMap::new(),
        logging: LoggingConfig { level: "error".into(), file: None, security_file: None },
        security: Default::default(),
    }
}

#[tokio::test]
async fn lock_persists_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let cfg1 = base_config(data_dir.to_str().unwrap()).await;
    {
        let mut server = BbsServer::new(cfg1.clone()).await.unwrap();
        server.test_register("mod", "Password123").await.unwrap();
        server.test_update_level("mod", 5).await.unwrap();
        server.moderator_lock_topic("general", "mod").await.unwrap();
        // Ensure lock file written
    assert!(server.test_is_locked("general"));
    }
    // Recreate server with same data dir
    let cfg2 = base_config(data_dir.to_str().unwrap()).await;
    let server2 = BbsServer::new(cfg2).await.unwrap();
    assert!(server2.test_is_locked("general"), "Lock did not persist across restart");
}
