/// Test that chunk markers config option works correctly
use meshbbs::bbs::server::BbsServer;
use meshbbs::config::Config;

#[tokio::test]
async fn test_chunk_markers_disabled_by_default() {
    let mut cfg = Config::default();
    cfg.storage.max_message_size = 50; // Force chunking with small size
    cfg.storage.show_chunk_markers = false; // Default: disabled

    let _server = BbsServer::new(cfg).await.expect("server should initialize");
}

#[tokio::test]
async fn test_chunk_markers_can_be_enabled() {
    let mut cfg = Config::default();
    cfg.storage.max_message_size = 50; // Force chunking with small size
    cfg.storage.show_chunk_markers = true; // ENABLED

    let _server = BbsServer::new(cfg).await.expect("server should initialize");
}

#[tokio::test]
async fn test_default_config_has_markers_disabled() {
    let cfg = Config::default();

    // Verify default is false (preserves backward compatibility)
    assert!(
        !cfg.storage.show_chunk_markers,
        "Chunk markers should be disabled by default"
    );
}
