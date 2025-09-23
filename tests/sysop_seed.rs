use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig};
use meshbbs::bbs::server::BbsServer;
use tokio::runtime::Runtime;
use password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;
use std::collections::HashMap;

#[test]
fn sysop_user_seeded_with_hash() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let tmpdir = tempfile::tempdir().unwrap();
        let datadir = tmpdir.path().join("data");
        let _ = std::fs::create_dir_all(&datadir);
        let salt = SaltString::generate(&mut rand::thread_rng());
        let hash = Argon2::default().hash_password("SecretP@ss1".as_bytes(), &salt).unwrap().to_string();
        let cfg = Config {
            bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), description: "d".into(), max_users: 10, session_timeout: 10, welcome_message: "w".into(), sysop_password_hash: Some(hash.clone()) },
            meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 115200, node_id: "".into(), channel: 0 },
            storage: StorageConfig { data_dir: datadir.to_str().unwrap().to_string(), max_message_size: 1024, message_retention_days: 30, max_messages_per_area: 100 },
            message_topics: HashMap::new(),
            logging: LoggingConfig { level: "info".into(), file: None, security_file: None },
            security: Default::default(),
        };
        let mut server = BbsServer::new(cfg).await.unwrap();
        server.seed_sysop().await.unwrap();
        let u = server.get_user("sysop").await.unwrap().expect("sysop exists");
        assert_eq!(u.user_level, 10);
        assert_eq!(u.password_hash.is_some(), true);
    });
}