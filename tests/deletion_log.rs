use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig};
use meshbbs::bbs::server::BbsServer;
use std::collections::HashMap;

async fn base_config(dir: &str) -> Config {
    Config {
    bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), zipcode: "00000".into(), description: "d".into(), max_users: 10, session_timeout: 10, welcome_message: "w".into(), sysop_password_hash: None },
        meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0 },
        storage: StorageConfig { data_dir: dir.to_string(), max_message_size: 1024, message_retention_days: 30, max_messages_per_area: 100 },
        message_areas: HashMap::new(),
        logging: LoggingConfig { level: "error".into(), file: None, security_file: None },
        security: Default::default(),
    }
}

#[tokio::test]
async fn deletion_log_pagination() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let cfg = base_config(data_dir.to_str().unwrap()).await;
    let mut server = BbsServer::new(cfg).await.unwrap();
    server.test_register("mod", "Password123").await.unwrap();
    server.test_update_level("mod", 5).await.unwrap();

    // Seed 25 messages then delete them to get 25 audit entries
    for i in 0..25 {
        let id = server.test_store_message("general", "mod", &format!("msg{i}")).await.unwrap();
        let _ = server.moderator_delete_message("general", &id, "mod").await.unwrap();
    }

    let page1 = server.test_deletion_page(1, 10).await.unwrap();
    let page2 = server.test_deletion_page(2, 10).await.unwrap();
    let page3 = server.test_deletion_page(3, 10).await.unwrap();

    assert_eq!(page1.len(), 10);
    assert_eq!(page2.len(), 10);
    assert_eq!(page3.len(), 5);
}
