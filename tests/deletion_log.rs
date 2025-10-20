use meshbbs::config::{
    AdminDashboardConfig, BbsConfig, Config, LoggingConfig, MeshtasticConfig, StorageConfig,
};
use meshbbs::{bbs::server::BbsServer, config::*};
use std::collections::HashMap;

async fn base_config(dir: &str) -> Config {
    Config {
        bbs: BbsConfig {
            name: "Test".into(),
            sysop: "sysop".into(),
            location: "loc".into(),
            description: "d".into(),
            max_users: 10,
            session_timeout: 10,
            welcome_message: "w".into(),
            sysop_password_hash: None,
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
            data_dir: dir.to_string(),
            max_message_size: 1024,
            show_chunk_markers: false,
        },
        message_topics: HashMap::new(),
        logging: LoggingConfig {
            level: "error".into(),
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

    // Create the general topic first (sysop-only operation)
    server
        .test_create_topic(
            "general",
            "General Discussion",
            "General topic for discussion",
            0,
            1,
            "sysop",
        )
        .await
        .unwrap();

    // Seed 25 messages then delete them to get 25 audit entries
    for i in 0..25 {
        let id = server
            .test_store_message("general", "mod", &format!("msg{i}"))
            .await
            .unwrap();
        let _ = server
            .moderator_delete_message("general", &id, "mod")
            .await
            .unwrap();
    }

    let page1 = server.test_deletion_page(1, 10).await.unwrap();
    let page2 = server.test_deletion_page(2, 10).await.unwrap();
    let page3 = server.test_deletion_page(3, 10).await.unwrap();

    assert_eq!(page1.len(), 10);
    assert_eq!(page2.len(), 10);
    assert_eq!(page3.len(), 5);
}
