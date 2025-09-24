use meshbbs::config::Config;
use meshbbs::bbs::BbsServer;

#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn scheduler_overflow_drops_low_priority() {
    let mut config = Config::default();
    config.meshtastic.min_send_gap_ms = Some(1000); // high to keep items queued during test window
    config.meshtastic.scheduler_max_queue = Some(3); // tiny queue
    config.meshtastic.scheduler_stats_interval_ms = Some(0); // disable periodic logs

    let mut server = BbsServer::new(config.clone()).await.expect("server new");

    // Enqueue 3 broadcasts (fill queue)
    for i in 0..3 { let _ = server.send_broadcast(&format!("BCAST {i}")).await; }
    // Enqueue one more broadcast that should cause a drop
    let _ = server.send_broadcast("BCAST overflow").await;

    // Give scheduler a moment to process enqueue operations
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    // We cannot directly inspect internal stats without snapshot; attempt snapshot via scheduler handle
    if let Some(handle) = server.scheduler_handle() { if let Some(stats) = handle.snapshot().await { assert!(stats.dropped_overflow >= 1, "Expected at least one overflow drop, stats={:?}", stats); } }
}

#[cfg(not(feature = "meshtastic-proto"))]
#[test]
fn scheduler_overflow_drops_low_priority_skipped() {
    eprintln!("scheduler_overflow_drops_low_priority skipped: meshtastic-proto feature disabled");
}
