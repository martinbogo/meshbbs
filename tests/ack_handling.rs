//! ACK handling smoke test placeholder: ensures metrics snapshot accessible after sending DM.

#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn ack_clears_pending() {
    use meshbbs::config::Config;

    // We exercise only the writer logic, constructing a MeshtasticWriter minimally.
    // Because MeshtasticWriter is not publicly exported we adapt by spinning a BbsServer and accessing control channel indirectly.
    // Simplify: send a DM through a BbsServer in mock mode (no device), then manually send AckReceived to writer control channel via exposed method if available.

    // Build config with meshtastic-proto assumptions (no actual serial port used)
    let mut cfg = Config::default();
    cfg.meshtastic.port = String::new(); // no device
    let mut server = meshbbs::bbs::BbsServer::new(cfg).await.expect("server");

    // Ensure scheduler exists by simulating device connect skip; we only need outgoing_tx
    // Send a DM
    server.send_message("123456789", "Test reliable DM").await.expect("send");

    // Placeholder: ensure metrics snapshot accessible.
    let snap_before = meshbbs::metrics::snapshot();
    // Read individual fields to exercise struct usage (avoids dead_code field warnings when tests are compiled).
    let _tot = snap_before.reliable_sent
        .saturating_add(snap_before.reliable_acked)
        .saturating_add(snap_before.reliable_failed)
        .saturating_add(snap_before.reliable_retries)
        .saturating_add(snap_before.ack_latency_avg_ms.unwrap_or(0));
    // Basic invariants: no acks or failures should be recorded yet; ack latency absent; acked cannot exceed sent.
    assert!(snap_before.reliable_acked <= snap_before.reliable_sent, "acked count exceeds sent count");
    assert_eq!(snap_before.reliable_failed, 0, "no failures expected in smoke path");
    assert!(snap_before.ack_latency_avg_ms.is_none(), "no ack latency expected without ack events");
}

#[cfg(not(feature = "meshtastic-proto"))]
#[test]
fn ack_clears_pending_noop() { /* feature gated no-op */ }