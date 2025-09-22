use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig, WebConfig, MessageAreaConfig};
use meshbbs::bbs::server::BbsServer;
use std::collections::HashMap;

async fn config_with_areas(dir: &str) -> Config {
    let mut areas = HashMap::new();
    areas.insert("general".into(), MessageAreaConfig { name: "General".into(), description: "g".into(), read_level: 0, post_level: 0 });
    areas.insert("mods".into(), MessageAreaConfig { name: "Mods".into(), description: "m".into(), read_level: 5, post_level: 5 });
    areas.insert("ann".into(), MessageAreaConfig { name: "Ann".into(), description: "a".into(), read_level: 0, post_level: 10 });
    Config {
        bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), zipcode: "00000".into(), description: "d".into(), max_users: 10, welcome_message: "w".into(), sysop_password_hash: None },
        meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0 },
        storage: StorageConfig { data_dir: dir.to_string(), max_message_size: 1024, message_retention_days: 30, max_messages_per_area: 100 },
        message_areas: areas,
        web: WebConfig { enabled: false, bind_address: "127.0.0.1".into(), port: 8080, admin_username: "a".into(), admin_password: "b".into() },
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
    server.test_register("sysop", "Password123").await.unwrap();
    server.test_update_level("mod",5).await.unwrap();
    server.test_update_level("sysop",10).await.unwrap_err(); // sysop immutable path; ensure test uses configured sysop

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
