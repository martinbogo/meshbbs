use argon2::Argon2;
use meshbbs::bbs::server::BbsServer;
use meshbbs::config::{
    AdminDashboardConfig, BbsConfig, Config, GamesConfig, IdentBeaconConfig, LoggingConfig,
    MeshtasticConfig, StorageConfig,
};
use password_hash::{PasswordHasher, SaltString};
use std::collections::HashMap;
use tokio::runtime::Runtime;

#[test]
fn sysop_user_seeded_with_hash() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let tmpdir = tempfile::tempdir().unwrap();
        let datadir = tmpdir.path().join("data");
        let _ = std::fs::create_dir_all(&datadir);
        let salt = SaltString::generate(&mut rand::thread_rng());
        let hash = Argon2::default()
            .hash_password("SecretP@ss1".as_bytes(), &salt)
            .unwrap()
            .to_string();
        let cfg = Config {
            bbs: BbsConfig {
                name: "Test".into(),
                sysop: "sysop".into(),
                location: "loc".into(),
                description: "d".into(),
                max_users: 10,
                session_timeout: 10,
                welcome_message: "w".into(),
                sysop_password_hash: Some(hash.clone()),
                public_command_prefix: None,
                allow_public_login: true,
                help_command: "HELP".to_string(),
            },
            meshtastic: MeshtasticConfig {
                port: "".into(),
                baud_rate: 115200,
                node_id: "".into(),
                channel: 0,
                min_send_gap_ms: None,
                dm_resend_backoff_seconds: None,
                post_dm_broadcast_gap_ms: None,
                dm_to_dm_gap_ms: None,
                help_broadcast_delay_ms: None,
                scheduler_max_queue: None,
                scheduler_aging_threshold_ms: None,
                scheduler_stats_interval_ms: None,
                require_device_at_startup: false,
            },
            storage: StorageConfig {
                data_dir: datadir.to_str().unwrap().to_string(),
                max_message_size: 1024,
                show_chunk_markers: false,
            },
            message_topics: HashMap::new(),
            logging: LoggingConfig {
                level: "info".into(),
                file: None,
                security_file: None,
            },
            security: None,
            ident_beacon: IdentBeaconConfig::default(),
            weather: Default::default(),
            games: GamesConfig::default(),
            welcome: meshbbs::bbs::welcome::WelcomeConfig {
                enabled: false,
                public_greeting: true,
                private_guide: true,
                cooldown_minutes: 5,
                max_welcomes_per_node: 1,
            },
            admin_dashboard: AdminDashboardConfig::default(),
        };
        let mut server = BbsServer::new(cfg).await.unwrap();
        server.seed_sysop().await.unwrap();
        let u = server
            .get_user("sysop")
            .await
            .unwrap()
            .expect("sysop exists");
        assert_eq!(u.user_level, 10);
        assert!(u.password_hash.is_some());
    });
}
