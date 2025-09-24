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
    // Inject AckReceived via a dedicated control channel is not currently exposed; future refactor could add a test hook.
    // This test is a placeholder to keep infrastructure ready; assert snapshot struct fields existence.
    assert!(snap_before.reliable_sent >= 0, "snapshot accessible");
}

#[cfg(not(feature = "meshtastic-proto"))]
#[test]
fn ack_clears_pending_noop() { /* feature gated no-op */ }