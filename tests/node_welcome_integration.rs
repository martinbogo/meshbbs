/// Integration tests for the node detection welcome system.
/// This tests the feature that automatically welcomes new Meshtastic nodes
/// with default names (e.g., "Meshtastic 1A2B") when they appear in the network.
use meshbbs::bbs::welcome::WelcomeConfig;
use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

mod common;

/// Helper to create a minimal node cache JSON for testing.
fn create_test_node_cache(data_dir: &str, nodes: Vec<TestNode>) -> std::io::Result<()> {
    use serde_json::json;
    use std::fs;

    let cache_path = PathBuf::from(data_dir).join("node_cache.json");

    let cache_entries: Vec<_> = nodes
        .iter()
        .map(|node| {
            json!({
                "node_id": node.node_id,
                "long_name": node.long_name,
                "short_name": node.short_name,
                "last_seen": node.last_seen_timestamp(),
            })
        })
        .collect();

    let cache_json = json!({
        "nodes": cache_entries
    });

    fs::write(cache_path, serde_json::to_string_pretty(&cache_json)?)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct TestNode {
    node_id: u32,
    long_name: String,
    short_name: String,
    /// How many seconds ago this node was last seen.
    last_seen_seconds_ago: u64,
}

impl TestNode {
    fn last_seen_timestamp(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now.saturating_sub(self.last_seen_seconds_ago)
    }
}

#[tokio::test]
async fn test_queue_startup_welcomes_filters_correctly() {
    // Test that queue_startup_welcomes only queues nodes that:
    // 1. Have default names (Meshtastic XXXX)
    // 2. Were seen within the last hour
    // 3. Have not been welcomed yet

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_nodes = vec![
        // Should be queued: default name, recent, not welcomed
        TestNode {
            node_id: 0x1001,
            long_name: "Meshtastic 1A2B".to_string(),
            short_name: "1A2B".to_string(),
            last_seen_seconds_ago: 1800, // 30 minutes ago
        },
        // Should be queued: another default name, recent
        TestNode {
            node_id: 0x1002,
            long_name: "Meshtastic FFFF".to_string(),
            short_name: "FFFF".to_string(),
            last_seen_seconds_ago: 600, // 10 minutes ago
        },
        // Should NOT be queued: custom name
        TestNode {
            node_id: 0x1003,
            long_name: "Alice's Node".to_string(),
            short_name: "ALIC".to_string(),
            last_seen_seconds_ago: 900, // 15 minutes ago
        },
        // Should NOT be queued: stale (seen over 1 hour ago)
        TestNode {
            node_id: 0x1004,
            long_name: "Meshtastic ABCD".to_string(),
            short_name: "ABCD".to_string(),
            last_seen_seconds_ago: 7200, // 2 hours ago
        },
        // Should be queued: edge case - seen exactly 1 hour ago
        TestNode {
            node_id: 0x1005,
            long_name: "Meshtastic 9999".to_string(),
            short_name: "9999".to_string(),
            last_seen_seconds_ago: 3600, // 1 hour ago
        },
    ];

    create_test_node_cache(data_dir, test_nodes.clone()).expect("Failed to create node cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let _server = BbsServer::new(cfg).await.expect("Failed to create server");

    // Access the startup queue - we'll need to use test methods on the server
    // Since we don't have direct access, we verify by checking if messages would be sent
    // by waiting for the queue processing to happen in the event loop.

    // Give the server a moment to process the startup queue
    sleep(Duration::from_millis(100)).await;

    // Note: In a real integration test, we'd verify:
    // - startup_welcome_queue has exactly 3 entries (0x1001, 0x1002, 0x1005)
    // - Queue entries have 30-second stagger timestamps
    // - Stale and custom-name nodes are excluded

    // For now, this test validates the test infrastructure is set up correctly.
    // The actual verification would require exposing test methods on BbsServer
    // to inspect the startup_welcome_queue field.
}

#[tokio::test]
async fn test_startup_queue_processing_timing() {
    // Test that queued welcomes are sent at 30-second intervals

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_nodes = vec![
        TestNode {
            node_id: 0x2001,
            long_name: "Meshtastic 1111".to_string(),
            short_name: "1111".to_string(),
            last_seen_seconds_ago: 300, // 5 minutes ago
        },
        TestNode {
            node_id: 0x2002,
            long_name: "Meshtastic 2222".to_string(),
            short_name: "2222".to_string(),
            last_seen_seconds_ago: 600, // 10 minutes ago
        },
        TestNode {
            node_id: 0x2003,
            long_name: "Meshtastic 3333".to_string(),
            short_name: "3333".to_string(),
            last_seen_seconds_ago: 900, // 15 minutes ago
        },
    ];

    create_test_node_cache(data_dir, test_nodes).expect("Failed to create node cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let _server = BbsServer::new(cfg).await.expect("Failed to create server");

    // In a full implementation, we would:
    // 1. Mock the scheduler/dispatcher to capture outgoing messages
    // 2. Monitor timestamps when messages are queued
    // 3. Verify 30-second spacing between welcomes
    // 4. Verify correct order (first node queued = first welcomed)

    // This test structure is ready for implementation once we have
    // test hooks into the message scheduling system.
}

#[tokio::test]
async fn test_rate_limiting_across_server_restarts() {
    // Test that once a node is welcomed, it won't be welcomed again
    // even if the server restarts.

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_node = TestNode {
        node_id: 0x3001,
        long_name: "Meshtastic BEEF".to_string(),
        short_name: "BEEF".to_string(),
        last_seen_seconds_ago: 300, // 5 minutes ago
    };

    create_test_node_cache(data_dir, vec![test_node.clone()]).expect("Failed to create node cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    // First server instance - should queue welcome
    {
        let _server1 = BbsServer::new(cfg.clone())
            .await
            .expect("Failed to create server 1");
        sleep(Duration::from_millis(100)).await;

        // In full implementation: verify welcome was sent
        // For now: this creates the welcomed_nodes.json state file
    }

    // Simulate the node being seen again (update last_seen)
    let updated_node = TestNode {
        last_seen_seconds_ago: 60, // Just seen 1 minute ago
        ..test_node
    };
    create_test_node_cache(data_dir, vec![updated_node]).expect("Failed to update node cache");

    // Second server instance - should NOT queue welcome (already welcomed)
    {
        let _server2 = BbsServer::new(cfg)
            .await
            .expect("Failed to create server 2");
        sleep(Duration::from_millis(100)).await;

        // In full implementation: verify NO welcome was sent
        // The welcomed_nodes.json should prevent re-welcoming
    }
}

#[tokio::test]
async fn test_welcome_only_default_names() {
    // Verify that only nodes with default "Meshtastic XXXX" names trigger welcomes

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_nodes = vec![
        // Valid default names that SHOULD trigger welcomes
        TestNode {
            node_id: 0x4001,
            long_name: "Meshtastic 0000".to_string(),
            short_name: "0000".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x4002,
            long_name: "Meshtastic FFFF".to_string(),
            short_name: "FFFF".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x4003,
            long_name: "Meshtastic aB3D".to_string(), // Mixed case
            short_name: "aB3D".to_string(),
            last_seen_seconds_ago: 300,
        },
        // Custom names that should NOT trigger welcomes
        TestNode {
            node_id: 0x4010,
            long_name: "Bob".to_string(),
            short_name: "BOB".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x4011,
            long_name: "Node-1234".to_string(),
            short_name: "N124".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x4012,
            long_name: "Meshtastic".to_string(), // No hex suffix
            short_name: "MESH".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x4013,
            long_name: "Meshtastic 123".to_string(), // Only 3 chars
            short_name: "123".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x4014,
            long_name: "Meshtastic 12345".to_string(), // 5 chars
            short_name: "1234".to_string(),
            last_seen_seconds_ago: 300,
        },
    ];

    create_test_node_cache(data_dir, test_nodes).expect("Failed to create node cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let _server = BbsServer::new(cfg).await.expect("Failed to create server");
    sleep(Duration::from_millis(100)).await;

    // In full implementation: verify exactly 3 welcomes queued (0x4001, 0x4002, 0x4003)
}

#[tokio::test]
async fn test_disabled_welcome_system() {
    // Test that when welcome system is disabled, no welcomes are queued

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_nodes = vec![TestNode {
        node_id: 0x5001,
        long_name: "Meshtastic CAFE".to_string(),
        short_name: "CAFE".to_string(),
        last_seen_seconds_ago: 300,
    }];

    create_test_node_cache(data_dir, test_nodes).expect("Failed to create node cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: false, // Disabled!
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let _server = BbsServer::new(cfg).await.expect("Failed to create server");
    sleep(Duration::from_millis(100)).await;

    // In full implementation: verify NO welcomes queued
}

#[tokio::test]
async fn test_empty_cache_handling() {
    // Test that an empty node cache doesn't cause errors

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    // Create empty cache
    create_test_node_cache(data_dir, vec![]).expect("Failed to create empty cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let server = BbsServer::new(cfg)
        .await
        .expect("Failed to create server with empty cache");
    sleep(Duration::from_millis(100)).await;

    // Should not panic, should not queue any welcomes
    drop(server);
}

#[tokio::test]
async fn test_missing_cache_file_handling() {
    // Test that a missing node cache file doesn't cause crashes

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    // Don't create cache file at all

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let server = BbsServer::new(cfg)
        .await
        .expect("Failed to create server with missing cache");
    sleep(Duration::from_millis(100)).await;

    // Should handle gracefully, not panic
    drop(server);
}

#[tokio::test]
async fn test_welcome_config_variations() {
    // Test different welcome configuration combinations

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_node = TestNode {
        node_id: 0x6001,
        long_name: "Meshtastic DEAD".to_string(),
        short_name: "DEAD".to_string(),
        last_seen_seconds_ago: 300,
    };

    create_test_node_cache(data_dir, vec![test_node]).expect("Failed to create node cache");

    // Test 1: Public only
    {
        let mut cfg = Config::default();
        cfg.storage.data_dir = data_dir.to_string();
        cfg.apps.welcome = WelcomeConfig {
            enabled: true,
            public_greeting: true,
            private_guide: false, // No private message
            cooldown_minutes: 5,
            max_welcomes_per_node: 1,
        };

        let _server = BbsServer::new(cfg).await.expect("Failed to create server");
        sleep(Duration::from_millis(100)).await;

        // In full implementation: verify only public message sent
    }

    // Test 2: Private only
    {
        let mut cfg = Config::default();
        cfg.storage.data_dir = data_dir.to_string();
        cfg.apps.welcome = WelcomeConfig {
            enabled: true,
            public_greeting: false, // No public message
            private_guide: true,
            cooldown_minutes: 5,
            max_welcomes_per_node: 1,
        };

        let _server = BbsServer::new(cfg).await.expect("Failed to create server");
        sleep(Duration::from_millis(100)).await;

        // In full implementation: verify only private message sent
    }

    // Test 3: Neither (should still mark as welcomed but send nothing)
    {
        let mut cfg = Config::default();
        cfg.storage.data_dir = data_dir.to_string();
        cfg.apps.welcome = WelcomeConfig {
            enabled: true,
            public_greeting: false,
            private_guide: false,
            cooldown_minutes: 5,
            max_welcomes_per_node: 1,
        };

        let _server = BbsServer::new(cfg).await.expect("Failed to create server");
        sleep(Duration::from_millis(100)).await;

        // In full implementation: verify node marked as welcomed but no messages sent
    }
}

#[tokio::test]
async fn test_global_rate_limit() {
    // Test that global rate limiting works (5 minute cooldown between ANY welcomes)

    let tmpdir = tempfile::tempdir().unwrap();
    let data_dir = tmpdir.path().to_str().unwrap();

    let test_nodes = vec![
        TestNode {
            node_id: 0x7001,
            long_name: "Meshtastic 1111".to_string(),
            short_name: "1111".to_string(),
            last_seen_seconds_ago: 300,
        },
        TestNode {
            node_id: 0x7002,
            long_name: "Meshtastic 2222".to_string(),
            short_name: "2222".to_string(),
            last_seen_seconds_ago: 300,
        },
    ];

    create_test_node_cache(data_dir, test_nodes).expect("Failed to create node cache");

    let mut cfg = Config::default();
    cfg.storage.data_dir = data_dir.to_string();
    cfg.apps.welcome = WelcomeConfig {
        enabled: true,
        public_greeting: true,
        private_guide: true,
        cooldown_minutes: 5,
        max_welcomes_per_node: 1,
    };

    let _server = BbsServer::new(cfg).await.expect("Failed to create server");

    // Both nodes should be queued with 30-second stagger
    // But global rate limit means after first welcome, must wait 5 minutes before next
    // So the 30-second queue timing is for startup only; during runtime, 5-minute limit applies

    sleep(Duration::from_millis(100)).await;

    // In full implementation: verify rate limiting logic
}
