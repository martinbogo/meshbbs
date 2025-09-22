use meshbbs::config::Config;

// This test exercises the server direct message routing path for an immediate LOGIN command as the first DM.
// We build a Server (or minimal struct if accessible) via public API. If direct route_text_event not available without feature, we compile only when meshtastic-proto enabled.

#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn first_dm_login_immediate() {
    use meshbbs::bbs::server::BbsServer;
    use meshbbs::meshtastic::TextEvent;

    let mut cfg = Config::default();
    // Use temp dir for storage
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_str().unwrap().into();
    let mut server = BbsServer::new(cfg).await.unwrap();

    // Simulate incoming direct REGISTER command as very first DM
    let node_id: u32 = 0x1234;
    let ev = TextEvent { source: node_id, dest: None, is_direct: true, channel: None, content: "REGISTER testuser pass123".into() };
    server.route_text_event(ev).await.unwrap();

    // The server should have a session logged in as testuser
    let sess = server.test_get_session(&node_id.to_string()).expect("session created");
    assert!(sess.is_logged_in(), "Session should be logged in");
    assert_eq!(sess.username.as_deref(), Some("testuser"));
}
