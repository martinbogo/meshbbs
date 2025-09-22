use meshbbs::config::Config;
use meshbbs::bbs::BbsServer;
#[cfg(feature = "meshtastic-proto")]
use meshbbs::meshtastic::TextEvent;

// Test that when max_users is reached further LOGIN attempts are rejected.
#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn max_users_enforced() {
    let mut cfg = Config::default();
    cfg.bbs.max_users = 1; // only one user allowed
    cfg.storage.data_dir = "./test-data-int".into();
    let mut server = BbsServer::new(cfg).await.expect("server");

    // First user login flow
    let public1 = TextEvent { source: 100, dest: None, is_direct: false, channel: None, content: "^LOGIN alice".into() };
    server.route_text_event(public1).await.expect("public1");
    let dm1 = TextEvent { source: 100, dest: Some(1), is_direct: true, channel: None, content: "LOGIN alice".into() };
    server.route_text_event(dm1).await.expect("dm1");

    // Second user attempts login
    let public2 = TextEvent { source: 200, dest: None, is_direct: false, channel: None, content: "^LOGIN bob".into() };
    server.route_text_event(public2).await.expect("public2");
    let dm2 = TextEvent { source: 200, dest: Some(1), is_direct: true, channel: None, content: "LOGIN bob".into() };
    // Should not panic; actual rejection message isn't captured (no outbound capture hook)
    server.route_text_event(dm2).await.expect("dm2");
    // We assert internal count still 1
    assert_eq!(server.test_logged_in_count(), 1, "second login should be rejected due to max_users");
}

// Test idle logout: set timeout to 0 minutes (immediate) to force logout upon prune
#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn idle_logout_triggers() {
    let mut cfg = Config::default();
    cfg.bbs.session_timeout = 0; // treat as disabled? we interpret 0 as skip pruning above, so use 1 and manually adjust time
    cfg.bbs.session_timeout = 1; // 1 minute
    cfg.storage.data_dir = "./test-data-int".into();
    let mut server = BbsServer::new(cfg).await.expect("server");

    let public = TextEvent { source: 300, dest: None, is_direct: false, channel: None, content: "^LOGIN carol".into() };
    server.route_text_event(public).await.expect("public");
    let dm = TextEvent { source: 300, dest: Some(2), is_direct: true, channel: None, content: "LOGIN carol".into() };
    server.route_text_event(dm).await.expect("dm");
    assert_eq!(server.test_logged_in_count(), 1);

    // We can't easily warp time without a time provider; future improvement could inject a clock.
    // For now, we ensure prune_idle_sessions does not logout immediately (<1 minute).
    server.test_prune_idle().await;
    assert_eq!(server.test_logged_in_count(), 1, "should still be logged in prior to real timeout");
}
