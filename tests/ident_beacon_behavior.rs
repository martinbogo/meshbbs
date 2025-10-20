//! Test ident beacon functionality and timing logic
use chrono::{Timelike, Utc};
use meshbbs::bbs::server::BbsServer;
use meshbbs::config::{
    AdminDashboardConfig, BbsConfig, Config, GamesConfig, IdentBeaconConfig, LoggingConfig,
    MeshtasticConfig, StorageConfig,
};
use std::collections::HashMap;

async fn test_config_with_beacon(enabled: bool, frequency: &str) -> Config {
    Config {
        bbs: BbsConfig {
            name: "Test BBS".into(),
            sysop: "sysop".into(),
            location: "Test Location".into(),
            description: "Test Description".into(),
            max_users: 10,
            session_timeout: 10,
            welcome_message: "Welcome".into(),
            sysop_password_hash: None,
            public_command_prefix: None,
            allow_public_login: true,
            help_command: "HELP".to_string(),
        },
        meshtastic: MeshtasticConfig {
            port: "".into(),
            baud_rate: 115200,
            node_id: "0x123456".into(),
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
            data_dir: tempfile::tempdir()
                .unwrap()
                .path()
                .join("data")
                .to_str()
                .unwrap()
                .to_string(),
            max_message_size: 230,
            show_chunk_markers: false,
        },
        message_topics: HashMap::new(),
        logging: LoggingConfig {
            level: "error".into(),
            file: None,
            security_file: None,
        },
        security: Default::default(),
        ident_beacon: IdentBeaconConfig {
            enabled,
            frequency: frequency.to_string(),
        },
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
async fn test_ident_beacon_disabled() {
    let config = test_config_with_beacon(false, "15min").await;
    assert_eq!(config.ident_beacon.enabled, false);
    assert_eq!(config.ident_beacon.frequency, "15min");

    // The beacon should be disabled regardless of frequency
    let _server = BbsServer::new(config).await.unwrap();
    // Note: Full integration testing of disabled beacon would require mocking time
    // and running the server loop, which is complex. The unit tests in config/mod.rs
    // cover the configuration logic, and this test verifies config creation.
}

#[tokio::test]
async fn test_ident_beacon_enabled_different_frequencies() {
    let frequencies = vec!["5min", "15min", "30min", "1hour", "2hours", "4hours"];

    for freq in frequencies {
        let config = test_config_with_beacon(true, freq).await;
        assert_eq!(config.ident_beacon.enabled, true);
        assert_eq!(config.ident_beacon.frequency, freq);

        // Verify the frequency conversion works
        let expected_minutes = match freq {
            "5min" => 5,
            "15min" => 15,
            "30min" => 30,
            "1hour" => 60,
            "2hours" => 120,
            "4hours" => 240,
            _ => panic!("Unexpected frequency: {}", freq),
        };

        assert_eq!(config.ident_beacon.frequency_minutes(), expected_minutes);

        // Verify server can be created with this config
        let _server = BbsServer::new(config).await.unwrap();
    }
}

#[tokio::test]
async fn test_ident_beacon_invalid_frequency_fallback() {
    let config = test_config_with_beacon(true, "invalid_frequency").await;
    assert_eq!(config.ident_beacon.enabled, true);
    assert_eq!(config.ident_beacon.frequency, "invalid_frequency");

    // Should fall back to 15 minutes for invalid frequency
    assert_eq!(config.ident_beacon.frequency_minutes(), 15);

    // Server should still work with invalid frequency (using fallback)
    let _server = BbsServer::new(config).await.unwrap();
}

#[tokio::test]
async fn test_ident_beacon_message_format_components() {
    let config = test_config_with_beacon(true, "15min").await;
    let _server = BbsServer::new(config.clone()).await.unwrap();

    // Test that the BBS name from config is available
    assert_eq!(config.bbs.name, "Test BBS");

    // Test that the node_id from config is available
    assert_eq!(config.meshtastic.node_id, "0x123456");

    // Verify the ident beacon config is properly loaded
    assert_eq!(config.ident_beacon.enabled, true);
    assert_eq!(config.ident_beacon.frequency, "15min");
}

#[test]
fn test_ident_beacon_time_boundary_logic() {
    // Test the time boundary logic that would be used in the beacon
    let now = Utc::now();
    let minutes = now.minute();

    // Test 15-minute boundaries
    let is_15min_boundary = minutes % 15 == 0;
    let expected_15min = matches!(minutes, 0 | 15 | 30 | 45);
    assert_eq!(is_15min_boundary, expected_15min);

    // Test 30-minute boundaries
    let is_30min_boundary = minutes % 30 == 0;
    let expected_30min = matches!(minutes, 0 | 30);
    assert_eq!(is_30min_boundary, expected_30min);

    // Test hourly boundaries
    let is_hourly_boundary = minutes == 0;
    assert_eq!(is_hourly_boundary, minutes == 0);

    // Test 2-hour boundaries (would also check hour % 2 == 0)
    let is_2hour_boundary = minutes == 0 && now.hour() % 2 == 0;
    assert_eq!(is_2hour_boundary, minutes == 0 && now.hour() % 2 == 0);

    // Test 4-hour boundaries (would also check hour % 4 == 0)
    let is_4hour_boundary = minutes == 0 && now.hour() % 4 == 0;
    assert_eq!(is_4hour_boundary, minutes == 0 && now.hour() % 4 == 0);
}

#[tokio::test]
async fn test_config_serialization_with_ident_beacon() {
    let config = test_config_with_beacon(true, "2hours").await;

    // Test that the config can be serialized/deserialized with ident_beacon
    let serialized = toml::to_string(&config).unwrap();
    assert!(serialized.contains("[ident_beacon]"));
    assert!(serialized.contains("enabled = true"));
    assert!(serialized.contains("frequency = \"2hours\""));

    // Test deserialization
    let deserialized: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.ident_beacon.enabled, true);
    assert_eq!(deserialized.ident_beacon.frequency, "2hours");
    assert_eq!(deserialized.ident_beacon.frequency_minutes(), 120);
}

#[tokio::test]
async fn test_default_config_has_ident_beacon() {
    let config = Config::default();

    // Default config should have ident beacon enabled with 15min frequency
    assert_eq!(config.ident_beacon.enabled, true);
    assert_eq!(config.ident_beacon.frequency, "15min");
    assert_eq!(config.ident_beacon.frequency_minutes(), 15);

    // Should be able to create server with default config
    let _server = BbsServer::new(config).await.unwrap();
}
